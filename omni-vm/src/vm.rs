use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use rayon::prelude::*;
use crate::gc::{GarbageCollector, HeapHandle, Value};
use omni_compiler::bytecode::{Chunk, CompiledProgram, Instruction};

// ── Call Frame ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
enum Handler {
    Catch(u32),
    Finally(u32),
}

/// One activation record on the call stack.
struct CallFrame {
    /// The chunk being executed.
    chunk: Chunk,
    /// Instruction pointer into `chunk.code`.
    ip: usize,
    /// Local variable slots.
    locals: Vec<Value>,
    /// The method key (for diagnostics).
    #[allow(dead_code)]
    name: String,
    /// Active try block handlers (Catch or Finally).
    handlers: Vec<Handler>,
}

impl CallFrame {
    fn new(chunk: Chunk, name: String, arg_count: usize) -> Self {
        // Pre-allocate slots based on compiler's local count calculation and arguments.
        let size = (chunk.local_count as usize).max(arg_count).max(8);
        let locals = vec![Value::Null; size];
        Self { 
            chunk, 
            ip: 0, 
            locals, 
            name, 
            handlers: Vec::new() 
        }
    }

    fn read_instr(&mut self) -> Option<&Instruction> {
        let instr = self.chunk.code.get(self.ip)?;
        self.ip += 1;
        Some(instr)
    }
}

// ── VM Error ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
enum PendingAction {
    #[default]
    None,
    Return(Value),
    Throw(Value),
}

#[derive(Debug)]
pub enum VmError {
    StackUnderflow,
    UndefinedMethod(String),
    UndefinedField(String),
    InvalidHandle(HeapHandle),
    CheckedExceptionUnhandled(String),
    TypeError(String),
    DivisionByZero,
    NullDereference(String),
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
    pub gc: Arc<Mutex<GarbageCollector>>,
    /// Global variables / static fields.
    #[allow(dead_code)]
    globals: HashMap<String, Value>,
    /// Currently active exception (if any).
    current_exception: Option<Value>,
    /// Pending action to resume after finally block (if any).
    pending_action: PendingAction,
    /// Unique ID for this thread (0 = main).
    pub thread_id: i32,
}

impl Vm {
    pub fn new(program: CompiledProgram) -> Self {
        Self {
            program,
            stack: Vec::with_capacity(256),
            frames: Vec::new(),
            gc: Arc::new(Mutex::new(GarbageCollector::new())),
            globals: HashMap::new(),
            current_exception: None,
            pending_action: PendingAction::None,
            thread_id: 0,
        }
    }

