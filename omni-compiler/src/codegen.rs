// omni-compiler/src/codegen.rs
// Phase 4: Bytecode Code Generator
//
// Walks the semantically-verified AST and emits Omni bytecode Chunks.
// One Chunk is produced per method; all methods are stored in CompiledProgram.

use crate::ast::*;
use crate::bytecode::{Chunk, CompiledProgram, Instruction};
use std::collections::HashMap;

// ── Local variable slot allocator ─────────────────────────────────────────────

#[derive(Clone)]
struct Locals {
    /// Maps variable name → slot index.
    slots: Vec<HashMap<String, u16>>,
    next_slot: u16,
}

impl Locals {
    fn new() -> Self {
        Self {
            slots: vec![HashMap::new()],
            next_slot: 0,
        }
    }

    fn push_scope(&mut self) {
        self.slots.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.slots.pop();
    }

    fn declare(&mut self, name: &str) -> u16 {
        let slot = self.next_slot;
        self.slots
            .last_mut()
            .unwrap()
            .insert(name.to_string(), slot);
        self.next_slot += 1;
        slot
    }

    fn lookup(&self, name: &str) -> Option<u16> {
        for frame in self.slots.iter().rev() {
            if let Some(&s) = frame.get(name) {
                return Some(s);
            }
        }
        None
    }
}

// ── Code Generator ────────────────────────────────────────────────────────────

pub struct CodeGen {
    pub output: CompiledProgram,
    current_class: String,
    current_parent: Option<String>,
    forall_counter: u32,
}

impl CodeGen {
    pub fn new() -> Self {
        Self {
            output: CompiledProgram::default(),
            current_class: String::new(),
            current_parent: None,
            forall_counter: 0,
        }
    }

    // ── Entry point ───────────────────────────────────────────────────────

    pub fn generate(&mut self, program: &Program) {
        for class in &program.classes {
            self.gen_class(class);
        }
    }

    // ── Class ─────────────────────────────────────────────────────────────

    fn gen_class(&mut self, class: &ClassDef) {
        self.current_class = class.name.clone();
        self.current_parent = class.extends.clone();
        if let Some(parent) = &class.extends {
            self.output
                .inheritance
                .insert(class.name.clone(), parent.clone());
        }

        // Collect all field initializers to prepend them to constructors
        let mut field_inits = Vec::new();
        for member in &class.members {
            if let ClassMember::Field(_, decl) = member {
                if let Some(ref init) = decl.initializer {
                    field_inits.push((decl.name.clone(), init.clone()));
                }
            }
        }

        let mut constructor_found = false;
        for member in &class.members {
            if let ClassMember::Method(_, method) = member {
                if method.name == class.name {
                    constructor_found = true;
                    break;
                }
            }
        }

        // Generate methods
        for member in &class.members {
            if let ClassMember::Method(_, method) = member {
                let is_constructor = method.name == class.name;
                let key = format!("{}::{}", class.name, method.name);
                let chunk =
                    self.gen_method(method, if is_constructor { &field_inits } else { &[] });
                self.output.methods.insert(key, chunk);
            }
        }

        // If no constructor found but we have field initializers, generate a default constructor
        if !constructor_found && !field_inits.is_empty() {
            let default_constructor = MethodDef {
                name: class.name.clone(),
                params: Vec::new(),
                throws: Vec::new(),
                return_type: None,
                body: Vec::new(), // Empty body, but gen_method will add field inits
            };
            let key = format!("{}::{}", class.name, class.name);
            let chunk = self.gen_method(&default_constructor, &field_inits);
            self.output.methods.insert(key, chunk);
        }
    }

    // ── Method ────────────────────────────────────────────────────────────

