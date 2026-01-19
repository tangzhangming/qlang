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
pub use net::NetHttpLib;

use std::collections::HashMap;
use std::sync::Arc;
use crossbeam_channel::{Sender, Receiver, bounded};
use crate::vm::value::Value;

/// 标准库函数类型
pub type StdlibFn = fn(&[Value]) -> Result<Value, String>;

// ============================================================================
// 回调机制支持
// ============================================================================

/// 回调请求类型
/// 用于标准库方法请求VM执行Q语言回调函数
#[derive(Debug)]
pub enum CallbackRequest {
    /// 需要执行回调函数
    Execute {
        /// 回调函数（闭包Value）
        handler: Value,
        /// 回调参数
        args: Vec<Value>,
        /// 响应通道（用于接收回调返回值）
        response_tx: Sender<CallbackResponse>,
    },
    /// 停止回调循环
    Stop,
}

/// 回调响应类型
#[derive(Debug)]
pub enum CallbackResponse {
    /// 回调执行成功，返回结果
    Success(Value),
    /// 回调执行失败
    Error(String),
}

/// 回调通道
/// 用于标准库和VM之间的异步通信
pub struct CallbackChannel {
    /// 请求发送端（标准库使用）
    pub request_tx: Sender<CallbackRequest>,
    /// 请求接收端（VM使用）
    pub request_rx: Receiver<CallbackRequest>,
}

impl CallbackChannel {
    /// 创建新的回调通道
    pub fn new() -> Self {
        let (tx, rx) = bounded(16);
        Self { 
            request_tx: tx, 
            request_rx: rx,
        }
    }
    
    /// 发送回调请求并等待响应
    pub fn call(&self, handler: Value, args: Vec<Value>) -> Result<Value, String> {
        let (response_tx, response_rx) = bounded(1);
        
        self.request_tx.send(CallbackRequest::Execute {
            handler,
            args,
            response_tx,
        }).map_err(|e| format!("Failed to send callback request: {}", e))?;
        
        match response_rx.recv() {
            Ok(CallbackResponse::Success(value)) => Ok(value),
            Ok(CallbackResponse::Error(e)) => Err(e),
            Err(e) => Err(format!("Failed to receive callback response: {}", e)),
        }
    }
    
    /// 发送停止信号
    pub fn stop(&self) -> Result<(), String> {
        self.request_tx.send(CallbackRequest::Stop)
            .map_err(|e| format!("Failed to send stop request: {}", e))
    }
}

impl Default for CallbackChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for CallbackChannel {
    fn clone(&self) -> Self {
        Self {
            request_tx: self.request_tx.clone(),
            request_rx: self.request_rx.clone(),
        }
    }
}

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
    
    /// 检查方法是否需要回调支持
    /// 需要回调的方法（如 HttpServer.listen）会通过回调通道与VM通信
    fn needs_callback(&self, _class_name: &str, _method_name: &str) -> bool {
        false
    }
    
    /// 调用需要回调支持的方法
    /// 当 needs_callback 返回 true 时，VM会调用此方法而不是 call_method
    /// 
    /// # 参数
    /// - instance: 类实例
    /// - method_name: 方法名
    /// - args: 方法参数（第一个参数通常是handler函数）
    /// - callback_channel: 回调通道，用于请求VM执行Q语言函数
    fn call_method_with_callback(
        &self,
        _instance: &Value,
        method_name: &str,
        _args: &[Value],
        _callback_channel: Arc<CallbackChannel>,
    ) -> Result<Value, String> {
        Err(format!("Method '{}' does not support callback", method_name))
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
        registry.register(Box::new(NetHttpLib::new()));
        
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
    
    /// 根据简短类名查找完整类名
    /// 例如：HttpServer -> std.net.http.HttpServer
    pub fn resolve_class_name(&self, short_name: &str) -> Option<String> {
        // 如果已经是完整名称，直接返回
        if short_name.starts_with("std.") {
            if self.find_class_module(short_name).is_some() {
                return Some(short_name.to_string());
            }
        }
        
        // 搜索所有模块，查找匹配的简短类名
        for (module_name, module) in &self.modules {
            // 构造可能的完整类名
            let full_name = format!("{}.{}", module_name, short_name);
            if module.has_class(&full_name) {
                return Some(full_name);
            }
        }
        
        None
    }
    
    /// 创建标准库类实例
    pub fn create_class_instance(&self, class_name: &str, args: &[Value]) -> Result<Value, String> {
        // 先尝试解析完整类名
        let full_name = self.resolve_class_name(class_name)
            .ok_or_else(|| format!("Class '{}' not found in any standard library module", class_name))?;
        
        let (_, module) = self.find_class_module(&full_name)
            .ok_or_else(|| format!("Class '{}' not found in any standard library module", full_name))?;
        module.create_class_instance(&full_name, args)
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
    
    /// 检查方法是否需要回调支持
    pub fn needs_callback(&self, class_name: &str, method_name: &str) -> bool {
        if let Some((_, module)) = self.find_class_module(class_name) {
            module.needs_callback(class_name, method_name)
        } else {
            false
        }
    }
    
    /// 调用需要回调支持的方法
    pub fn call_class_method_with_callback(
        &self,
        instance: &Value,
        method_name: &str,
        args: &[Value],
        callback_channel: Arc<CallbackChannel>,
    ) -> Result<Value, String> {
        // 从实例中提取类名
        if let Some(class_instance) = instance.as_class() {
            let instance_guard = class_instance.lock();
            let class_name = instance_guard.class_name.clone();
            drop(instance_guard);
            
            // 查找对应的模块
            let (_, module) = self.find_class_module(&class_name)
                .ok_or_else(|| format!("Class '{}' not found in any standard library module", class_name))?;
            
            module.call_method_with_callback(instance, method_name, args, callback_channel)
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