    fn get_stack_trace(&self) -> String {
        self.frames.iter()
            .rev()
            .map(|f| f.name.clone())
            .collect::<Vec<_>>()
            .join(" -> ")
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
    fn gc_trace_roots(&mut self) {
        let mut gc = self.gc.lock().unwrap();
        for val in &self.stack {
            gc.mark_value(val);
        }
        for frame in &self.frames {
            for val in &frame.locals {
                gc.mark_value(val);
            }
        }
    }

    /// Run an incremental GC step if the threshold is exceeded.
    fn maybe_collect(&mut self) {
        if !self.gc.lock().unwrap().should_collect() {
            return;
        }
        // Begin a new cycle: seed roots.
        self.gc_trace_roots();
        // Mark one step per call to keep pauses minimal.
        let mut gc = self.gc.lock().unwrap();
        if gc.mark_step() {
            gc.sweep();
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

        self.run().map(Some)
    }

    // ── Main dispatch loop ────────────────────────────────────────────────

    pub fn run(&mut self) -> Result<Value, VmError> {
        loop {
            // Only the main thread triggers incremental GC steps.
            if self.thread_id == 0 {
                self.maybe_collect();
            }

            let frame = self.frames.last_mut()
                .ok_or(VmError::StackUnderflow)?;

            let instr = match frame.read_instr() {
                Some(i) => i.clone(),
                None    => return Ok(Value::Null), // fell off the end
            };

            // TRACE LOGGING
            if std::env::var("OMNI_TRACE").is_ok() {
                println!("TRACE: {:?}", instr);
            }

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
                    let obj_val = self.pop()?;
                    if let Value::Object(handle) = obj_val {
                        let val = self.gc.lock().unwrap().get(handle)
                            .ok_or(VmError::InvalidHandle(handle))?
                            .fields.get(&field_name)
                            .cloned()
                            .unwrap_or(Value::Null);
                        self.push(val);
                    } else {
                        let stack_trace = self.get_stack_trace();
                        return Err(VmError::NullDereference(format!("Attempted to load field '{}' from {:?}. Trace: {}", field_name, obj_val, stack_trace)));
                    }
                }
                Instruction::StoreField(name_idx) => {
                    let frame = self.frames.last().unwrap();
                    let field_name = frame.chunk.names.get(name_idx as usize)
                        .cloned()
                        .unwrap_or_default();
                    let obj_val = self.pop()?;
                    let new_val = self.pop()?;
                    if let Value::Object(handle) = obj_val {
                        if let Some(obj) = self.gc.lock().unwrap().get_mut(handle) {
                            obj.fields.insert(field_name, new_val);
                        }
                    } else {
                        let stack_trace = self.get_stack_trace();
                        return Err(VmError::NullDereference(format!("Attempted to store into field '{}' of {:?}. Trace: {}", field_name, obj_val, stack_trace)));
                    }
                }

                // Arithmetic instructions are handled below in a unified block
                Instruction::AddInt | Instruction::AddFloat | Instruction::Add | Instruction::AddStr => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    match (&a, &b) {
                        (Value::Int(x), Value::Int(y))     => self.push(Value::Int(x + y)),
                        (Value::Float(x), Value::Float(y)) => self.push(Value::Float(x + y)),
                        (Value::Int(x), Value::Float(y))   => self.push(Value::Float(*x as f64 + *y)),
                        (Value::Float(x), Value::Int(y))   => self.push(Value::Float(*x + *y as f64)),
                        (Value::Str(x), Value::Str(y))     => self.push(Value::Str(x.clone() + y)),
                        // String Coercion (Mixed Types)
                        (Value::Str(x), other)             => self.push(Value::Str(x.clone() + &other.to_string())),
                        (other, Value::Str(y))             => self.push(Value::Str(other.to_string() + y)),
                        _ => return Err(VmError::TypeError(format!("Addition type mismatch: cannot add {:?} and {:?}", a, b))),
                    }
                }
                Instruction::SubInt | Instruction::SubFloat | Instruction::Sub => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    match (&a, &b) {
                        (Value::Int(x), Value::Int(y))     => self.push(Value::Int(x - y)),
                        (Value::Float(x), Value::Float(y)) => self.push(Value::Float(x - y)),
                        (Value::Int(x), Value::Float(y))   => self.push(Value::Float(*x as f64 - *y)),
                        (Value::Float(x), Value::Int(y))   => self.push(Value::Float(*x - *y as f64)),
                        _ => return Err(VmError::TypeError(format!("Subtraction type mismatch: {:?} - {:?}", a, b))),
                    }
                }
                Instruction::MulInt | Instruction::MulFloat | Instruction::Mul => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    match (&a, &b) {
                        (Value::Int(x), Value::Int(y))     => self.push(Value::Int(x * y)),
                        (Value::Float(x), Value::Float(y)) => self.push(Value::Float(x * y)),
                        (Value::Int(x), Value::Float(y))   => self.push(Value::Float(*x as f64 * *y)),
                        (Value::Float(x), Value::Int(y))   => self.push(Value::Float(*x * *y as f64)),
                        _ => return Err(VmError::TypeError(format!("Multiplication type mismatch: {:?} * {:?}", a, b))),
                    }
                }
                Instruction::DivInt | Instruction::DivFloat | Instruction::Div => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    match (&a, &b) {
                        (_, Value::Int(0))                 => return Err(VmError::DivisionByZero),
                        (Value::Int(x), Value::Int(y))     => self.push(Value::Int(x / y)),
                        (Value::Float(x), Value::Float(y)) => self.push(Value::Float(x / y)),
                        (Value::Int(x), Value::Float(y))   => self.push(Value::Float(*x as f64 / *y)),
                        (Value::Float(x), Value::Int(y))   => self.push(Value::Float(*x / *y as f64)),
                        _ => return Err(VmError::TypeError(format!("Division type mismatch: {:?} / {:?}", a, b))),
                    }
                }
                Instruction::ModInt | Instruction::Mod => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    match (a, b) {
                        (Value::Int(_), Value::Int(0))   => return Err(VmError::DivisionByZero),
                        (Value::Int(x), Value::Int(y))     => self.push(Value::Int(x % y)),
                        _ => return Err(VmError::TypeError("Modulo type mismatch".to_string())),
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
                Instruction::LtInt | Instruction::LtFloat | Instruction::Lt => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    match (a, b) {
                        (Value::Int(x), Value::Int(y))     => self.push(Value::Bool(x < y)),
                        (Value::Float(x), Value::Float(y)) => self.push(Value::Bool(x < y)),
                        (Value::Int(x), Value::Float(y))   => self.push(Value::Bool((x as f64) < y)),
                        (Value::Float(x), Value::Int(y))   => self.push(Value::Bool(x < (y as f64))),
                        _ => return Err(VmError::TypeError("Lt comparison type mismatch".to_string())),
                    }
                }
                Instruction::LtEqInt | Instruction::LtEqFloat | Instruction::LtEq => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    match (a, b) {
                        (Value::Int(x), Value::Int(y))     => self.push(Value::Bool(x <= y)),
                        (Value::Float(x), Value::Float(y)) => self.push(Value::Bool(x <= y)),
                        (Value::Int(x), Value::Float(y))   => self.push(Value::Bool((x as f64) <= y)),
                        (Value::Float(x), Value::Int(y))   => self.push(Value::Bool(x <= (y as f64))),
                        _ => return Err(VmError::TypeError("LtEq comparison type mismatch".to_string())),
                    }
                }
                Instruction::GtInt | Instruction::GtFloat | Instruction::Gt => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    match (a, b) {
                        (Value::Int(x), Value::Int(y))     => self.push(Value::Bool(x > y)),
                        (Value::Float(x), Value::Float(y)) => self.push(Value::Bool(x > y)),
                        (Value::Int(x), Value::Float(y))   => self.push(Value::Bool((x as f64) > y)),
                        (Value::Float(x), Value::Int(y))   => self.push(Value::Bool(x > (y as f64))),
                        _ => return Err(VmError::TypeError("Gt comparison type mismatch".to_string())),
                    }
                }
                Instruction::GtEqInt | Instruction::GtEqFloat | Instruction::GtEq => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    match (a, b) {
                        (Value::Int(x), Value::Int(y))     => self.push(Value::Bool(x >= y)),
                        (Value::Float(x), Value::Float(y)) => self.push(Value::Bool(x >= y)),
                        (Value::Int(x), Value::Float(y))   => self.push(Value::Bool((x as f64) >= y)),
                        (Value::Float(x), Value::Int(y))   => self.push(Value::Bool(x >= (y as f64))),
                        _ => return Err(VmError::TypeError("GtEq comparison type mismatch".to_string())),
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
                                Value::Closure(c, _, _) => print!("[Closure {}]", c),
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
                    let mut args = Vec::new();
                    for _ in 0..argc {
                        args.push(self.pop()?);
                    }
                    args.reverse();

                    let receiver = self.pop()?;
                    let frame = self.frames.last().unwrap();
                    let method_name = frame.chunk.names.get(name_idx as usize)
                        .cloned()
                        .unwrap_or_default();

                    if let Value::Object(handle) = &receiver {
                        let class_name = self.gc.lock().unwrap().get(*handle)
                            .map(|obj| obj.class_name.clone())
                            .unwrap_or_default();
                        let mut current_class = class_name.clone();
                        let mut found = false;

                        while !current_class.is_empty() {
                            let key = format!("{}::{}", current_class, method_name);
                            if let Some(chunk) = self.program.methods.get(&key).cloned() {
                                let mut new_frame = CallFrame::new(chunk, key, argc as usize + 1);
                                new_frame.locals[0] = receiver.clone(); // slot 0 = self
                                for (i, arg) in args.iter().enumerate() {
                                    new_frame.locals[i + 1] = arg.clone();
                                }
                                self.frames.push(new_frame);
                                found = true;
                                break;
                            }
                            
                            // Move to parent class
                            if let Some(parent) = self.program.inheritance.get(&current_class) {
                                current_class = parent.clone();
                            } else {
                                break;
                            }
                        }

                        if found {
                            // Already handled
                        } else if class_name == "List" {
                            // Use the `args` we already popped

                            let mut result = None;
                            let mut err = None;

                            {
                                let mut gc = self.gc.lock().unwrap();
                                let obj_ref = gc.get_mut(*handle).unwrap();
                                let elements = obj_ref.elements.as_mut().unwrap();

                                match method_name.as_str() {
                                    "add" => {
                                        if args.len() == 1 {
                                            elements.push(args[0].clone());
                                            result = Some(Value::Null);
                                        } else {
                                            err = Some(VmError::UndefinedMethod("List::add requires 1 argument".to_string()));
                                        }
                                    }
                                    "get" => {
                                        if args.len() == 1 {
                                            if let Value::Int(idx) = &args[0] {
                                                if *idx >= 0 && (*idx as usize) < elements.len() {
                                                    result = Some(elements[*idx as usize].clone());
                                                } else {
                                                    err = Some(VmError::TypeError("Index out of bounds".to_string()));
                                                }
                                            } else {
                                                err = Some(VmError::TypeError("List::get requires an integer index".to_string()));
                                            }
                                        } else {
                                            err = Some(VmError::UndefinedMethod("List::get requires 1 argument".to_string()));
                                        }
                                    }
                                    "size" => {
                                        if args.len() == 0 {
                                            result = Some(Value::Int(elements.len() as i64));
                                        } else {
                                            err = Some(VmError::UndefinedMethod("List::size requires 0 arguments".to_string()));
                                        }
                                    }
                                    _ => err = Some(VmError::UndefinedMethod(format!("{}::{}", class_name, method_name))),
                                }
                            }

                            if let Some(e) = err {
                                return Err(e);
                            }
                            if let Some(res) = result {
                                self.push(res);
                            }
                        } else {
                            return Err(VmError::UndefinedMethod(format!("{}::{}", class_name, method_name)));
                        }
                    } else {
                        return Err(VmError::NullDereference(format!("Attempted to invoke virtual method '{}' on {:?}", method_name, receiver)));
                    }
                }

                Instruction::Return => {
                    let frame = self.frames.last_mut().unwrap();
                    // Check for finally blocks in the current frame
                    while let Some(handler) = frame.handlers.pop() {
                        if let Handler::Finally(ip) = handler {
                            self.pending_action = PendingAction::Return(Value::Null);
                            frame.ip = ip as usize;
                            continue; // We stay in this frame for now
                        }
                    }
                    
                    self.frames.pop();
                    if self.frames.is_empty() {
                        return Ok(Value::Null);
                    }
                }
                Instruction::ReturnValue => {
                    let retval = self.pop()?;
                    let frame = self.frames.last_mut().unwrap();
                    
                    // Check for finally blocks in the current frame
                    while let Some(handler) = frame.handlers.pop() {
                        if let Handler::Finally(ip) = handler {
                            self.pending_action = PendingAction::Return(retval.clone());
                            frame.ip = ip as usize;
                            continue; // Value is irrelevant while unwinding
                        }
                    }

                    self.frames.pop();
                    if self.frames.is_empty() {
                        return Ok(retval);
                    }
                    self.push(retval);
                }

                // ── Object / Closure Allocation ───────────────────────────
                Instruction::MakeClosure { name_idx, base_slot } => {
                    let frame = self.frames.last().unwrap();
                    let closure_key = frame.chunk.names.get(name_idx as usize)
                        .cloned()
                        .unwrap_or_default();
                    let env = frame.locals.clone();
                    self.push(Value::Closure(closure_key, env, base_slot));
                }

                Instruction::CallClosure { argc } => {
                    let mut args = Vec::new();
                    for _ in 0..argc {
                        args.push(self.pop()?);
                    }
                    args.reverse();

                    let closure_val = self.pop()?;

                    if let Value::Closure(closure_key, env, base_slot) = closure_val {
                        if let Some(chunk) = self.program.methods.get(&closure_key).cloned() {
                            let mut new_frame = CallFrame::new(chunk, closure_key, base_slot as usize + argc as usize);
                            // Restore the captured environment (this gives the closure access to the parent's locals!)
                            for (i, val) in env.into_iter().enumerate() {
                                new_frame.locals[i] = val;
                            }
                            // Callers pass arguments; map them to the corresponding arguments offsets
                            for (i, arg) in args.iter().enumerate() {
                                new_frame.locals[base_slot as usize + i] = arg.clone();
                            }
                            self.frames.push(new_frame);
                        } else {
                            return Err(VmError::UndefinedMethod(format!("Closure method {} not found", closure_key)));
                        }
                    } else {
                        return Err(VmError::TypeError("Attempted to call a non-closure value".to_string()));
                    }
                }

                // ── Object Allocation ─────────────────────────────────────
                Instruction::New { class_idx, argc } => {
                    let frame = self.frames.last().unwrap();
                    let class_name = frame.chunk.names.get(class_idx as usize)
                        .cloned()
                        .unwrap_or_default();

                    let ctor_key = format!("{}::{}", class_name, class_name);
                    let mut args = Vec::new();
                    for _ in 0..argc {
                        args.push(self.pop()?);
                    }
                    args.reverse();

                    let handle = self.gc.lock().unwrap().allocate(&class_name);
                    let obj_val = Value::Object(handle);
                    self.push(obj_val.clone()); // Push to stack now so it's there after constructor returns

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

                // ── Array Operations ──────────────────────────────────────
                Instruction::NewArray { class_idx, dims } => {
                    let frame = self.frames.last().unwrap();
                    let class_name = frame.chunk.names.get(class_idx as usize)
                        .cloned()
                        .unwrap_or_default();

                    let mut sizes = Vec::new();
                    for _ in 0..dims {
                        if let Value::Int(s) = self.pop()? {
                            sizes.push(s as usize);
                        } else {
                            return Err(VmError::TypeError("Array dimension must be integer".to_string()));
                        }
                    }
                    sizes.reverse(); // Popped in reverse order (dimN...dim1)

                    let total_size: usize = sizes.iter().product();
                    let handle = {
                        let mut gc = self.gc.lock().unwrap();
                        let handle = gc.allocate(&class_name);
                        let obj = gc.get_mut(handle).unwrap();
                        obj.elements = Some(vec![Value::Null; total_size]);
                        obj.dimensions = Some(sizes);
                        handle
                    };
                    
                    self.push(Value::Object(handle));
                }

                Instruction::ALoad { dims } => {
                    let mut indices = Vec::new();
                    for _ in 0..dims {
                        if let Value::Int(i) = self.pop()? {
                            indices.push(i as usize);
                        } else {
                            return Err(VmError::TypeError("Array index must be integer".to_string()));
                        }
                    }
                    indices.reverse();

                    let array_val = self.pop()?;
                    if let Value::Object(handle) = array_val {
                        let val = {
                            let gc = self.gc.lock().unwrap();
                            let obj = gc.get(handle).ok_or(VmError::InvalidHandle(handle))?;
                            
                            let dimensions = obj.dimensions.as_ref().ok_or(VmError::TypeError("Not an array".to_string()))?;
                            let elements = obj.elements.as_ref().unwrap();

                            if indices.len() != dimensions.len() {
                                return Err(VmError::TypeError(format!("Array dimension mismatch: expected {}, got {}", dimensions.len(), indices.len())));
                            }

                            let mut flat_index = 0;
                            let mut stride = 1;
                            for (item_idx, (&idx, &dim)) in indices.iter().zip(dimensions.iter()).rev().enumerate() {
                                if idx >= dim {
                                    return Err(VmError::TypeError(format!("Array index out of bounds: index {} is {} but dimension is {}", indices.len() - 1 - item_idx, idx, dim)));
                                }
                                flat_index += idx * stride;
                                stride *= dim;
                            }

                            elements[flat_index].clone()
                        };

                        self.push(val);
                    } else {
                        return Err(VmError::NullDereference("Attempted ALoad on null or non-object".to_string()));
                    }
                }

                Instruction::AStore { dims } => {
                    let value = self.pop()?;
                    let mut indices = Vec::new();
                    for _ in 0..dims {
                        if let Value::Int(i) = self.pop()? {
                            indices.push(i as usize);
                        } else {
                            return Err(VmError::TypeError("Array index must be integer".to_string()));
                        }
                    }
                    indices.reverse();

                    let array_val = self.pop()?;
                    if let Value::Object(handle) = array_val {
                        let mut gc = self.gc.lock().unwrap();
                        let obj = gc.get_mut(handle).ok_or(VmError::InvalidHandle(handle))?;
                        
                        let dimensions = obj.dimensions.as_ref().ok_or(VmError::TypeError("Not an array".to_string()))?;
                        let elements = obj.elements.as_mut().unwrap();

                        if indices.len() != dimensions.len() {
                            return Err(VmError::TypeError(format!("Array dimension mismatch: expected {}, got {}", dimensions.len(), indices.len())));
                        }

                        let mut flat_index = 0;
                        let mut stride = 1;
                        for (item_idx, (&idx, &dim)) in indices.iter().zip(dimensions.iter()).rev().enumerate() {
                            if idx >= dim {
                                return Err(VmError::TypeError(format!("Array index out of bounds: index {} is {} but dimension is {}", indices.len() - 1 - item_idx, idx, dim)));
                            }
                            flat_index += idx * stride;
                            stride *= dim;
                        }

                        elements[flat_index] = value;
                    } else {
                        return Err(VmError::NullDereference("Attempted AStore on null or non-object".to_string()));
                    }
                }

                // ── Exception Handling ────────────────────────────────────
                Instruction::Throw => {
                    let exc = self.pop()?;
                    self.current_exception = Some(exc.clone());
                    
                    loop {
                        let frame = match self.frames.last_mut() {
                            Some(f) => f,
                            None => break,
                        };
                        
                        if let Some(handler) = frame.handlers.pop() {
                            match handler {
                                Handler::Catch(ip) => {
                                    frame.ip = ip as usize;
                                    break;
                                }
                                Handler::Finally(ip) => {
                                    self.pending_action = PendingAction::Throw(exc.clone());
                                    frame.ip = ip as usize;
                                    break;
                                }
                            }
                        } else {
                            self.frames.pop();
                        }
                    }

                    if self.frames.is_empty() {
                        let msg = match self.current_exception.as_ref().unwrap() {
                            Value::Object(h) => self.gc.lock().unwrap().get(*h).map(|o| o.class_name.clone()).unwrap_or_default(),
                            o => format!("{:?}", o),
                        };
                        return Err(VmError::CheckedExceptionUnhandled(msg));
                    }
                }
                Instruction::TryBeginCatch { handler_ip } => {
                    self.frames.last_mut().unwrap().handlers.push(Handler::Catch(handler_ip));
                }
                Instruction::TryBeginFinally { handler_ip } => {
                    self.frames.last_mut().unwrap().handlers.push(Handler::Finally(handler_ip));
                }
                Instruction::TryEnd { past_ip } => {
                    let frame = self.frames.last_mut().unwrap();
                    frame.handlers.pop(); 
                    frame.ip = past_ip as usize;
                }
                Instruction::CatchMatch { class_idx, local_slot, next_ip } => {
                    if let Some(exc) = &self.current_exception {
                        let frame = self.frames.last_mut().unwrap();
                        let target_class = frame.chunk.names.get(class_idx as usize).cloned().unwrap_or_default();
                        
                        let is_match = match exc {
                            Value::Object(h) => {
                                if let Some(obj) = self.gc.lock().unwrap().get(*h) {
                                    obj.class_name == target_class
                                } else { false }
                            },
                            _ => false,
                        };
                        
                        if is_match {
                            frame.locals[local_slot as usize] = self.current_exception.take().unwrap();
                        } else {
                            frame.ip = next_ip as usize;
                        }
                    } else {
                        self.frames.last_mut().unwrap().ip = next_ip as usize;
                    }
                }
                Instruction::Rethrow => {
                    if let Some(exc) = self.current_exception.clone() {
                        loop {
                            let frame = match self.frames.last_mut() {
                                Some(f) => f,
                                None => break,
                            };
                            
                            if let Some(handler) = frame.handlers.pop() {
                                match handler {
                                    Handler::Catch(ip) => {
                                        frame.ip = ip as usize;
                                        break;
                                    }
                                    Handler::Finally(ip) => {
                                        self.pending_action = PendingAction::Throw(exc.clone());
                                        frame.ip = ip as usize;
                                        break;
                                    }
                                }
                            } else {
                                self.frames.pop();
                            }
                        }
                    }
                    if self.frames.is_empty() {
                        let msg = "Rethrow reached top of stack".to_string();
                        return Err(VmError::CheckedExceptionUnhandled(msg));
                    }
                }
                Instruction::EndFinally => {
                    match std::mem::take(&mut self.pending_action) {
                        PendingAction::None => {} // Continue normally
                        PendingAction::Return(val) => {
                            // Resume return: check for OUTER finally blocks in SAME frame
                            let frame = self.frames.last_mut().unwrap();
                            if let Some(handler) = frame.handlers.pop() {
                                if let Handler::Finally(ip) = handler {
                                    self.pending_action = PendingAction::Return(val);
                                    frame.ip = ip as usize;
                                }
                            } else {
                                // Pop and return
                                self.frames.pop();
                                if self.frames.is_empty() {
                                    return Ok(val);
                                } else {
                                    // If we are NOT at the top of the stack, push the return value
                                    // and continue in the caller.
                                    self.push(val);
                                }
                            }
                        }
                        PendingAction::Throw(exc) => {
                            // Resume throw: search for next handler
                            self.current_exception = Some(exc.clone());
                            loop {
                                let frame = match self.frames.last_mut() {
                                    Some(f) => f,
                                    None => break,
                                };
                                
                                if let Some(handler) = frame.handlers.pop() {
                                    match handler {
                                        Handler::Catch(ip) => {
                                            frame.ip = ip as usize;
                                            break;
                                        }
                                        Handler::Finally(ip) => {
                                            self.pending_action = PendingAction::Throw(exc);
                                            frame.ip = ip as usize;
                                            break;
                                        }
                                    }
                                } else {
                                    self.frames.pop();
                                }
                            }
                            if self.frames.is_empty() {
                                let msg = match self.current_exception.as_ref().unwrap() {
                                    Value::Object(h) => self.gc.lock().unwrap().get(*h).map(|o| o.class_name.clone()).unwrap_or_default(),
                                    o => format!("{:?}", o),
                                };
                                return Err(VmError::CheckedExceptionUnhandled(msg));
                            }
                        }
                    }
                }
                Instruction::MonitorEnter => {
                    let val = self.pop()?;
                    if let Value::Object(h) = val {
                        let lock_owner = {
                            let gc = self.gc.lock().unwrap();
                            gc.get(h).map(|o| o.lock_owner.clone())
                        }.ok_or(VmError::InvalidHandle(h))?;

                        // Spin-lock for simplicity (in a real VM, we'd use a Futex/WaitQueue)
                        while lock_owner.compare_exchange(-1, self.thread_id, Ordering::SeqCst, Ordering::SeqCst).is_err() {
                            if lock_owner.load(Ordering::SeqCst) == self.thread_id {
                                // Reentrant lock support (optional but good)
                                break; 
                            }
                            std::thread::yield_now();
                        }
                    } else {
                        return Err(VmError::TypeError("Monitor requires an object".to_string()));
                    }
                }
                Instruction::MonitorExit => {
                    let val = self.pop()?;
                    if let Value::Object(h) = val {
                        let lock_owner = {
                            let gc = self.gc.lock().unwrap();
                            gc.get(h).map(|o| o.lock_owner.clone())
                        }.ok_or(VmError::InvalidHandle(h))?;

                        if lock_owner.load(Ordering::SeqCst) == self.thread_id {
                            lock_owner.store(-1, Ordering::SeqCst);
                        }
                    }
                }

                Instruction::GetType => {
                    let val = self.pop()?;
                    let type_name = match val {
                        Value::Int(_) => "Int".to_string(),
                        Value::Float(_) => "Float".to_string(),
                        Value::Bool(_) => "Bool".to_string(),
                        Value::Str(_) => "String".to_string(),
                        Value::Null => "Null".to_string(),
                        Value::Object(h) => {
                            let gc = self.gc.lock().unwrap();
                            gc.get(h).map(|o| o.class_name.clone()).unwrap_or_else(|| "Object".to_string())
                        }
                        Value::Closure(_, _, _) => "Closure".to_string(),
                    };
                    self.push(Value::Str(type_name));
                }

                Instruction::ExecuteForall => {
                    let closure_val = self.pop()?;
                    let end_val = self.pop()?;
                    let start_val = self.pop()?;

                    if let (Value::Int(start), Value::Int(end), Value::Closure(key, env, base)) = (start_val, end_val, closure_val) {
                        let program = self.program.clone();
                        let shared_gc = self.gc.clone();
                        
                        // Use rayon's worker pool for high-performance scoped parallelism
                        (start..=end).into_par_iter().for_each(|i| {
                            let key = key.clone();
                            let env = env.clone();
                            let prog = program.clone();
                            let gc = shared_gc.clone();
                            let t_id = (i + 1) as i32; // Unique worker ID
                            
                            let mut worker_vm = Vm::new(prog);
                            worker_vm.gc = gc;
                            worker_vm.thread_id = t_id;
                            
                            let chunk = worker_vm.program.methods.get(&key).expect("Closure method not found").clone();
                            let mut frame = CallFrame::new(chunk, key, base as usize + 1);
                            for (j, v) in env.into_iter().enumerate() {
                                frame.locals[j] = v;
                            }
                            frame.locals[base as usize] = Value::Int(i);
                            worker_vm.frames.push(frame);
                            
                            if let Err(e) = worker_vm.run() {
                                eprintln!("\n[Thread {}] ❌ Parallel worker error: {:?}", t_id, e);
                            }
                        });
                        self.push(Value::Null);
                    } else {
                        return Err(VmError::TypeError("Invalid forall arguments".to_string()));
                    }
                }

                Instruction::Nop  => {}
                Instruction::Halt => return Ok(Value::Null),
            }
        }
    }
}