    fn gen_method(&mut self, method: &MethodDef, field_inits: &[(String, Expr)]) -> Chunk {
        let mut chunk = Chunk::new();
        let mut locals = Locals::new();

        // Slot 0 is always `self` (the receiver), allocated implicitly.
        locals.declare("self");

        // Declare each parameter as successive local slots.
        for param in &method.params {
            locals.declare(&param.name);
        }

        // If this is a constructor, prepend field initializers
        for (field_name, init_expr) in field_inits {
            // self.field = initializer
            self.gen_expr(init_expr, &mut chunk, &mut locals);
            chunk.emit(Instruction::LoadLocal(0), 0);
            let fidx = chunk.intern_name(field_name);
            chunk.emit(Instruction::StoreField(fidx), 0);
        }

        self.gen_block(&method.body, &mut chunk, &mut locals);

        // Guarantee every method ends with a Return (prevents fall-through).
        if chunk.code.last() != Some(&Instruction::Return)
            && chunk.code.last() != Some(&Instruction::ReturnValue)
        {
            if method.name == self.current_class {
                // Constructors implicitly return `this` (local 0)
                chunk.emit(Instruction::LoadLocal(0), 0);
                chunk.emit(Instruction::ReturnValue, 0);
            } else {
                chunk.emit(Instruction::PushNull, 0);
                chunk.emit(Instruction::ReturnValue, 0);
            }
        }

        chunk.local_count = locals.next_slot as u16;
        chunk
    }

    // ── Block ─────────────────────────────────────────────────────────────

    fn gen_block(&mut self, block: &Block, chunk: &mut Chunk, locals: &mut Locals) {
        locals.push_scope();
        for stmt in block {
            self.gen_stmt(stmt, chunk, locals);
        }
        locals.pop_scope();
    }

    // ── Statements ────────────────────────────────────────────────────────

