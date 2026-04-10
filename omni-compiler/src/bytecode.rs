// omni-compiler/src/bytecode.rs
// Phase 4: Omni Bytecode Instruction Set
//
// Omni compiles to a compact, stack-based bytecode.
// The VM maintains an operand stack — instructions push/pop values,
// exactly like the JVM or CPython's bytecode format.

/// A single Omni bytecode instruction.
/// Each variant encodes all operands it needs inline (no separate operand stream).
#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    // ── Stack Manipulation ────────────────────────────────────────────────
    /// Push a constant integer onto the stack.
    PushInt(i64),
    /// Push a constant float onto the stack.
    PushFloat(f64),
    /// Push a constant string (interned index) onto the stack.
    PushString(u32),       // index into the string constant pool
    /// Push a boolean.
    PushBool(bool),
    /// Push the special Null sentinel.
    PushNull,
    /// Discard the top of stack.
    Pop,
    /// Duplicate the top of stack.
    Dup,

    // ── Local Variable Operations ─────────────────────────────────────────
    /// Load a local variable by index and push its value.
    LoadLocal(u16),
    /// Pop the top of stack and store it in a local slot.
    StoreLocal(u16),
    /// Load a field on the object at the top of the stack.
    LoadField(u32),        // field name index in constant pool
    /// Store into a field: (value, object) → object.field = value
    StoreField(u32),

    Add, Sub, Mul, Div, Mod,   // Generic arithmetic
    AddInt, SubInt, MulInt, DivInt, ModInt, // Specialized for performance (optional)
    AddFloat, SubFloat, MulFloat, DivFloat,
    AddStr,                // string concatenation
    Eq, NotEq,
    Lt, LtEq, Gt, GtEq,     // Generic comparisons
    LtInt, LtEqInt, GtInt, GtEqInt,
    LtFloat, LtEqFloat, GtFloat, GtEqFloat,
    And, Or, Not,

    // ── Control Flow ─────────────────────────────────────────────────────
    /// Unconditional jump to absolute instruction index.
    Jump(u32),
    /// Jump if top-of-stack is false (pop the condition).
    JumpIfFalse(u32),
    /// Jump if top-of-stack is true (pop the condition).
    JumpIfTrue(u32),

    // ── Function / Method Calls ───────────────────────────────────────────
    /// Call a global function by name-pool index, with `argc` arguments.
    Call { name_idx: u32, argc: u8 },
    /// Call a virtual method on `self` (top of stack = receiver), dispatch by name.
    InvokeVirtual { name_idx: u32, argc: u8 },
    /// Call a closure value popped from the stack.
    CallClosure { argc: u8 },
    /// Return from the current call frame (no value).
    Return,
    /// Return the top-of-stack value from the current call frame.
    ReturnValue,

    // ── Object / Closure Lifecycle ───────────────────────────────────────
    /// Allocate a new heap object by class name index, call constructor with argc args.
    New { class_idx: u32, argc: u8 },
    /// Create a closure binding the current local frame environment to the target anonymous method.
    MakeClosure { name_idx: u32, base_slot: u16 },

    // ── Array Operations ────────────────────────────────────────────────
    /// Allocate a new array. Pops `dims` size values from stack.
    NewArray { class_idx: u32, dims: u32 },
    /// Load from an array: pops `dims` indices, then `array` ref.
    ALoad { dims: u32 },
    /// Store to an array: pops `value`, then `dims` indices, then `array` ref.
    AStore { dims: u32 },

    // ── Exception Handling ────────────────────────────────────────────────
    /// Mark the beginning of a protected try region with a catch handler.
    TryBeginCatch { handler_ip: u32 },
    /// Mark the beginning of a protected region with a finally handler.
    TryBeginFinally { handler_ip: u32 },
    /// End the try region (normal path — jump past handlers).
    TryEnd { past_ip: u32 },
    /// Check the top-of-stack exception against a class name.
    /// If it matches, bind to a local slot and continue; else jump to next_ip.
    CatchMatch { class_idx: u32, local_slot: u16, next_ip: u32 },
    /// Re-raise the current exception if it was not matched.
    Rethrow,
    /// Throw the exception at the top of the stack.
    Throw,
    /// Signifies the end of a finally block; resumes pending return/exception.
    EndFinally,

    // ── Concurrency (Parallelism & Monitors) ─────────────────────────────
    /// Acquire the monitor lock on the object at top-of-stack.
    MonitorEnter,
    /// Release the monitor lock on the object at top-of-stack.
    MonitorExit,

    /// Execute a parallel range loop. Pops: end, start, closure.
    ExecuteForall,

    // ── Misc ──────────────────────────────────────────────────────────────
    /// Push high-performance reflection metadata (the type name) onto the stack.
    GetType,
    /// No-operation (used for patching jump targets during code generation).
    Nop,
    /// Halt execution (end of main program).
    Halt,
}

