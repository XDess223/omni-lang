// omni-vm/src/vm.rs
// Phase 5: Omni Stack-Based Virtual Machine
//
// Executes the bytecode Chunks produced by omni-compiler::codegen.
// Uses a call-frame stack, an operand stack, and the incremental GC.

use std::collections::HashMap;
use crate::gc::{GarbageCollector, HeapHandle, Value};
use omni_compiler::bytecode::{Chunk, CompiledProgram, Instruction};

// ── Call Frame ────────────────────────────────────────────────────────────────

/// One activation record on the call stack.
struct CallFrame {
    /// The chunk being executed.
    chunk: Chunk,
    /// Instruction pointer into `chunk.code`.
    ip: usize,
    /// Local variable slots.
    locals: Vec<Value>,
    /// The method key (for diagnostics).
    name: String,
}

impl CallFrame {
    fn new(chunk: Chunk, name: String, arg_count: usize) -> Self {
        // Pre-allocate enough slots (self + params + declared locals).
        let locals = vec![Value::Null; 256.max(arg_count + 1)];
        Self { chunk, ip: 0, locals, name }
    }

    fn read_instr(&mut self) -> Option<&Instruction> {
        let instr = self.chunk.code.get(self.ip)?;
        self.ip += 1;
        Some(instr)
    }
}

// ── VM Error ──────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum VmError {
    StackUnderflow,
    UndefinedMethod(String),
    UndefinedField(String),
    InvalidHandle(HeapHandle),
    CheckedExceptionUnhandled(String),
    TypeError(String),
    DivisionByZero,
    NullDereference,
}

// ── Virtual Machine ───────────────────────────────────────────────────────────

pub struct Vm {
    /// The compiled program (all method chunks + string pools).
    program: CompiledProgram,
    /// Operand value stack.
    stack: Vec<Value>,
    /// Call frame stack.
    frames: Vec<CallFrame>,
    /// The incremental garbage collector.
    pub gc: GarbageCollector,
    /// Global variables / static fields.
    globals: HashMap<String, Value>,
    /// Currently active exception (if any).
    current_exception: Option<Value>,
}

impl Vm {
    pub fn new(program: CompiledProgram) -> Self {
        Self {
            program,
            stack: Vec::with_capacity(256),
            frames: Vec::new(),
            gc: GarbageCollector::new(),
            globals: HashMap::new(),
            current_exception: None,
        }
    }

    // ── Stack helpers ─────────────────────────────────────────────────────

    fn push(&mut self, v: Value) {
        self.stack.push(v);
    }

    fn pop(&mut self) -> Result<Value, VmError> {
        self.stack.pop().ok_or(VmError::StackUnderflow)
    }

    fn peek(&self) -> Result<&Value, VmError> {
        self.stack.last().ok_or(VmError::StackUnderflow)
    }

    // ── GC integration ────────────────────────────────────────────────────

    /// Collect live roots from the operand stack and all locals.
    fn collect_roots(&self) -> Vec<HeapHandle> {
        let mut roots = Vec::new();
        for v in &self.stack {
            if let Value::Object(h) = v { roots.push(*h); }
        }
        for frame in &self.frames {
            for v in &frame.locals {
                if let Value::Object(h) = v { roots.push(*h); }
            }
        }
        roots
    }

    /// Run an incremental GC step if the threshold is exceeded.
    fn maybe_collect(&mut self) {
        if !self.gc.should_collect() {
            return;
        }
        // Begin a new cycle: seed roots.
        let roots = self.collect_roots();
        self.gc.mark_roots(&roots);
        // Mark one step per call to keep pauses minimal.
        if self.gc.mark_step() {
            self.gc.sweep();
        }
    }

    // ── Entry point ───────────────────────────────────────────────────────

    /// Execute a named method (e.g. "Main::main").
    pub fn execute(&mut self, method_key: &str) -> Result<Option<Value>, VmError> {
        let chunk = self.program.methods
            .get(method_key)
            .cloned()
            .ok_or_else(|| VmError::UndefinedMethod(method_key.to_string()))?;

        let frame = CallFrame::new(chunk, method_key.to_string(), 0);
        self.frames.push(frame);

        self.run()
    }

    // ── Main dispatch loop ────────────────────────────────────────────────