    fn gen_stmt(&mut self, stmt: &Stmt, chunk: &mut Chunk, locals: &mut Locals) {
        match stmt {
            // var x = expr  or  var x : Type = expr
            Stmt::VarDecl(decl) => {
                if let Some(ref init) = decl.initializer {
                    self.gen_expr(init, chunk, locals);
                } else {
                    chunk.emit(Instruction::PushNull, 0);
                }
                let slot = locals.declare(&decl.name);
                chunk.emit(Instruction::StoreLocal(slot), 0);
            }

            // target = value
            Stmt::Assign { target, value } => {
                match target {
                    Expr::ArrayAccess { array, indices } => {
                        // Standardize protocol: [array, indices..., value]
                        self.gen_expr(array, chunk, locals);
                        for idx in indices {
                            self.gen_expr(idx, chunk, locals);
                        }
                        self.gen_expr(value, chunk, locals);
                        chunk.emit(
                            Instruction::AStore {
                                dims: indices.len() as u32,
                            },
                            0,
                        );
                    }
                    _ => {
                        self.gen_expr(value, chunk, locals);
                        self.gen_assign_target(target, chunk, locals);
                    }
                }
            }

            // return [expr]
            Stmt::Return(opt_expr) => {
                if let Some(expr) = opt_expr {
                    self.gen_expr(expr, chunk, locals);
                    chunk.emit(Instruction::ReturnValue, 0);
                } else {
                    chunk.emit(Instruction::Return, 0);
                }
            }

            // if (cond) { then } [else { else }]
            // EBNF: <if_stmt> → if (<logic_expr>) <stmt> [else <stmt>]
            Stmt::If {
                cond,
                then_block,
                else_block,
            } => {
                self.gen_expr(cond, chunk, locals);

                // Emit a placeholder JumpIfFalse — we'll patch it once we know
                // where the else/end is.
                let jump_false_idx = chunk.emit(Instruction::JumpIfFalse(0), 0);

                self.gen_block(then_block, chunk, locals);

                if let Some(eb) = else_block {
                    // Jump past the else block at the end of the then block.
                    let jump_end_idx = chunk.emit(Instruction::Jump(0), 0);
                    chunk.patch_jump(jump_false_idx); // false → start of else
                    self.gen_block(eb, chunk, locals);
                    chunk.patch_jump(jump_end_idx); // end of else → here
                } else {
                    chunk.patch_jump(jump_false_idx); // false → after if
                }
            }

            // foreach (var in collection) { body }
            // EBNF: <iteration_stmt> → foreach (<id> in <collection>) <block>
            // Emitted sequence:
            //   [eval collection]  StoreLocal(col_slot)
            //   PushInt(0)         StoreLocal(i_slot)        ← loop counter = 0
            // loop_start:
            //   LoadLocal(i_slot)
            //   LoadLocal(col_slot) InvokeVirtual(size, 0)   ← size on stack
            //   Lt                                            ← i < size?
            //   JumpIfFalse → exit
            //   LoadLocal(col_slot) LoadLocal(i_slot)
            //   InvokeVirtual(get, 1)                        ← element on stack
            //   StoreLocal(var_slot)                         ← bind loop var
            //   [body]
            //   LoadLocal(i_slot) PushInt(1) Add StoreLocal(i_slot)  ← i++
            //   Jump → loop_start
            // exit:
            Stmt::Foreach {
                var,
                collection,
                body,
            } => {
                // 1. Evaluate collection and stash it
                self.gen_expr(collection, chunk, locals);
                let col_slot = locals.declare("__foreach_col__");
                chunk.emit(Instruction::StoreLocal(col_slot), 0);

                // 2. Declare index counter, initialise to 0
                let i_slot = locals.declare("__foreach_i__");
                chunk.emit(Instruction::PushInt(0), 0);
                chunk.emit(Instruction::StoreLocal(i_slot), 0);

                // 3. Loop header — record the IP we will jump back to
                let loop_start = chunk.current_ip();

                // 4. Condition: i < col.size()
                chunk.emit(Instruction::LoadLocal(i_slot), 0);
                chunk.emit(Instruction::LoadLocal(col_slot), 0);
                let size_idx = chunk.intern_name("size");
                chunk.emit(
                    Instruction::InvokeVirtual {
                        name_idx: size_idx,
                        argc: 0,
                    },
                    0,
                );
                chunk.emit(Instruction::Lt, 0);

                // 5. Exit jump (placeholder — patched at end)
                let exit_jump_idx = chunk.emit(Instruction::JumpIfFalse(0), 0);

                // 6. Load current element:  col.get(i)
                chunk.emit(Instruction::LoadLocal(col_slot), 0);
                chunk.emit(Instruction::LoadLocal(i_slot), 0);
                let get_idx = chunk.intern_name("get");
                chunk.emit(
                    Instruction::InvokeVirtual {
                        name_idx: get_idx,
                        argc: 1,
                    },
                    0,
                );

                // 7. Bind loop variable
                let var_slot = locals.declare(var);
                chunk.emit(Instruction::StoreLocal(var_slot), 0);

                // 8. Body
                self.gen_block(body, chunk, locals);

                // 9. Increment: i = i + 1
                chunk.emit(Instruction::LoadLocal(i_slot), 0);
                chunk.emit(Instruction::PushInt(1), 0);
                chunk.emit(Instruction::Add, 0);
                chunk.emit(Instruction::StoreLocal(i_slot), 0);

                // 10. Back-edge jump
                chunk.emit(Instruction::Jump(loop_start), 0);

                // 11. Patch exit
                chunk.patch_jump(exit_jump_idx);
            }
            // forall (var = start to end) { body }  — statement-level concurrency
            Stmt::Forall {
                var,
                start,
                end,
                body,
            } => {
                self.gen_expr(start, chunk, locals);
                self.gen_expr(end, chunk, locals);

                // Compile the body as a closure with 1 parameter (the loop variable)
                let closure_id = self.forall_counter;
                self.forall_counter += 1;
                let closure_key = format!("{}::__forall_{}__", self.current_class, closure_id);
                let mut closure_chunk = Chunk::new();
                let mut closure_locals = locals.clone();
                closure_locals.push_scope();
                closure_locals.declare(var); // The loop index is parameter 0

                for s in body {
                    self.gen_stmt(s, &mut closure_chunk, &mut closure_locals);
                }
                if closure_chunk.code.last() != Some(&Instruction::Return)
                    && closure_chunk.code.last() != Some(&Instruction::ReturnValue)
                {
                    closure_chunk.emit(Instruction::Return, 0);
                }
                closure_chunk.local_count = closure_locals.next_slot as u16;
                self.output
                    .methods
                    .insert(closure_key.clone(), closure_chunk);

                let name_idx = chunk.intern_name(&closure_key);
                let base_slot = locals.next_slot;
                chunk.emit(
                    Instruction::MakeClosure {
                        name_idx,
                        base_slot,
                    },
                    0,
                );
                chunk.emit(Instruction::ExecuteForall, 0);
            }

            // monitor (target) { body }
            // Emitted sequence:
            //   [eval target]         StoreLocal(lock_slot)
            //   LoadLocal(lock_slot)  MonitorEnter
            //   TryBeginFinally { handler_ip: ?? }   ← patched to finally block
            //   [body]
            //   TryEnd { past_ip: ?? }               ← normal exit; patched to after finally
            // --- finally block (handler_ip) ---
            //   LoadLocal(lock_slot)  MonitorExit
            //   EndFinally                           ← resumes return or re-throws
            // --- end (past_ip) ---
            Stmt::Monitor { target, body } => {
                // 1. Evaluate target and save it
                self.gen_expr(target, chunk, locals);
                let lock_slot = locals.declare("__monitor_lock__");
                chunk.emit(Instruction::StoreLocal(lock_slot), 0);

                // 2. Acquire the lock
                chunk.emit(Instruction::LoadLocal(lock_slot), 0);
                chunk.emit(Instruction::MonitorEnter, 0);

                // 3. Open try-finally region
                let try_begin_idx = chunk.emit(Instruction::TryBeginFinally { handler_ip: 0 }, 0);

                // 4. Body
                self.gen_block(body, chunk, locals);

                // 5. Normal-path exit marker — jumps past the finally block
                let try_end_idx = chunk.emit(Instruction::TryEnd { past_ip: 0 }, 0);

                // 6. Patch TryBeginFinally → here (start of finally block)
                chunk.patch_jump(try_begin_idx);

                // 7. Finally block: always release the lock, then resume
                chunk.emit(Instruction::LoadLocal(lock_slot), 0);
                chunk.emit(Instruction::MonitorExit, 0);
                chunk.emit(Instruction::EndFinally, 0);

                // 8. Patch TryEnd → here (after the finally block)
                chunk.patch_jump(try_end_idx);
            }

            // try { } catch (E e) { } finally { }
            Stmt::TryCatch {
                try_block,
                catches,
                finally_block,
            } => {
                // Reserve a TryBeginCatch slot; patch handler_ip after try body.
                let try_begin_idx = chunk.emit(Instruction::TryBeginCatch { handler_ip: 0 }, 0);

                self.gen_block(try_block, chunk, locals);

                // Normal exit: jump past all catch/finally blocks.
                let try_end_idx = chunk.emit(Instruction::TryEnd { past_ip: 0 }, 0);

                // Patch TryBegin to point here (start of catch chain).
                chunk.patch_jump(try_begin_idx);

                let mut catch_end_jumps = Vec::new();

                for catch in catches {
                    let class_idx = chunk.intern_name(&catch.exception_type);
                    let local_slot = locals.declare(&catch.binding);
                    let catch_match_idx = chunk.emit(
                        Instruction::CatchMatch {
                            class_idx,
                            local_slot,
                            next_ip: 0,
                        },
                        0,
                    );

                    self.gen_block(&catch.body, chunk, locals);

                    let after_catch = chunk.emit(Instruction::Jump(0), 0);
                    catch_end_jumps.push(after_catch);

                    // Patch mismatch to start at next catch
                    chunk.patch_jump(catch_match_idx);
                }

                chunk.emit(Instruction::Rethrow, 0);

                // Patch all successful catch completions and the normal path TryEnd
                // to jump here (before finally block).
                let _finally_start = chunk.current_ip();
                chunk.patch_jump(try_end_idx);
                for jump in catch_end_jumps {
                    chunk.patch_jump(jump);
                }

                // Finally block (always runs).
                let _finally_start = chunk.current_ip();
                if let Some(fb) = finally_block {
                    self.gen_block(fb, chunk, locals);
                }
            }

            Stmt::Throw(expr) => {
                self.gen_expr(expr, chunk, locals);
                chunk.emit(Instruction::Throw, 0);
            }

            Stmt::ExprStmt(expr) => {
                self.gen_expr(expr, chunk, locals);
                // Discard any value left on the stack by a bare expression.
                chunk.emit(Instruction::Pop, 0);
            }

            Stmt::Switch {
                condition,
                cases,
                default_case,
            } => {
                self.gen_expr(condition, chunk, locals);
                let mut end_jumps = Vec::new();

                for case in cases {
                    chunk.emit(Instruction::Dup, 0);
                    self.gen_expr(&case.value, chunk, locals);
                    chunk.emit(Instruction::Eq, 0);

                    let next_case_jump = chunk.emit(Instruction::JumpIfFalse(0), 0);

                    chunk.emit(Instruction::Pop, 0); // pop the condition clone
                    self.gen_block(&case.body, chunk, locals);
                    end_jumps.push(chunk.emit(Instruction::Jump(0), 0));

                    chunk.patch_jump(next_case_jump);
                }

                // Fallthrough to default if no cases matched
                chunk.emit(Instruction::Pop, 0); // pop original condition
                if let Some(df) = default_case {
                    self.gen_block(df, chunk, locals);
                }

                for jump in end_jumps {
                    chunk.patch_jump(jump);
                }
            }
        }
    }