// ─────────────────────────────────────────────────────────────────────────────
// Bytecode Chunk — a compiled unit (one method / one class body)
// ─────────────────────────────────────────────────────────────────────────────

/// A compiled bytecode chunk for a single method or top-level scope.
#[derive(Debug, Clone, Default)]
pub struct Chunk {
    /// The linear instruction stream.
    pub code: Vec<Instruction>,
    /// Interned string constant pool (strings are deduplicated).
    pub strings: Vec<String>,
    /// Interned name pool for field/class/method names.
    pub names: Vec<String>,
    /// Source-line map: code[i] was generated from source line map[i].
    pub line_map: Vec<u32>,
    /// Total number of local variable slots required.
    pub local_count: u16,
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            local_count: 0,
            ..Self::default()
        }
    }

    /// Emit a single instruction and return its index.
    pub fn emit(&mut self, instr: Instruction, line: u32) -> u32 {
        let idx = self.code.len() as u32;
        self.code.push(instr);
        self.line_map.push(line);
        idx
    }

    /// Intern a string literal; returns its pool index.
    pub fn intern_string(&mut self, s: &str) -> u32 {
        if let Some(pos) = self.strings.iter().position(|x| x == s) {
            return pos as u32;
        }
        let idx = self.strings.len() as u32;
        self.strings.push(s.to_string());
        idx
    }

    /// Intern a name (class, field, or method); returns its pool index.
    pub fn intern_name(&mut self, name: &str) -> u32 {
        if let Some(pos) = self.names.iter().position(|x| x == name) {
            return pos as u32;
        }
        let idx = self.names.len() as u32;
        self.names.push(name.to_string());
        idx
    }

    /// Patch a previously emitted Jump/JumpIfFalse/TryBegin to point to
    /// the current instruction position.
    pub fn patch_jump(&mut self, jump_idx: u32) {
        let target = self.code.len() as u32;
        match &mut self.code[jump_idx as usize] {
            Instruction::Jump(ip)        => *ip = target,
            Instruction::JumpIfFalse(ip) => *ip = target,
            Instruction::JumpIfTrue(ip)  => *ip = target,
            Instruction::TryBeginCatch { handler_ip } => *handler_ip = target,
            Instruction::TryBeginFinally { handler_ip } => *handler_ip = target,
            Instruction::TryEnd { past_ip }      => *past_ip = target,
            Instruction::CatchMatch { next_ip, .. } => *next_ip = target,
            _ => {}
        }
    }

    /// Return the current instruction count (the next index to be emitted).
    pub fn current_ip(&self) -> u32 {
        self.code.len() as u32
    }
}

/// The full compiled output of one Omni source file.
#[derive(Debug, Clone, Default)]
pub struct CompiledProgram {
    /// One chunk per method (keyed as "ClassName::method_name").
    pub methods: std::collections::HashMap<String, Chunk>,
    /// Inheritance map: child class -> parent class.
    pub inheritance: std::collections::HashMap<String, String>,
    /// A special top-level chunk for static initializers / main entry.
    pub main_chunk: Chunk,
}
