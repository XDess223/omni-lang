// omni-compiler/src/ast.rs
// Phase 2: Abstract Syntax Tree node definitions
//
// These Rust enums serve as the "tagged union" representation of
// every grammatical construct in the Omni EBNF grammar.

/// A complete Omni source file.
#[derive(Debug, Clone)]
pub struct Program {
    pub imports: Vec<String>,
    pub namespace: Option<String>,
    pub interfaces: Vec<InterfaceDef>,
    pub classes: Vec<ClassDef>,
}

// ── Interface Definitions ──────────────────────────────────────────────────

/// <interface_def> → interface <id> [extends <ident_list>] '{' { <interface_member> } '}'
#[derive(Debug, Clone)]
pub struct InterfaceDef {
    pub name: String,
    pub type_params: Vec<TypeParam>,
    pub extends: Vec<String>,
    pub methods: Vec<MethodDecl>,
}

/// A method declaration inside an interface (no body)
#[derive(Debug, Clone)]
pub struct MethodDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub throws: Vec<String>,
    pub return_type: Option<TypeExpr>,
}

// ── Class Definitions ────────────────────────────────────────────────────

/// <class_def> → class <id> [extends <id>] [implements <ident_list>]
///               '{' { <class_member> } '}'
#[derive(Debug, Clone)]
pub struct ClassDef {
    pub name: String,                        // Must be uppercase (ClassIdent)
    pub type_params: Vec<TypeParam>,         // Generics e.g. <T extends Ident>
    pub extends: Option<String>,
    pub implements: Vec<String>,
    pub members: Vec<ClassMember>,
}

#[derive(Debug, Clone)]
pub struct TypeParam {
    pub name: String,
    pub bound: Option<String>, // e.g. "Comparable" in <T extends Comparable>
}

/// <class_member> → <access_mod> <method_def> | <access_mod> <var_decl> ;
#[derive(Debug, Clone)]
pub enum ClassMember {
    Method(AccessMod, MethodDef),
    Field(AccessMod, VarDecl),
}

/// <access_mod> → public | private | protected
#[derive(Debug, Clone, PartialEq)]
pub enum AccessMod {
    Public,
    Private,
    Protected,
}

// ── Method Definitions ───────────────────────────────────────────────────

/// <method_def> → function <id> ( [<param_list>] ) [throws <ident_list>] <block>
#[derive(Debug, Clone)]
pub struct MethodDef {
    pub name: String,
    pub params: Vec<Param>,
    pub throws: Vec<String>,                 // checked exception list
    pub return_type: Option<TypeExpr>,
    pub body: Block,
}

/// A single parameter: `in name : Type`
#[derive(Debug, Clone)]
pub struct Param {
    pub is_in_mode: bool,                    // `in` keyword → read-only view
    pub name: String,
    pub ty: TypeExpr,
}

// ── Statements ───────────────────────────────────────────────────────────

/// <block> → '{' { <statement> } '}'
pub type Block = Vec<Stmt>;

/// <statement> → ... (all Omni statement forms)
#[derive(Debug, Clone)]
pub enum Stmt {
    /// var <id> : <type> [?] | var <id> = <expr>
    VarDecl(VarDecl),
    /// <expr> = <expr>  (assignment)
    Assign { target: Expr, value: Expr },
    /// return [<expr>]
    Return(Option<Expr>),
    /// if (<expr>) <block> [else <block>]
    If { cond: Expr, then_block: Block, else_block: Option<Block> },
    /// foreach (<id> in <collection>) <block>
    Foreach { var: String, collection: Expr, body: Block },
    /// forall (<id> = <expr> to <expr>) <block> — statement-level concurrency
    Forall { var: String, start: Expr, end: Expr, body: Block },
    /// try <block> catch (<id> <id>) <block> [finally <block>]
    TryCatch {
        try_block: Block,
        catches: Vec<CatchClause>,
        finally_block: Option<Block>,
    },
    /// throw <expr>
    Throw(Expr),
    /// monitor (<expr>) <block> — mutual exclusion
    Monitor { target: Expr, body: Block },
    /// A bare expression statement: method calls, etc.
    ExprStmt(Expr),
}

/// Variable / field declaration.
/// <var_decl> → var <id> : <type> [?] | var <id> = <expr>
#[derive(Debug, Clone)]
pub struct VarDecl {
    pub name: String,
    pub ty: Option<TypeExpr>,               // None means "infer from expr"
    pub optional: bool,                     // true when '?' modifier is present
    pub initializer: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct CatchClause {
    pub exception_type: String,
    pub binding: String,
    pub body: Block,
}

// ── Expressions ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Expr {
    IntLit(i64),
    FloatLit(f64),
    StringLit(String),
    BoolLit(bool),
    Null,

    /// A variable or parameter reference.
    Ident(String),

    /// object.field or object.method(...)
    FieldAccess { object: Box<Expr>, field: String },

    /// method_name(arg1, arg2, ...)  — also handles keyword args
    Call { callee: Box<Expr>, args: Vec<Expr> },

    /// new ClassName<T1, T2>(args)
    New { class_name: String, type_args: Vec<TypeExpr>, args: Vec<Expr> },

    /// Binary operation: left OP right
    BinOp { op: BinOp, left: Box<Expr>, right: Box<Expr> },

    /// Unary operation: OP expr
    UnaryOp { op: UnaryOp, operand: Box<Expr> },

    /// Closure / anonymous function: function(params) { body }
    Closure { params: Vec<Param>, body: Block },
}

#[derive(Debug, Clone)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, NotEq, Lt, LtEq, Gt, GtEq,
    And, Or,
}

#[derive(Debug, Clone)]
pub enum UnaryOp {
    Not, Neg,
}

// ── Types ────────────────────────────────────────────────────────────────

/// A type expression, e.g. `Int`, `String?`, `List<Student>`, `(Int, Int) -> Int`
#[derive(Debug, Clone)]
pub enum TypeExpr {
    Named {
        name: String,
        type_args: Vec<TypeExpr>,
        optional: bool,
    },
    Function {
        params: Vec<TypeExpr>,
        return_type: Box<TypeExpr>,
        optional: bool,
    }
}

impl TypeExpr {
    pub fn is_optional(&self) -> bool {
        match self {
            TypeExpr::Named { optional, .. } => *optional,
            TypeExpr::Function { optional, .. } => *optional,
        }
    }
}