    // ── Assignment target (LHS of = ) ─────────────────────────────────────

    fn gen_assign_target(&mut self, target: &Expr, chunk: &mut Chunk, locals: &mut Locals) {
        match target {
            Expr::Ident(name) => {
                if let Some(slot) = locals.lookup(name) {
                    chunk.emit(Instruction::StoreLocal(slot), 0);
                } else {
                    // Implicit `this.field` assignment
                    let fidx = chunk.intern_name(name);
                    chunk.emit(Instruction::LoadLocal(0), 0); // push this
                    chunk.emit(Instruction::StoreField(fidx), 0);
                }
            }
            Expr::FieldAccess { object, field } => {
                // Stack has: value (from gen_expr of the RHS).
                // We need: value object on stack → StoreField.
                self.gen_expr(object, chunk, locals);
                let fidx = chunk.intern_name(field);
                chunk.emit(Instruction::StoreField(fidx), 0);
            }
            Expr::ArrayAccess { array, indices } => {
                // Stack already has: value (from RHS)
                // New logic: We need to push 'array' then 'indices' before the value.
                // However, since value is already on stack, we must handle this in Expr::Assign directly
                // to avoid complex stack manipulation.
                // This branch (gen_assign) is now a fallback or for simple targets.
                // For ArrayAccess, the primary logic is moved to Expr::Assign to get the order right.

                // Fallback (if somehow called directly): [value, array, indices]
                self.gen_expr(array, chunk, locals);
                for idx in indices {
                    self.gen_expr(idx, chunk, locals);
                }
                // At this point stack is [value, array, i, j]. We need AStore to pop value first thing.
                chunk.emit(
                    Instruction::AStore {
                        dims: indices.len() as u32,
                    },
                    0,
                );
            }
            _ => {}
        }
    }

