// omni-compiler/src/types.rs
// Phase 3: Omni Type System Representations
//
// Implements nominal type equivalence: two types are only compatible
// if they share the EXACT same name, not merely the same structure.

use std::fmt;

/// A fully resolved Omni type, used by the semantic analyzer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OmniType {
    // ── Primitive / Built-in ──────────────────────────────────────────
    Int,
    Float,
    Bool,
    Str,            // maps to Omni's `String` keyword (primitive type)
    Void,           // return type of methods with no return

    // ── Optional wrapper (Nominal: Optional<Int> != Int) ─────────────
    Optional(Box<OmniType>),

    // ── User-defined class (resolved by name === nominal equivalence) ─
    Class(String),

    // ── User-defined interface
    Interface(String),

    // ── Generic instantiation: e.g. List<Student> ────────────────────
    Generic { base: String, params: Vec<OmniType> },

    // ── Function / closure type ───────────────────────────────────────
    Function { param_types: Vec<OmniType>, return_type: Box<OmniType> },

    // ── Array type ───────────────────────────────────────────────────
    Array { element_type: Box<OmniType>, dimensions: usize },

    /// Placeholder used before type inference resolves a declaration.
    Inferred,
}

impl fmt::Display for OmniType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OmniType::Int           => write!(f, "Int"),
            OmniType::Float         => write!(f, "Float"),
            OmniType::Bool          => write!(f, "Bool"),
            OmniType::Str           => write!(f, "String"),
            OmniType::Void          => write!(f, "Void"),
            OmniType::Optional(t)   => write!(f, "{}?", t),
            OmniType::Class(name)   => write!(f, "{}", name),
            OmniType::Interface(name)=> write!(f, "{}", name),
            OmniType::Inferred      => write!(f, "<inferred>"),
            OmniType::Generic { base, params } => {
                write!(f, "{}<", base)?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", p)?;
                }
                write!(f, ">")
            }
            OmniType::Function { param_types, return_type } => {
                write!(f, "function(")?;
                for (i, t) in param_types.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", t)?;
                }
                write!(f, ") -> {}", return_type)
            }
            OmniType::Array { element_type, dimensions } => {
                let brackets = ",".repeat(dimensions - 1);
                write!(f, "{}[{}]", element_type, brackets)
            }
        }
    }
}

impl OmniType {
    /// Resolve an AST type expression string to an OmniType.
    pub fn from_name(name: &str, type_args: Vec<OmniType>, optional: bool) -> Self {
        let base = match name {
            "Int"    => OmniType::Int,
            "Float"  => OmniType::Float,
            "Bool"   => OmniType::Bool,
            "String" => OmniType::Str,
            "Void"   => OmniType::Void,
            other => {
                if type_args.is_empty() {
                    OmniType::Class(other.to_string())
                } else {
                    OmniType::Generic { base: other.to_string(), params: type_args }
                }
            }
        };
        if optional { OmniType::Optional(Box::new(base)) } else { base }
    }

    /// Nominal type equivalence: strict name matching.
    /// `Optional<Int>` is NOT compatible with `Int`, but `Int` IS compatible with `Optional<Int>`.
    pub fn is_compatible_with(&self, other: &OmniType) -> bool {
        if self == other {
            return true;
        }
        // Allow assigning T into Optional<T>
        if let OmniType::Optional(inner) = self {
            if inner.as_ref() == other { return true; }
        }
        // Allow Function inferred return assignment
        if let (OmniType::Function { param_types: p1, return_type: r1 },
                OmniType::Function { param_types: p2, return_type: r2 }) = (self, other) {
            if p1 == p2 {
                if **r2 == OmniType::Inferred {
                    return true;
                }
                if r1 == r2 {
                    return true;
                }
            }
        }
        // Array compatibility: same element type and same dimensions
        if let (OmniType::Array { element_type: e1, dimensions: d1 },
                OmniType::Array { element_type: e2, dimensions: d2 }) = (self, other) {
            return d1 == d2 && e1.as_ref().is_compatible_with(e2);
        }
        false
    }

    /// Returns true if the type can legally hold null/None.
    pub fn is_nullable(&self) -> bool {
        matches!(self, OmniType::Optional(_))
    }
}
