//! 标准库模块
//! 
//! 实现用 Rust 编写的内置标准库

mod vmtest;
pub mod exception;
pub mod net;

pub use vmtest::VmTestLib;
pub use exception::ExceptionLib;
pub use exception::{THROWABLE_TYPES, is_throwable_type};
pub use net::NetTcpLib;

use std::collections::HashMap;
use crate::vm::value::Value;

/// 标准库函数类型
pub type StdlibFn = fn(&[Value]) -> Result<Value, String>;

/// 标准库模块接口
pub trait StdlibModule: Send + Sync {
    /// 获取模块名称（如 "std.Vmtest"）
    fn name(&self) -> &'static str;
    
    /// 获取导出的函数列表
    fn exports(&self) -> Vec<&'static str>;
    
    /// 调用函数
    fn call(&self, name: &str, args: &[Value]) -> Result<Value, String>;
    
    /// 检查模块是否包含指定的类
    /// 类名格式：完整类名，如 "std.net.tcp.TCPSocket"
    fn has_class(&self, class_name: &str) -> bool {
        false
    }
    
    /// 创建类实例（构造函数调用）
    /// class_name: 完整类名，如 "std.net.tcp.TCPSocket"
    /// args: 构造函数参数
    fn create_class_instance(&self, class_name: &str, args: &[Value]) -> Result<Value, String> {
        Err(format!("Class '{}' not found in module '{}'", class_name, self.name()))
    }
    
    /// 调用类实例的方法
    /// instance: 类实例的 Value
    /// method_name: 方法名
    /// args: 方法参数（不包含 this）
    fn call_method(&self, instance: &Value, method_name: &str, args: &[Value]) -> Result<Value, String> {
        Err(format!("Method '{}' not found", method_name))
    }
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
        registry.register(Box::new(NetTcpLib::new()));
        
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
    
    /// 查找包含指定类的模块
    /// 返回 (模块名, 模块引用)
    pub fn find_class_module(&self, class_name: &str) -> Option<(&str, &dyn StdlibModule)> {
        for (module_name, module) in &self.modules {
            if module.has_class(class_name) {
                return Some((module_name, module.as_ref()));
            }
        }
        None
    }
    
    /// 创建标准库类实例
    pub fn create_class_instance(&self, class_name: &str, args: &[Value]) -> Result<Value, String> {
        let (_, module) = self.find_class_module(class_name)
            .ok_or_else(|| format!("Class '{}' not found in any standard library module", class_name))?;
        module.create_class_instance(class_name, args)
    }
    
    /// 调用标准库类实例的方法
    pub fn call_class_method(&self, instance: &Value, method_name: &str, args: &[Value]) -> Result<Value, String> {
        // 从实例中提取类名
        if let Some(class_instance) = instance.as_class() {
            let instance_guard = class_instance.lock();
            let class_name = instance_guard.class_name.clone();
            drop(instance_guard);
            
            // 查找对应的模块
            let (_, module) = self.find_class_module(&class_name)
                .ok_or_else(|| format!("Class '{}' not found in any standard library module", class_name))?;
            
            module.call_method(instance, method_name, args)
        } else {
            Err("Value is not a class instance".to_string())
        }
    }
}

impl Default for StdlibRegistry {
    fn default() -> Self {
        Self::new()
    }
}