    // ── Expressions ───────────────────────────────────────────────────────

    fn gen_expr(&mut self, expr: &Expr, chunk: &mut Chunk, locals: &mut Locals) {
        match expr {
            Expr::IntLit(n) => {
                chunk.emit(Instruction::PushInt(*n), 0);
            }
            Expr::FloatLit(f) => {
                chunk.emit(Instruction::PushFloat(*f), 0);
            }
            Expr::BoolLit(b) => {
                chunk.emit(Instruction::PushBool(*b), 0);
            }
            Expr::Null => {
                chunk.emit(Instruction::PushNull, 0);
            }
            Expr::This => {
                chunk.emit(Instruction::LoadLocal(0), 0);
            }
            Expr::Super => {
                chunk.emit(Instruction::LoadLocal(0), 0);
            }

            Expr::StringLit(s) => {
                let idx = chunk.intern_string(s);
                chunk.emit(Instruction::PushString(idx), 0);
            }

            Expr::Ident(name) => {
                if let Some(slot) = locals.lookup(name) {
                    chunk.emit(Instruction::LoadLocal(slot), 0);
                } else {
                    // Implicit `this.field` access
                    let fidx = chunk.intern_name(name);
                    // 'this' is always local 0 in methods
                    chunk.emit(Instruction::LoadLocal(0), 0);
                    chunk.emit(Instruction::LoadField(fidx), 0);
                }
            }

            Expr::FieldAccess { object, field } => {
                self.gen_expr(object, chunk, locals);
                let fidx = chunk.intern_name(field);
                chunk.emit(Instruction::LoadField(fidx), 0);
            }

            Expr::Call { callee, args } => {
                // If it's super(...) it's the parent constructor
                if let Expr::Super = callee.as_ref() {
                    if let Some(parent) = self.current_parent.clone() {
                        // Push 'this' as first argument for the constructor
                        chunk.emit(Instruction::LoadLocal(0), 0);
                        for arg in args {
                            self.gen_expr(&arg.value, chunk, locals);
                        }

                        let constructor_key = format!("{}::{}", parent, parent);
                        let nidx = chunk.intern_name(&constructor_key);
                        let argc = args.len() as u8 + 1;
                        // We use Call here because parent constructor is a static-like lookup
                        chunk.emit(
                            Instruction::Call {
                                name_idx: nidx,
                                argc,
                            },
                            0,
                        );
                        return;
                    }
                }

                // Virtual method call: push receiver BEFORE args.
                match callee.as_ref() {
                    Expr::FieldAccess { object, field } => {
                        if field == "getType" && args.is_empty() {
                            self.gen_expr(object, chunk, locals);
                            chunk.emit(Instruction::GetType, 0);
                        } else {
                            self.gen_expr(object, chunk, locals);
                            for arg in args {
                                self.gen_expr(&arg.value, chunk, locals);
                            }
                            let nidx = chunk.intern_name(field);
                            chunk.emit(
                                Instruction::InvokeVirtual {
                                    name_idx: nidx,
                                    argc: args.len() as u8,
                                },
                                0,
                            );
                        }
                    }
                    _ => {
                        let argc = args.len() as u8;
                        match callee.as_ref() {
                            Expr::Ident(name) => {
                                if let Some(slot) = locals.lookup(name) {
                                    // Pushes the closure from the local slot
                                    chunk.emit(Instruction::LoadLocal(slot), 0);
                                    for arg in args {
                                        self.gen_expr(&arg.value, chunk, locals);
                                    }
                                    chunk.emit(Instruction::CallClosure { argc }, 0);
                                } else {
                                    // Named call (Builtin or other global function)
                                    // We do NOT call gen_expr(callee) here to avoid the implicit 'this.field' lookup
                                    for arg in args {
                                        self.gen_expr(&arg.value, chunk, locals);
                                    }
                                    let nidx = chunk.intern_name(name);
                                    chunk.emit(
                                        Instruction::Call {
                                            name_idx: nidx,
                                            argc,
                                        },
                                        0,
                                    );
                                }
                            }
                            _ => {
                                // Complex callee (e.g. an expression returning a closure)
                                self.gen_expr(callee, chunk, locals);
                                for arg in args {
                                    self.gen_expr(&arg.value, chunk, locals);
                                }
                                chunk.emit(Instruction::CallClosure { argc }, 0);
                            }
                        }
                    }
                }
                return; // Since we handled it manually
            }

            Expr::ArrayAccess { array, indices } => {
                // Protocol: [array, indices...]
                self.gen_expr(array, chunk, locals);
                for idx in indices {
                    self.gen_expr(idx, chunk, locals);
                }
                chunk.emit(
                    Instruction::ALoad {
                        dims: indices.len() as u32,
                    },
                    0,
                );
            }

            Expr::ArrayAlloc {
                element_type,
                sizes,
            } => {
                for size in sizes {
                    self.gen_expr(size, chunk, locals);
                }
                // Array elements are treated as a special class for GC/Typing
                let type_name = match element_type {
                    TypeExpr::Named { name, .. } => name.clone(),
                    _ => "Object".to_string(),
                };
                let class_idx = chunk.intern_name(&format!("{}$Array", type_name));
                chunk.emit(
                    Instruction::NewArray {
                        class_idx,
                        dims: sizes.len() as u32,
                    },
                    0,
                );
            }

            Expr::New {
                class_name,
                type_args: _,
                args,
            } => {
                for arg in args {
                    self.gen_expr(&arg.value, chunk, locals);
                }
                let class_idx = chunk.intern_name(class_name);
                let argc = args.len() as u8;
                chunk.emit(Instruction::New { class_idx, argc }, 0);
            }

            Expr::BinOp { op, left, right } => {
                self.gen_expr(left, chunk, locals);
                self.gen_expr(right, chunk, locals);
                let instr = match op {
                    BinOp::Add => Instruction::Add,
                    BinOp::Sub => Instruction::Sub,
                    BinOp::Mul => Instruction::Mul,
                    BinOp::Div => Instruction::Div,
                    BinOp::Mod => Instruction::Mod,
                    BinOp::Eq => Instruction::Eq,
                    BinOp::NotEq => Instruction::NotEq,
                    BinOp::Lt => Instruction::Lt,
                    BinOp::LtEq => Instruction::LtEq,
                    BinOp::Gt => Instruction::Gt,
                    BinOp::GtEq => Instruction::GtEq,
                    BinOp::And => Instruction::And,
                    BinOp::Or => Instruction::Or,
                };
                chunk.emit(instr, 0);
            }

            Expr::UnaryOp { op, operand } => {
                if matches!(op, UnaryOp::Neg) {
                    chunk.emit(Instruction::PushInt(0), 0);
                }
                self.gen_expr(operand, chunk, locals);
                let instr = match op {
                    UnaryOp::Not => Instruction::Not,
                    UnaryOp::Neg => Instruction::SubInt,
                };
                chunk.emit(instr, 0);
            }

            Expr::Closure { params, body } => {
                // Closures are compiled as anonymous methods named by a unique key.
                // They are stored in the output and referenced by a PushString of their key.
                let closure_key = format!(
                    "{}::__closure_{}__",
                    self.current_class,
                    self.output.methods.len()
                );
                let mut closure_chunk = Chunk::new();
                let mut closure_locals = locals.clone(); // inherit parent environment
                closure_locals.push_scope();

                for p in params {
                    closure_locals.declare(&p.name);
                }
                for s in body {
                    self.gen_stmt(s, &mut closure_chunk, &mut closure_locals);
                }
                if closure_chunk.code.last() != Some(&Instruction::Return)
                    && closure_chunk.code.last() != Some(&Instruction::ReturnValue)
                {
                    closure_chunk.emit(Instruction::Return, 0);
                }
                closure_chunk.local_count = closure_locals.next_slot as u16;
                self.output
                    .methods
                    .insert(closure_key.clone(), closure_chunk);
                // Push the closure onto the VM stack via MakeClosure
                let name_idx = chunk.intern_name(&closure_key);
                let base_slot = locals.next_slot;
                chunk.emit(
                    Instruction::MakeClosure {
                        name_idx,
                        base_slot,
                    },
                    0,
                );
            }
        }
    }
}
