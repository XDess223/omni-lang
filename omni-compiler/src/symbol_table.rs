// omni-compiler/src/symbol_table.rs
// Phase 3: Hierarchical Symbol Table for the Omni Semantic Analyzer
//
// The symbol table tracks every declared name — classes, fields, methods,
// parameters, and local variables — across lexical scopes.
// Omni uses lexical scoping + namespace isolation via its import system.
//
// Method Overloading: methods are keyed as "name/N" (N = arity) so that
// methods with the same name but different parameter counts coexist.

use std::collections::HashMap;
use crate::types::OmniType;

// ── Symbol kinds ──────────────────────────────────────────────────────────

/// Metadata stored alongside each symbol name.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub ty: OmniType,
    pub kind: SymbolKind,
    /// For Function symbols: the declared parameter names in order (used for keyword arg resolution).
    pub param_names: Vec<String>,
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
        /// Generic type parameters for this class.
        type_params: Vec<String>,
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

    /// Insert with a plain name key. Returns false if already exists.
    fn insert(&mut self, sym: Symbol) -> bool {
        if self.symbols.contains_key(&sym.name) {
            return false; // duplicate in same scope
        }
        self.symbols.insert(sym.name.clone(), sym);
        true
    }

    /// Insert using an overload key "name/arity". Always succeeds (caller controls uniqueness).
    fn insert_overload(&mut self, key: String, sym: Symbol) {
        self.symbols.insert(key, sym);
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

    /// Declare a function overload using the key "name/arity".
    /// Multiple overloads with the same name but different arities are allowed.
    /// Returns `false` if an exact "name/arity" key already exists in this scope.
    pub fn declare_overload(&mut self, sym: Symbol, arity: usize) -> bool {
        let key = format!("{}/{}", sym.name, arity);
        let scope = self.scopes.last_mut().unwrap();
        if scope.symbols.contains_key(&key) {
            return false; // same name+arity in same scope
        }
        scope.insert_overload(key, sym);
        true
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

    /// Look up a method overload: try "name/arity" first, then bare "name".
    pub fn lookup_overload(&self, name: &str, arity: usize) -> Option<&Symbol> {
        let overload_key = format!("{}/{}", name, arity);
        for scope in self.scopes.iter().rev() {
            if let Some(sym) = scope.lookup(&overload_key) {
                return Some(sym);
            }
            // Fallback: bare name (for non-overloaded functions / variables)
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
        self.declare(Symbol {
            name: name.to_string(),
            ty,
            kind: SymbolKind::Variable,
            param_names: Vec::new(),
        })
    }

    /// Convenience: declare a parameter with its in-mode flag.
    pub fn declare_param(&mut self, name: &str, ty: OmniType, in_mode: bool) -> bool {
        self.declare(Symbol {
            name: name.to_string(),
            ty,
            kind: SymbolKind::Parameter { in_mode },
            param_names: Vec::new(),
        })
    }
}
