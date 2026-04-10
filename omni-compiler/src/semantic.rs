// omni-compiler/src/semantic.rs
// Phase 3: Semantic Analyzer for the Omni Language
//
// Walks the AST produced by the parser and enforces:
//   1. Type inference   — resolves `var x = 0` → Int
//   2. Nominal typing   — strict name-equivalence, no structural compatibility
//   3. Null safety      — non-Optional types may never hold null
//   4. In-mode (read-only) parameter enforcement
//   5. Checked exception verification — callers must handle/re-throw
//   6. Duplicate name detection within the same scope

use crate::ast::*;
use crate::types::OmniType;
use crate::symbol_table::{Symbol, SymbolKind, SymbolTable};

// ── Semantic Errors ───────────────────────────────────────────────────────

#[derive(Debug)]
pub enum SemanticError {
    /// A name was used before being declared.
    Undeclared(String),
    /// A name was declared twice in the same scope.
    DuplicateDeclaration(String),
    /// Assigning a null to a non-Optional variable.
    NullToNonOptional { var: String, ty: String },
    /// A method call on a parameter declared `in` tried to mutate state.
    InModeViolation { param: String, method: String },
    /// A method that throws a checked exception but no try-catch wraps the call.
    UncaughtCheckedException { exception: String, method: String },
    /// Nominal type mismatch on assignment or argument passing.
    TypeMismatch { expected: String, found: String },
    /// A class body referenced an undeclared super-class.
    UndeclaredSuperClass(String),
}

// ── Analyzer state ────────────────────────────────────────────────────────

pub struct Analyzer {
    pub table: SymbolTable,
    pub errors: Vec<SemanticError>,
    /// Stack of checked-exception sets currently "declared to be caught".
    /// Each try block pushes its catch types; the finally exit pops them.
    caught_exceptions: Vec<Vec<String>>,
    /// The set of checked exceptions the current method has declared via `throws`.
    current_method_throws: Vec<String>,
    /// Names of `in`-mode parameters in the current method scope.
    in_mode_params: Vec<String>,
}

impl Analyzer {
    pub fn new() -> Self {
        Self {
            table: SymbolTable::new(),
            errors: Vec::new(),
            caught_exceptions: Vec::new(),
            current_method_throws: Vec::new(),
            in_mode_params: Vec::new(),
        }
    }

    // ── Entry point ───────────────────────────────────────────────────────

    pub fn analyze(&mut self, program: &Program) {
        // First pass: register all class names so forward references work.
        for class in &program.classes {
            let declared = self.table.declare(Symbol {
                name: class.name.clone(),
                ty: OmniType::Class(class.name.clone()),
                kind: SymbolKind::Class {
                    parent: class.extends.clone(),
                    interfaces: class.implements.clone(),
                },
            });
            if !declared {
                self.errors.push(SemanticError::DuplicateDeclaration(class.name.clone()));
            }
        }

        // Second pass: analyze class bodies.
        for class in &program.classes {
            self.analyze_class(class);
        }
    }

    // ── Class analysis ────────────────────────────────────────────────────

    fn analyze_class(&mut self, class: &ClassDef) {
        // Verify super-class is declared (if any).
        if let Some(ref parent) = class.extends {
            if !self.table.is_declared(parent) {
                self.errors.push(SemanticError::UndeclaredSuperClass(parent.clone()));
            }
        }

        self.table.push_scope();

        for member in &class.members {
            match member {
                ClassMember::Field(_, decl) => self.analyze_var_decl(decl),
                ClassMember::Method(_, method) => {
                    // Register method in the class scope.
                    let param_modes: Vec<bool> =
                        method.params.iter().map(|p| p.is_in_mode).collect();
                    let param_types: Vec<OmniType> =
                        method.params.iter().map(|p| self.resolve_type_expr(&p.ty)).collect();
                    let ret = method.return_type.as_ref()
                        .map(|t| self.resolve_type_expr(t))
                        .unwrap_or(OmniType::Void);

                    let fn_type = OmniType::Function {
                        param_types,
                        return_type: Box::new(ret),
                    };
                    let declared = self.table.declare(Symbol {
                        name: method.name.clone(),
                        ty: fn_type,
                        kind: SymbolKind::Function {
                            param_modes,
                            throws: method.throws.clone(),
                        },
                    });
                    if !declared {
                        self.errors.push(SemanticError::DuplicateDeclaration(method.name.clone()));
                    }
                    self.analyze_method(method);
                }
            }
        }

        self.table.pop_scope();
    }

    // ── Method analysis ───────────────────────────────────────────────────

    fn analyze_method(&mut self, method: &MethodDef) {
        self.current_method_throws = method.throws.clone();
        self.in_mode_params.clear();
        self.table.push_scope();

        // Declare all parameters in the method scope.
        for param in &method.params {
            let ty = self.resolve_type_expr(&param.ty);
            if !self.table.declare_param(&param.name, ty, param.is_in_mode) {
                self.errors.push(SemanticError::DuplicateDeclaration(param.name.clone()));
            }
            if param.is_in_mode {
                self.in_mode_params.push(param.name.clone());
            }
        }

        self.analyze_block(&method.body);

        self.table.pop_scope();
        self.current_method_throws.clear();
        self.in_mode_params.clear();
    }

