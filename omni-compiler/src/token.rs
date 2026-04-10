// omni-compiler/src/token.rs
// Phase 1: Token Definitions for the Omni Language Lexer

/// Every meaningful unit the Omni scanner produces.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // ── Literals ──────────────────────────────────────────────
    IntLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(String),
    BoolLiteral(bool),

    // ── Identifiers ───────────────────────────────────────────
    /// A class-name identifier — must start with an uppercase letter.
    ClassIdent(String),
    /// A variable / method identifier — must start with a lowercase letter.
    Ident(String),

    // ── Keywords ──────────────────────────────────────────────
    Class,
    Function,
    Var,
    Return,
    If,
    Else,
    Foreach,
    In,
    Forall,
    Try,
    Catch,
    Finally,
    Throw,
    Throws,
    New,
    Extends,
    Implements,
    Import,
    Namespace,
    Public,
    Private,
    Protected,
    True,
    False,
    Null,       // Only allowed via Optional types (String?)

    // ── Types ─────────────────────────────────────────────────
    TypeInt,
    TypeFloat,
    TypeString,
    TypeBool,

    // ── Operators ─────────────────────────────────────────────
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,         // ==
    NotEq,      // !=
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,        // &&
    Or,         // ||
    Not,        // !
    Assign,     // =

    // ── Punctuation ───────────────────────────────────────────
    Semicolon,
    Colon,
    Comma,
    Dot,
    Question,   // ? — marks Optional types
    LParen,
    RParen,
    LBrace,
    RBrace,
    LAngle,     // < for generics
    RAngle,     // > for generics

    // ── Special ───────────────────────────────────────────────
    Eof,
}
