//! Symbol Table
//!
//! Manages variables and scopes

#![allow(dead_code)]

use crate::types::Type;

/// Symbol information
#[derive(Debug, Clone)]
pub struct Symbol {
    /// Symbol name
    pub name: String,
    /// Symbol type
    pub ty: Type,
    /// Whether this is a constant
    pub is_const: bool,
    /// Position in stack (for local variables)
    pub slot: usize,
    /// Scope depth
    pub depth: usize,
}

impl Symbol {
    /// Create a new symbol
    pub fn new(name: String, ty: Type, is_const: bool, slot: usize, depth: usize) -> Self {
        Self {
            name,
            ty,
            is_const,
            slot,
            depth,
        }
    }
}

/// Symbol table (supports nested scopes)
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    /// All symbols
    symbols: Vec<Symbol>,
    /// Current scope depth
    scope_depth: usize,
    /// Current slot (for allocating local variables)
    current_slot: usize,
}

impl SymbolTable {
    /// Create a new symbol table
    pub fn new() -> Self {
        Self::default()
    }

    /// Enter a new scope
    pub fn begin_scope(&mut self) {
        self.scope_depth += 1;
    }

    /// Exit current scope
    pub fn end_scope(&mut self) -> usize {
        let mut count = 0;

        // Remove all symbols in current scope
        while !self.symbols.is_empty() && self.symbols.last().unwrap().depth == self.scope_depth {
            self.symbols.pop();
            self.current_slot -= 1;
            count += 1;
        }

        self.scope_depth -= 1;
        count
    }

    /// Define a new symbol
    pub fn define(&mut self, name: String, ty: Type, is_const: bool) -> Result<usize, String> {
        // Check if symbol already exists in current scope
        for symbol in self.symbols.iter().rev() {
            if symbol.depth < self.scope_depth {
                break;
            }
            if symbol.name == name {
                return Err(format!(
                    "Variable '{}' is already defined in this scope",
                    name
                ));
            }
        }

        let slot = self.current_slot;
        let symbol = Symbol::new(name, ty, is_const, slot, self.scope_depth);
        self.symbols.push(symbol);
        self.current_slot += 1;

        Ok(slot)
    }

    /// Resolve a symbol
    pub fn resolve(&self, name: &str) -> Option<&Symbol> {
        // Search from back to front (nearest scope first)
        for symbol in self.symbols.iter().rev() {
            if symbol.name == name {
                return Some(symbol);
            }
        }
        None
    }

    /// Resolve symbol and return its slot
    pub fn resolve_slot(&self, name: &str) -> Option<usize> {
        self.resolve(name).map(|s| s.slot)
    }

    /// Check if symbol is a constant
    pub fn is_const(&self, name: &str) -> Option<bool> {
        self.resolve(name).map(|s| s.is_const)
    }

    /// Get current scope depth
    pub fn depth(&self) -> usize {
        self.scope_depth
    }

    /// Get current local variable count
    pub fn local_count(&self) -> usize {
        self.current_slot
    }
    
    /// Save current state for function compilation
    /// Returns (current_slot, symbols_count)
    pub fn save_state(&self) -> (usize, usize) {
        (self.current_slot, self.symbols.len())
    }
    
    /// Restore state after function compilation
    pub fn restore_state(&mut self, state: (usize, usize)) {
        self.current_slot = state.0;
        self.symbols.truncate(state.1);
    }
    
    /// Restore full state including scope depth
    pub fn restore_state_full(&mut self, state: (usize, usize), scope_depth: usize) {
        self.current_slot = state.0;
        self.symbols.truncate(state.1);
        self.scope_depth = scope_depth;
    }
    
    /// Reset slot counter for a new function scope
    /// This is used when compiling function bodies
    pub fn reset_for_function(&mut self) {
        self.current_slot = 0;
        self.scope_depth = 0;
    }
    
    /// Get current scope depth (alias for depth)
    pub fn scope_depth(&self) -> usize {
        self.scope_depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_define_and_resolve() {
        let mut table = SymbolTable::new();

        // Define variable
        let slot = table.define("x".to_string(), Type::Int, false).unwrap();
        assert_eq!(slot, 0);

        // Resolve variable
        let symbol = table.resolve("x").unwrap();
        assert_eq!(symbol.name, "x");
        assert_eq!(symbol.slot, 0);
        assert!(!symbol.is_const);
    }

    #[test]
    fn test_scope() {
        let mut table = SymbolTable::new();

        // Outer scope
        table.define("x".to_string(), Type::Int, false).unwrap();

        // Enter inner scope
        table.begin_scope();
        table.define("y".to_string(), Type::Int, false).unwrap();

        // Inner can access outer
        assert!(table.resolve("x").is_some());
        assert!(table.resolve("y").is_some());

        // Exit inner scope
        table.end_scope();

        // Outer can only access outer
        assert!(table.resolve("x").is_some());
        assert!(table.resolve("y").is_none());
    }

    #[test]
    fn test_shadowing_error() {
        let mut table = SymbolTable::new();

        table.define("x".to_string(), Type::Int, false).unwrap();

        // Duplicate definition in same scope should fail
        let result = table.define("x".to_string(), Type::Int, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_const() {
        let mut table = SymbolTable::new();

        table.define("PI".to_string(), Type::F64, true).unwrap();

        let symbol = table.resolve("PI").unwrap();
        assert!(symbol.is_const);
        assert!(table.is_const("PI").unwrap());
    }
}