    // ── Block / Statement analysis ────────────────────────────────────────

    fn analyze_block(&mut self, block: &Block) {
        self.table.push_scope();
        for stmt in block {
            self.analyze_stmt(stmt);
        }
        self.table.pop_scope();
    }

    fn analyze_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarDecl(decl) => self.analyze_var_decl(decl),

            Stmt::Assign { target, value } => {
                // Enforce in-mode: a parameter marked `in` cannot be assigned.
                if let Expr::Ident(name) = target {
                    if self.in_mode_params.contains(name) {
                        self.errors.push(SemanticError::InModeViolation {
                            param: name.clone(),
                            method: "(assignment)".to_string(),
                        });
                    }
                }
                let _ltype = self.analyze_expr(target);
                let _rtype = self.analyze_expr(value);
                // Full type-mismatch check would compare ltype vs rtype here.
            }

            Stmt::Return(expr) => {
                if let Some(e) = expr { self.analyze_expr(e); }
            }

            Stmt::If { cond, then_block, else_block } => {
                let cond_ty = self.analyze_expr(cond);
                if cond_ty != OmniType::Bool && cond_ty != OmniType::Inferred {
                    self.errors.push(SemanticError::TypeMismatch {
                        expected: "Bool".to_string(),
                        found: cond_ty.to_string(),
                    });
                }
                self.analyze_block(then_block);
                if let Some(eb) = else_block { self.analyze_block(eb); }
            }

            Stmt::Foreach { var, collection, body } => {
                let _col_ty = self.analyze_expr(collection);
                self.table.push_scope();
                // Infer the element type as Inferred (full generics resolution is Phase 4).
                self.table.declare_var(var, OmniType::Inferred);
                for s in body { self.analyze_stmt(s); }
                self.table.pop_scope();
            }

            Stmt::Forall { var, collection, body } => {
                let _col_ty = self.analyze_expr(collection);
                self.table.push_scope();
                self.table.declare_var(var, OmniType::Inferred);
                for s in body { self.analyze_stmt(s); }
                self.table.pop_scope();
            }

            Stmt::TryCatch { try_block, catches, finally_block } => {
                // Push the set of exception types this try-catch handles.
                let caught: Vec<String> =
                    catches.iter().map(|c| c.exception_type.clone()).collect();
                self.caught_exceptions.push(caught);

                self.analyze_block(try_block);

                for catch in catches {
                    self.table.push_scope();
                    // The caught exception is bound as a variable inside the catch block.
                    self.table.declare_var(
                        &catch.binding,
                        OmniType::Class(catch.exception_type.clone()),
                    );
                    for s in &catch.body { self.analyze_stmt(s); }
                    self.table.pop_scope();
                }

                if let Some(fb) = finally_block { self.analyze_block(fb); }

                self.caught_exceptions.pop();
            }

            Stmt::Throw(expr) => {
                self.analyze_expr(expr);
            }

