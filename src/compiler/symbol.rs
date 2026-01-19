//! Symbol Table
//!
//! Manages variables, scopes, and closure captures (upvalues)

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
    /// 是否被闭包捕获
    pub is_captured: bool,
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
            is_captured: false,
        }
    }
}

/// Upvalue 描述符（闭包捕获的外部变量）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Upvalue {
    /// 在父函数中的索引
    pub index: u16,
    /// true: 捕获的是父函数的局部变量; false: 捕获的是父函数的 upvalue
    pub is_local: bool,
}

impl Upvalue {
    /// 创建一个捕获局部变量的 upvalue
    pub fn local(index: u16) -> Self {
        Self { index, is_local: true }
    }
    
    /// 创建一个捕获 upvalue 的 upvalue
    pub fn upvalue(index: u16) -> Self {
        Self { index, is_local: false }
    }
}

/// 编译器的闭包上下文
#[derive(Debug, Clone, Default)]
pub struct ClosureContext {
    /// 当前闭包的 upvalues
    pub upvalues: Vec<Upvalue>,
    /// 父上下文
    pub parent: Option<Box<ClosureContext>>,
}

impl ClosureContext {
    /// 创建新的闭包上下文
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 创建带父上下文的闭包上下文
    pub fn with_parent(parent: ClosureContext) -> Self {
        Self {
            upvalues: Vec::new(),
            parent: Some(Box::new(parent)),
        }
    }
    
    /// 添加 upvalue，返回索引
    pub fn add_upvalue(&mut self, upvalue: Upvalue) -> u16 {
        // 检查是否已存在相同的 upvalue
        for (i, uv) in self.upvalues.iter().enumerate() {
            if uv == &upvalue {
                return i as u16;
            }
        }
        let index = self.upvalues.len() as u16;
        self.upvalues.push(upvalue);
        index
    }
}

/// Symbol table (supports nested scopes and closure captures)
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    /// All symbols
    symbols: Vec<Symbol>,
    /// Current scope depth
    scope_depth: usize,
    /// Current slot (for allocating local variables)
    current_slot: usize,
    /// 当前闭包上下文栈
    closure_contexts: Vec<ClosureContext>,
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
    
    /// 获取当前槽位
    pub fn current_slot(&self) -> usize {
        self.current_slot
    }
    
    /// 设置当前槽位（用于 try/catch 等特殊场景）
    pub fn set_current_slot(&mut self, slot: usize) {
        self.current_slot = slot;
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
    
    // ============ 闭包捕获支持 ============
    
    /// 进入闭包编译上下文
    pub fn begin_closure(&mut self) {
        let parent = if self.closure_contexts.is_empty() {
            ClosureContext::new()
        } else {
            let current = self.closure_contexts.pop().unwrap();
            ClosureContext::with_parent(current)
        };
        self.closure_contexts.push(parent);
    }
    
    /// 退出闭包编译上下文，返回 upvalues
    pub fn end_closure(&mut self) -> Vec<Upvalue> {
        if let Some(ctx) = self.closure_contexts.pop() {
            // 恢复父上下文
            if let Some(parent) = ctx.parent {
                self.closure_contexts.push(*parent);
            }
            ctx.upvalues
        } else {
            Vec::new()
        }
    }
    
    /// 解析变量，支持 upvalue 查找
    /// 返回: (Option<slot>, Option<upvalue_index>, is_upvalue)
    pub fn resolve_with_capture(&mut self, name: &str) -> VariableResolution {
        // 首先在当前作用域查找
        for symbol in self.symbols.iter().rev() {
            if symbol.name == name {
                return VariableResolution::Local(symbol.slot);
            }
        }
        
        // 如果在闭包上下文中，尝试解析为 upvalue
        if !self.closure_contexts.is_empty() {
            if let Some(upvalue_idx) = self.resolve_upvalue(name) {
                return VariableResolution::Upvalue(upvalue_idx);
            }
        }
        
        VariableResolution::NotFound
    }
    
    /// 递归解析 upvalue
    fn resolve_upvalue(&mut self, name: &str) -> Option<u16> {
        if self.closure_contexts.is_empty() {
            return None;
        }
        
        // 在父函数的局部变量中查找
        for (i, symbol) in self.symbols.iter_mut().enumerate().rev() {
            if symbol.name == name {
                // 标记为被捕获
                symbol.is_captured = true;
                
                // 添加 upvalue
                if let Some(ctx) = self.closure_contexts.last_mut() {
                    let upvalue = Upvalue::local(symbol.slot as u16);
                    return Some(ctx.add_upvalue(upvalue));
                }
            }
        }
        
        // 在父函数的 upvalues 中查找（嵌套闭包）
        // 这里简化处理，实际实现可能需要更复杂的递归
        None
    }
    
    /// 标记符号为被捕获
    pub fn mark_captured(&mut self, name: &str) {
        for symbol in self.symbols.iter_mut().rev() {
            if symbol.name == name {
                symbol.is_captured = true;
                break;
            }
        }
    }
    
    /// 检查符号是否被捕获
    pub fn is_captured(&self, name: &str) -> bool {
        self.resolve(name).map(|s| s.is_captured).unwrap_or(false)
    }
    
    /// 获取当前闭包上下文的 upvalues
    pub fn current_upvalues(&self) -> &[Upvalue] {
        self.closure_contexts.last()
            .map(|ctx| ctx.upvalues.as_slice())
            .unwrap_or(&[])
    }
}

/// 变量解析结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableResolution {
    /// 局部变量，包含槽位
    Local(usize),
    /// Upvalue，包含 upvalue 索引
    Upvalue(u16),
    /// 未找到
    NotFound,
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
