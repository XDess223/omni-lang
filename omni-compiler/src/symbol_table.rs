// omni-compiler/src/symbol_table.rs
// Phase 3: Hierarchical Symbol Table for the Omni Semantic Analyzer
//
// The symbol table tracks every declared name — classes, fields, methods,
// parameters, and local variables — across lexical scopes.
// Omni uses lexical scoping + namespace isolation via its import system.

use std::collections::HashMap;
use crate::types::OmniType;

// ── Symbol kinds ──────────────────────────────────────────────────────────

/// Metadata stored alongside each symbol name.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub ty: OmniType,
    pub kind: SymbolKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    /// A local variable or field.
    Variable,
    /// A method/function definition.
    Function {
        /// Whether each positional parameter was declared `in` (read-only).
        param_modes: Vec<bool>,
        /// The list of checked exception names this method may throw.
        throws: Vec<String>,
    },
    /// A class definition.
    Class {
        parent: Option<String>,
        interfaces: Vec<String>,
    },
    /// An interface definition.
    Interface {
        extends: Vec<String>,
    },
    /// A function parameter — carries its `in`-mode flag.
    Parameter { in_mode: bool },
}

// ── Scope ─────────────────────────────────────────────────────────────────

/// A single scope frame.  Each block, method, or class body gets its own frame.
#[derive(Debug)]
struct Scope {
    symbols: HashMap<String, Symbol>,
}

impl Scope {
    fn new() -> Self {
        Self { symbols: HashMap::new() }
    }

    fn insert(&mut self, sym: Symbol) -> bool {
        if self.symbols.contains_key(&sym.name) {
            return false; // duplicate in same scope
        }
        self.symbols.insert(sym.name.clone(), sym);
        true
    }

    fn lookup(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }
}

// ── Symbol Table ──────────────────────────────────────────────────────────

/// A stack of scopes.  Lookup walks from inner-most to outer-most.
pub struct SymbolTable {
    scopes: Vec<Scope>,
}

impl SymbolTable {
    pub fn new() -> Self {
        // Start with one global scope.
        Self { scopes: vec![Scope::new()] }
    }

    /// Push a new lexical scope (entering a block, method, or class body).
    pub fn push_scope(&mut self) {
        self.scopes.push(Scope::new());
    }

    /// Pop the innermost lexical scope (leaving a block, method, or class body).
    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Declare a symbol in the current (innermost) scope.
    /// Returns `false` if the name is already declared in this exact scope.
    pub fn declare(&mut self, sym: Symbol) -> bool {
        self.scopes.last_mut().unwrap().insert(sym)
    }

    /// Look up a name through all scopes (inner → outer).
    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        for scope in self.scopes.iter().rev() {
            if let Some(sym) = scope.lookup(name) {
                return Some(sym);
            }
        }
        None
    }

    /// Convenience: check whether a name is already visible in *any* scope.
    pub fn is_declared(&self, name: &str) -> bool {
        self.lookup(name).is_some()
    }

    /// Convenience: declare a simple variable.
    pub fn declare_var(&mut self, name: &str, ty: OmniType) -> bool {
        self.declare(Symbol { name: name.to_string(), ty, kind: SymbolKind::Variable })
    }

    /// Convenience: declare a parameter with its in-mode flag.
    pub fn declare_param(&mut self, name: &str, ty: OmniType, in_mode: bool) -> bool {
        self.declare(Symbol {
            name: name.to_string(),
            ty,
            kind: SymbolKind::Parameter { in_mode },
        })
    }
}