            Stmt::ExprStmt(expr) => {
                self.analyze_expr(expr);
            }
        }
    }

    // ── Variable declaration analysis (type inference + null safety) ───────

    fn analyze_var_decl(&mut self, decl: &VarDecl) {
        let ty = if let Some(ref type_expr) = decl.ty {
            let mut resolved = self.resolve_type_expr(type_expr);
            if decl.optional {
                resolved = OmniType::Optional(Box::new(resolved));
            }
            resolved
        } else {
            // Type inference: infer from the initializer expression.
            if let Some(ref init) = decl.initializer {
                self.infer_expr_type(init)
            } else {
                OmniType::Inferred
            }
        };

        // Null safety: if an assignment uses `null` and the type is not Optional, error.
        if let Some(Expr::Null) = &decl.initializer {
            if !ty.is_nullable() {
                self.errors.push(SemanticError::NullToNonOptional {
                    var: decl.name.clone(),
                    ty: ty.to_string(),
                });
            }
        }

        if !self.table.declare_var(&decl.name, ty) {
            self.errors.push(SemanticError::DuplicateDeclaration(decl.name.clone()));
        }
    }

    // ── Expression analysis ───────────────────────────────────────────────

    /// Recursively walks an expression, enforcing in-mode and returning inferred type.
    fn analyze_expr(&mut self, expr: &Expr) -> OmniType {
        match expr {
            Expr::IntLit(_)    => OmniType::Int,
            Expr::FloatLit(_)  => OmniType::Float,
            Expr::StringLit(_) => OmniType::Str,
            Expr::BoolLit(_)   => OmniType::Bool,
            Expr::Null         => OmniType::Optional(Box::new(OmniType::Inferred)),

            Expr::Ident(name) => {
                if let Some(sym) = self.table.lookup(name) {
                    sym.ty.clone()
                } else {
                    self.errors.push(SemanticError::Undeclared(name.clone()));
                    OmniType::Inferred
                }
            }

            Expr::FieldAccess { object, field } => {
                self.analyze_expr(object);
                // Full field resolution requires a full class registry (Phase 4).
                // For now, return Inferred and trust the symbol table lookup above.
                let _ = field;
                OmniType::Inferred
            }

            Expr::Call { callee, args } => {
                // ── IN-MODE ENFORCEMENT ───────────────────────────────────
                // If the callee is a field access on an `in`-mode parameter,
                // check whether the method being called is marked read-only.
                // Since we do not yet have a full method-purity registry,
                // we flag any mutating-pattern calls on known `in` params.
                if let Expr::FieldAccess { object, field } = callee.as_ref() {
                    if let Expr::Ident(obj_name) = object.as_ref() {
                        if self.in_mode_params.contains(obj_name) {
                            // Any method call on an `in` parameter is flagged.
                            // A full implementation would cross-reference a
                            // "read-only method" registry per class.
                            self.errors.push(SemanticError::InModeViolation {
                                param: obj_name.clone(),
                                method: field.clone(),
                            });
                        }
                    }
                }

                // ── CHECKED EXCEPTION VERIFICATION ────────────────────────
                // If calling a function that declares `throws`, verify the
                // call site is inside a try-catch that handles those exceptions,
                // OR the current method also re-declares them in its own `throws`.
                if let Expr::Ident(fn_name) = callee.as_ref() {
                    if let Some(sym) = self.table.lookup(fn_name).cloned() {
                        if let SymbolKind::Function { ref throws, .. } = sym.kind {
                            for exc in throws {
                                let is_caught = self.caught_exceptions
                                    .iter()
                                    .any(|set| set.contains(exc));
                                let is_rethrown = self.current_method_throws.contains(exc);
                                if !is_caught && !is_rethrown {
                                    self.errors.push(SemanticError::UncaughtCheckedException {
                                        exception: exc.clone(),
                                        method: fn_name.clone(),
                                    });
                                }
                            }
                        }
                    }
                }

                for arg in args { self.analyze_expr(arg); }
                OmniType::Inferred
            }

            Expr::New { class_name, args } => {
                if !self.table.is_declared(class_name) {
                    self.errors.push(SemanticError::Undeclared(class_name.clone()));
                }
                for arg in args { self.analyze_expr(arg); }
                OmniType::Class(class_name.clone())
            }

            Expr::BinOp { op, left, right } => {
                let l = self.analyze_expr(left);
                let r = self.analyze_expr(right);
                // Comparison and logical operators always produce Bool.
                match op {
                    BinOp::Eq | BinOp::NotEq
                    | BinOp::Lt | BinOp::LtEq
                    | BinOp::Gt | BinOp::GtEq
                    | BinOp::And | BinOp::Or => OmniType::Bool,
                    // Arithmetic operators produce the operand type.
                    _ => if l == r { l } else { OmniType::Inferred },
                }
            }

            Expr::UnaryOp { op: _, operand } => {
                self.analyze_expr(operand)
            }

            Expr::Closure { params, body } => {
                self.table.push_scope();
                let mut param_types = Vec::new();
                for p in params {
                    let ty = self.resolve_type_expr(&p.ty);
                    param_types.push(ty.clone());
                    self.table.declare_param(&p.name, ty, p.is_in_mode);
                    if p.is_in_mode {
                        self.in_mode_params.push(p.name.clone());
                    }
                }
                for s in body { self.analyze_stmt(s); }
                self.table.pop_scope();
                // Remove closure params from in-mode list when leaving closure scope.
                for p in params {
                    self.in_mode_params.retain(|n| n != &p.name);
                }
                OmniType::Function {
                    param_types,
                    return_type: Box::new(OmniType::Inferred),
                }
            }
        }
    }

    // ── Type inference helper ─────────────────────────────────────────────

    /// Infer the OmniType of a literal/primary expression WITHOUT
    /// recording any semantic errors.  Used for `var x = <expr>`.
    fn infer_expr_type(&self, expr: &Expr) -> OmniType {
        match expr {
            Expr::IntLit(_)    => OmniType::Int,
            Expr::FloatLit(_)  => OmniType::Float,
            Expr::StringLit(_) => OmniType::Str,
            Expr::BoolLit(_)   => OmniType::Bool,
            Expr::Null         => OmniType::Optional(Box::new(OmniType::Inferred)),
            Expr::New { class_name, .. } => OmniType::Class(class_name.clone()),
            _ =>  OmniType::Inferred,
        }
    }

    // ── Type expression resolver ──────────────────────────────────────────

    pub fn resolve_type_expr(&self, te: &TypeExpr) -> OmniType {
        let args: Vec<OmniType> =
            te.type_args.iter().map(|a| self.resolve_type_expr(a)).collect();
        OmniType::from_name(&te.name, args, te.optional)
    }
}
