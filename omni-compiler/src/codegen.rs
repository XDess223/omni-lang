// omni-compiler/src/codegen.rs
// Phase 4: Bytecode Code Generator
//
// Walks the semantically-verified AST and emits Omni bytecode Chunks.
// One Chunk is produced per method; all methods are stored in CompiledProgram.

use std::collections::HashMap;
use crate::ast::*;
use crate::bytecode::{Chunk, CompiledProgram, Instruction};

// ── Local variable slot allocator ─────────────────────────────────────────────

struct Locals {
    /// Maps variable name → slot index.
    slots: Vec<HashMap<String, u16>>,
    next_slot: u16,
}

impl Locals {
    fn new() -> Self {
        Self { slots: vec![HashMap::new()], next_slot: 0 }
    }

    fn push_scope(&mut self) {
        self.slots.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.slots.pop();
    }

    fn declare(&mut self, name: &str) -> u16 {
        let slot = self.next_slot;
        self.slots.last_mut().unwrap().insert(name.to_string(), slot);
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
}

impl CodeGen {
    pub fn new() -> Self {
        Self {
            output: CompiledProgram::default(),
            current_class: String::new(),
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
        for member in &class.members {
            if let ClassMember::Method(_, method) = member {
                let key = format!("{}::{}", class.name, method.name);
                let chunk = self.gen_method(method);
                self.output.methods.insert(key, chunk);
            }
        }
    }

    // ── Method ────────────────────────────────────────────────────────────

    fn gen_method(&mut self, method: &MethodDef) -> Chunk {
        let mut chunk = Chunk::new();
        let mut locals = Locals::new();

        // Slot 0 is always `self` (the receiver), allocated implicitly.
        locals.declare("self");

        // Declare each parameter as successive local slots.
        for param in &method.params {
            locals.declare(&param.name);
        }

        self.gen_block(&method.body, &mut chunk, &mut locals);

        // Guarantee every method ends with a Return (prevents fall-through).
        if chunk.code.last() != Some(&Instruction::Return)
            && chunk.code.last() != Some(&Instruction::ReturnValue)
        {
            chunk.emit(Instruction::Return, 0);
        }

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
                self.gen_expr(value, chunk, locals);
                self.gen_assign_target(target, chunk, locals);
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
            Stmt::If { cond, then_block, else_block } => {
                self.gen_expr(cond, chunk, locals);

                // Emit a placeholder JumpIfFalse — we'll patch it once we know
                // where the else/end is.
                let jump_false_idx = chunk.emit(Instruction::JumpIfFalse(0), 0);

                self.gen_block(then_block, chunk, locals);

                if let Some(eb) = else_block {
                    // Jump past the else block at the end of the then block.
                    let jump_end_idx = chunk.emit(Instruction::Jump(0), 0);
                    chunk.patch_jump(jump_false_idx);  // false → start of else
                    self.gen_block(eb, chunk, locals);
                    chunk.patch_jump(jump_end_idx);    // end of else → here
                } else {
                    chunk.patch_jump(jump_false_idx);  // false → after if
                }
            }

            // foreach (var in collection) { body }
            // EBNF: <iteration_stmt> → foreach (<id> in <collection>) <block>
            Stmt::Foreach { var, collection, body } => {
                self.gen_expr(collection, chunk, locals);
                let col_slot = locals.declare("__foreach_col__");
                chunk.emit(Instruction::StoreLocal(col_slot), 0);

                // Emit a ForallBegin hint for the VM's iterator protocol.
                // In full implementation this becomes an iterator object call.
                let var_slot = locals.declare(var);
                chunk.emit(Instruction::ForallBegin {
                    local_slot: var_slot,
                    collection_slot: col_slot,
                }, 0);

                self.gen_block(body, chunk, locals);

                chunk.emit(Instruction::ForallEnd, 0);
            }

            // FORALL (var in collection) { body }  — statement-level concurrency
            Stmt::Forall { var, collection, body } => {
                self.gen_expr(collection, chunk, locals);
                let col_slot = locals.declare("__forall_col__");
                chunk.emit(Instruction::StoreLocal(col_slot), 0);
                let var_slot = locals.declare(var);
                chunk.emit(Instruction::ForallBegin {
                    local_slot: var_slot,
                    collection_slot: col_slot,
                }, 0);
                self.gen_block(body, chunk, locals);
                chunk.emit(Instruction::ForallEnd, 0);
            }

            // try { } catch (E e) { } finally { }
            Stmt::TryCatch { try_block, catches, finally_block } => {
                // Reserve a TryBegin slot; patch handler_ip after try body.
                let try_begin_idx = chunk.emit(Instruction::TryBegin { handler_ip: 0 }, 0);

                self.gen_block(try_block, chunk, locals);

                // Normal exit: jump past all catch/finally blocks.
                let try_end_idx = chunk.emit(Instruction::TryEnd { past_ip: 0 }, 0);

                // Patch TryBegin to point here (start of catch chain).
                chunk.patch_jump(try_begin_idx);

                for catch in catches {
                    let class_idx = chunk.intern_name(&catch.exception_type);
                    let local_slot = locals.declare(&catch.binding);
                    chunk.emit(Instruction::CatchMatch { class_idx, local_slot }, 0);
                    self.gen_block(&catch.body, chunk, locals);
                    let after_catch = chunk.emit(Instruction::Jump(0), 0);
                    // Will be patched to jump past finally.
                    let _ = after_catch; // placeholder — full implementation patches this
                }

                chunk.emit(Instruction::Rethrow, 0);

                // Patch TryEnd to point past all catches + rethrow.
                chunk.patch_jump(try_end_idx);

                // Finally block (always runs).
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
            _ => {}
        }
    }

    // ── Expressions ───────────────────────────────────────────────────────

    fn gen_expr(&mut self, expr: &Expr, chunk: &mut Chunk, locals: &mut Locals) {
        match expr {
            Expr::IntLit(n)    => { chunk.emit(Instruction::PushInt(*n), 0); }
            Expr::FloatLit(f)  => { chunk.emit(Instruction::PushFloat(*f), 0); }
            Expr::BoolLit(b)   => { chunk.emit(Instruction::PushBool(*b), 0); }
            Expr::Null         => { chunk.emit(Instruction::PushNull, 0); }

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
                // Push all arguments left-to-right.
                for arg in args { self.gen_expr(arg, chunk, locals); }
                let argc = args.len() as u8;

                match callee.as_ref() {
                    Expr::FieldAccess { object, field } => {
                        // Virtual method call: push receiver AFTER args.
                        self.gen_expr(object, chunk, locals);
                        let nidx = chunk.intern_name(field);
                        chunk.emit(Instruction::InvokeVirtual { name_idx: nidx, argc }, 0);
                    }
                    Expr::Ident(name) => {
                        let nidx = chunk.intern_name(name);
                        chunk.emit(Instruction::Call { name_idx: nidx, argc }, 0);
                    }
                    _ => {
                        self.gen_expr(callee, chunk, locals);
                        let nidx = chunk.intern_name("__closure__");
                        chunk.emit(Instruction::Call { name_idx: nidx, argc }, 0);
                    }
                }
            }

            Expr::New { class_name, args } => {
                for arg in args { self.gen_expr(arg, chunk, locals); }
                let class_idx = chunk.intern_name(class_name);
                let argc = args.len() as u8;
                chunk.emit(Instruction::New { class_idx, argc }, 0);
            }

            Expr::BinOp { op, left, right } => {
                self.gen_expr(left, chunk, locals);
                self.gen_expr(right, chunk, locals);
                let instr = match op {
                    BinOp::Add  => Instruction::AddInt,
                    BinOp::Sub  => Instruction::SubInt,
                    BinOp::Mul  => Instruction::MulInt,
                    BinOp::Div  => Instruction::DivInt,
                    BinOp::Mod  => Instruction::ModInt,
                    BinOp::Eq   => Instruction::Eq,
                    BinOp::NotEq => Instruction::NotEq,
                    BinOp::Lt   => Instruction::LtInt,
                    BinOp::LtEq => Instruction::LtEqInt,
                    BinOp::Gt   => Instruction::GtInt,
                    BinOp::GtEq => Instruction::GtEqInt,
                    BinOp::And  => Instruction::And,
                    BinOp::Or   => Instruction::Or,
                };
                chunk.emit(instr, 0);
            }

            Expr::UnaryOp { op, operand } => {
                self.gen_expr(operand, chunk, locals);
                let instr = match op {
                    UnaryOp::Not => Instruction::Not,
                    UnaryOp::Neg => Instruction::SubInt, // VM handles negation via sub from 0
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
                let mut closure_locals = Locals::new();
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
                self.output.methods.insert(closure_key.clone(), closure_chunk);
                // Push the key string so the VM can look up the closure chunk.
                let idx = chunk.intern_string(&closure_key);
                chunk.emit(Instruction::PushString(idx), 0);
            }
        }
    }
}