    fn run(&mut self) -> Result<Option<Value>, VmError> {
        loop {
            // Periodically trigger the incremental GC.
            self.maybe_collect();

            let frame = self.frames.last_mut()
                .ok_or(VmError::StackUnderflow)?;

            let instr = match frame.read_instr() {
                Some(i) => i.clone(),
                None    => return Ok(None), // fell off the end
            };

            match instr {

                // ── Literals ─────────────────────────────────────────────
                Instruction::PushInt(n)    => self.push(Value::Int(n)),
                Instruction::PushFloat(f)  => self.push(Value::Float(f)),
                Instruction::PushBool(b)   => self.push(Value::Bool(b)),
                Instruction::PushNull      => self.push(Value::Null),
                Instruction::PushString(i) => {
                    let frame = self.frames.last().unwrap();
                    let s = frame.chunk.strings.get(i as usize)
                        .cloned()
                        .unwrap_or_default();
                    drop(frame);
                    self.push(Value::Str(s));
                }

                // ── Stack ops ─────────────────────────────────────────────
                Instruction::Pop => { self.pop()?; }
                Instruction::Dup => {
                    let v = self.peek()?.clone();
                    self.push(v);
                }

                // ── Locals ────────────────────────────────────────────────
                Instruction::LoadLocal(slot) => {
                    let frame = self.frames.last().unwrap();
                    let v = frame.locals.get(slot as usize)
                        .cloned()
                        .unwrap_or(Value::Null);
                    drop(frame);
                    self.push(v);
                }
                Instruction::StoreLocal(slot) => {
                    let v = self.pop()?;
                    let frame = self.frames.last_mut().unwrap();
                    if slot as usize >= frame.locals.len() {
                        frame.locals.resize(slot as usize + 1, Value::Null);
                    }
                    frame.locals[slot as usize] = v;
                }

                // ── Fields ────────────────────────────────────────────────
                Instruction::LoadField(name_idx) => {
                    let frame = self.frames.last().unwrap();
                    let field_name = frame.chunk.names.get(name_idx as usize)
                        .cloned()
                        .unwrap_or_default();
                    drop(frame);
                    let obj_val = self.pop()?;
                    if let Value::Object(handle) = obj_val {
                        let val = self.gc.get(handle)
                            .ok_or(VmError::InvalidHandle(handle))?
                            .fields.get(&field_name)
                            .cloned()
                            .unwrap_or(Value::Null);
                        self.push(val);
                    } else {
                        return Err(VmError::NullDereference);
                    }
                }
                Instruction::StoreField(name_idx) => {
                    let frame = self.frames.last().unwrap();
                    let field_name = frame.chunk.names.get(name_idx as usize)
                        .cloned()
                        .unwrap_or_default();
                    drop(frame);
                    let obj_val = self.pop()?;
                    let new_val = self.pop()?;
                    if let Value::Object(handle) = obj_val {
                        if let Some(obj) = self.gc.get_mut(handle) {
                            obj.fields.insert(field_name, new_val);
                        }
                    }
                }

                // ── Arithmetic ────────────────────────────────────────────
                Instruction::AddInt => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    match (a, b) {
                        (Value::Int(x), Value::Int(y))     => self.push(Value::Int(x + y)),
                        (Value::Float(x), Value::Float(y)) => self.push(Value::Float(x + y)),
                        (Value::Str(x), Value::Str(y))     => self.push(Value::Str(x + &y)),
                        _ => return Err(VmError::TypeError("AddInt type mismatch".to_string())),
                    }
                }
                Instruction::SubInt => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    match (a, b) {
                        (Value::Int(x), Value::Int(y))     => self.push(Value::Int(x - y)),
                        (Value::Float(x), Value::Float(y)) => self.push(Value::Float(x - y)),
                        _ => return Err(VmError::TypeError("SubInt type mismatch".to_string())),
                    }
                }
                Instruction::MulInt => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    match (a, b) {
                        (Value::Int(x), Value::Int(y))     => self.push(Value::Int(x * y)),
                        (Value::Float(x), Value::Float(y)) => self.push(Value::Float(x * y)),
                        _ => return Err(VmError::TypeError("MulInt type mismatch".to_string())),
                    }
                }
                Instruction::DivInt => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    match (a, b) {
                        (Value::Int(_, ), Value::Int(0))   => return Err(VmError::DivisionByZero),
                        (Value::Int(x), Value::Int(y))     => self.push(Value::Int(x / y)),
                        (Value::Float(x), Value::Float(y)) => self.push(Value::Float(x / y)),
                        _ => return Err(VmError::TypeError("DivInt type mismatch".to_string())),
                    }
                }
                Instruction::ModInt => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    match (a, b) {
                        (Value::Int(_, ), Value::Int(0))   => return Err(VmError::DivisionByZero),
                        (Value::Int(x), Value::Int(y))     => self.push(Value::Int(x % y)),
                        _ => return Err(VmError::TypeError("ModInt type mismatch".to_string())),
                    }
                }

                // For Float-specific ops: treat same as Int variants above.
                Instruction::AddFloat | Instruction::AddStr => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    match (a, b) {
                        (Value::Float(x), Value::Float(y)) => self.push(Value::Float(x + y)),
                        (Value::Str(x), Value::Str(y))     => self.push(Value::Str(x + &y)),
                        _ => return Err(VmError::TypeError("Add type mismatch".to_string())),
                    }
                }
                Instruction::SubFloat => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    match (a, b) {
                        (Value::Float(x), Value::Float(y)) => self.push(Value::Float(x - y)),
                        _ => return Err(VmError::TypeError("SubFloat type mismatch".to_string())),
                    }
                }
                Instruction::MulFloat => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    match (a, b) {
                        (Value::Float(x), Value::Float(y)) => self.push(Value::Float(x * y)),
                        _ => return Err(VmError::TypeError("MulFloat type mismatch".to_string())),
                    }
                }
                Instruction::DivFloat => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    match (a, b) {
                        (Value::Float(x), Value::Float(y)) => self.push(Value::Float(x / y)),
                        _ => return Err(VmError::TypeError("DivFloat type mismatch".to_string())),
                    }
                }

                // ── Comparison ────────────────────────────────────────────
                Instruction::Eq => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    let eq = match (&a, &b) {
                        (Value::Int(x), Value::Int(y))   => x == y,
                        (Value::Bool(x), Value::Bool(y)) => x == y,
                        (Value::Str(x), Value::Str(y))   => x == y,
                        _                                 => false,
                    };
                    self.push(Value::Bool(eq));
                }
                Instruction::NotEq => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    let neq = match (&a, &b) {
                        (Value::Int(x), Value::Int(y))   => x != y,
                        (Value::Bool(x), Value::Bool(y)) => x != y,
                        (Value::Str(x), Value::Str(y))   => x != y,
                        _                                 => true,
                    };
                    self.push(Value::Bool(neq));
                }
                Instruction::LtInt => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    if let (Value::Int(x), Value::Int(y)) = (a, b) {
                        self.push(Value::Bool(x < y));
                    }
                }
                Instruction::LtEqInt => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    if let (Value::Int(x), Value::Int(y)) = (a, b) {
                        self.push(Value::Bool(x <= y));
                    }
                }
                Instruction::GtInt => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    if let (Value::Int(x), Value::Int(y)) = (a, b) {
                        self.push(Value::Bool(x > y));
                    }
                }
                Instruction::GtEqInt => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    if let (Value::Int(x), Value::Int(y)) = (a, b) {
                        self.push(Value::Bool(x >= y));
                    }
                }
                Instruction::And => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    if let (Value::Bool(x), Value::Bool(y)) = (a, b) {
                        self.push(Value::Bool(x && y));
                    }
                }
                Instruction::Or => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    if let (Value::Bool(x), Value::Bool(y)) = (a, b) {
                        self.push(Value::Bool(x || y));
                    }
                }
                Instruction::Not => {
                    let a = self.pop()?;
                    if let Value::Bool(b) = a {
                        self.push(Value::Bool(!b));
                    }
                }

                // ── Control Flow ──────────────────────────────────────────
                Instruction::Jump(ip) => {
                    self.frames.last_mut().unwrap().ip = ip as usize;
                }
                Instruction::JumpIfFalse(ip) => {
                    let cond = self.pop()?;
                    if let Value::Bool(false) = cond {
                        self.frames.last_mut().unwrap().ip = ip as usize;
                    }
                }
                Instruction::JumpIfTrue(ip) => {
                    let cond = self.pop()?;
                    if let Value::Bool(true) = cond {
                        self.frames.last_mut().unwrap().ip = ip as usize;
                    }
                }

                // ── Calls ─────────────────────────────────────────────────
                Instruction::Call { name_idx, argc } => {
                    let frame = self.frames.last().unwrap();
                    let fn_name = frame.chunk.names.get(name_idx as usize)
                        .cloned()
                        .unwrap_or_default();
                    drop(frame);

                    if let Some(chunk) = self.program.methods.get(&fn_name).cloned() {
                        let mut new_frame = CallFrame::new(chunk, fn_name, argc as usize);
                        // Pop arguments from operand stack into local slots 0..argc.
                        for i in (0..argc as usize).rev() {
                            new_frame.locals[i] = self.pop()?;
                        }
                        self.frames.push(new_frame);
                    } else if fn_name == "print" {
                        // Built-in print handles variable arity, but we assume 1 for simple cases here,
                        // or pop `argc` items and print them. Let's just pop `argc` items.
                        let mut args = Vec::new();
                        for _ in 0..argc {
                            args.push(self.pop()?);
                        }
                        args.reverse(); // Because we pop from last to first argument
                        
                        for (i, val) in args.iter().enumerate() {
                            if i > 0 { print!(" "); }
                            match val {
                                Value::Int(n) => print!("{}", n),
                                Value::Float(f) => print!("{}", f),
                                Value::Bool(b) => print!("{}", b),
                                Value::Str(s) => print!("{}", s),
                                Value::Null => print!("null"),
                                Value::Object(_) => print!("[Object]"),
                                Value::Closure(c) => print!("[Closure {}]", c),
                            }
                        }
                        println!();
                        // Push a dummy return value so the surrounding ExprStmt has something to Pop.
                        self.push(Value::Null);
                    } else {
                        // Optional: return an error for undefined methods instead of ignoring
                        return Err(VmError::UndefinedMethod(fn_name));
                    }
                }

                Instruction::InvokeVirtual { name_idx, argc } => {
                    let frame = self.frames.last().unwrap();
                    let method_name = frame.chunk.names.get(name_idx as usize)
                        .cloned()
                        .unwrap_or_default();
                    drop(frame);

                    let receiver = self.pop()?;
                    if let Value::Object(handle) = &receiver {
                        let class_name = self.gc.get(*handle)
                            .map(|obj| obj.class_name.clone())
                            .unwrap_or_default();
                        let key = format!("{}::{}", class_name, method_name);
                        if let Some(chunk) = self.program.methods.get(&key).cloned() {
                            let mut new_frame = CallFrame::new(chunk, key, argc as usize + 1);
                            new_frame.locals[0] = receiver; // slot 0 = self
                            for i in (1..=argc as usize).rev() {
                                new_frame.locals[i] = self.pop()?;
                            }
                            self.frames.push(new_frame);
                        } else {
                            return Err(VmError::UndefinedMethod(key));
                        }
                    }
                }

                Instruction::Return => {
                    self.frames.pop();
                    if self.frames.is_empty() {
                        return Ok(None);
                    }
                }
                Instruction::ReturnValue => {
                    let retval = self.pop()?;
                    self.frames.pop();
                    if self.frames.is_empty() {
                        return Ok(Some(retval));
                    }
                    self.push(retval);
                }

                // ── Object Allocation ─────────────────────────────────────
                Instruction::New { class_idx, argc } => {
                    let frame = self.frames.last().unwrap();
                    let class_name = frame.chunk.names.get(class_idx as usize)
                        .cloned()
                        .unwrap_or_default();
                    drop(frame);

                    let ctor_key = format!("{}::{}", class_name, class_name);
                    let mut args = Vec::new();
                    for _ in 0..argc {
                        args.push(self.pop()?);
                    }
                    args.reverse();

                    let handle = self.gc.allocate(&class_name);
                    let obj_val = Value::Object(handle);

                    // Push the newly allocated object for the caller
                    self.push(obj_val.clone());

                    if let Some(chunk) = self.program.methods.get(&ctor_key).cloned() {
                        let mut ctor_frame = CallFrame::new(chunk, ctor_key, argc as usize + 1);
                        ctor_frame.locals[0] = obj_val;
                        // Map args into local slots (1-indexed for explicit args)
                        for (i, arg) in args.into_iter().enumerate() {
                            ctor_frame.locals[i + 1] = arg;
                        }
                        self.frames.push(ctor_frame);
                    }
                }

                // ── Exception Handling ────────────────────────────────────
                Instruction::Throw => {
                    let exc = self.pop()?;
                    self.current_exception = Some(exc);
                    // Unwind to nearest TryBegin.
                    // Full unwind logic traverses the frame stack.
                }
                Instruction::TryBegin { handler_ip: _ } => {
                    // In a full implementation, push an exception handler record.
                    // handler_ip is already embedded for the sweep phase.
                }
                Instruction::TryEnd { past_ip } => {
                    // Jump past all catch blocks on the normal (no-exception) path.
                    self.frames.last_mut().unwrap().ip = past_ip as usize;
                }
                Instruction::CatchMatch { class_idx: _, local_slot } => {
                    if let Some(exc) = self.current_exception.take() {
                        let frame = self.frames.last_mut().unwrap();
                        frame.locals[local_slot as usize] = exc;
                    }
                }
                Instruction::Rethrow => {
                    if self.current_exception.is_some() {
                        // Propagate up — in a full impl this unwinds the frame stack.
                    }
                }

                // ── Concurrency stubs ─────────────────────────────────────
                Instruction::MonitorEnter => {
                    // Acquire mutex on the object at top of stack.
                    // Full implementation uses std::sync::Mutex per HeapObject.
                    let _ = self.peek()?;
                }
                Instruction::MonitorExit => {
                    // Release mutex — stub for Phase 5+ threading implementation.
                }
                Instruction::ForallBegin { .. } => {
                    // Signal VM scheduler to dispatch iterations in parallel.
                    // Stub: sequential execution for now.
                }
                Instruction::ForallEnd => {}

                Instruction::Nop  => {}
                Instruction::Halt => return Ok(None),

                // Float ops not matched above — handled by AddInt's polymorphism.
                _ => {}
            }
        }
    }
}
