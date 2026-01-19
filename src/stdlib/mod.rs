//! 标准库模块
//! 
//! 实现用 Rust 编写的内置标准库

mod vmtest;
pub mod exception;

pub use vmtest::VmTestLib;
pub use exception::ExceptionLib;
pub use exception::{THROWABLE_TYPES, is_throwable_type};

use std::collections::HashMap;
use crate::vm::value::Value;

/// 标准库函数类型
pub type StdlibFn = fn(&[Value]) -> Result<Value, String>;

/// 标准库模块接口
pub trait StdlibModule {
    /// 获取模块名称（如 "std.Vmtest"）
    fn name(&self) -> &'static str;
    
    /// 获取导出的函数列表
    fn exports(&self) -> Vec<&'static str>;
    
    /// 调用函数
    fn call(&self, name: &str, args: &[Value]) -> Result<Value, String>;
}

/// 标准库注册表
pub struct StdlibRegistry {
    modules: HashMap<String, Box<dyn StdlibModule>>,
}

impl StdlibRegistry {
    /// 创建新的注册表
    pub fn new() -> Self {
        let mut registry = Self {
            modules: HashMap::new(),
        };
        
        // 注册内置模块
        registry.register(Box::new(VmTestLib::new()));
        registry.register(Box::new(ExceptionLib::new()));
        
        registry
    }
    
    /// 注册模块
    pub fn register(&mut self, module: Box<dyn StdlibModule>) {
        let name = module.name().to_string();
        self.modules.insert(name, module);
    }
    
    /// 获取模块
    pub fn get(&self, name: &str) -> Option<&dyn StdlibModule> {
        self.modules.get(name).map(|m| m.as_ref())
    }
    
    /// 检查模块是否存在
    pub fn has_module(&self, name: &str) -> bool {
        self.modules.contains_key(name)
    }
    
    /// 调用模块函数
    pub fn call(&self, module: &str, func: &str, args: &[Value]) -> Result<Value, String> {
        let module = self.modules.get(module)
            .ok_or_else(|| format!("Module not found: {}", module))?;
        module.call(func, args)
    }
}

impl Default for StdlibRegistry {
    fn default() -> Self {
        Self::new()
    }
}
