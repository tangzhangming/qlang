//! std.lang 异常模块
//! 
//! 提供异常相关的内置函数
//! 
//! Java/Kotlin 风格的异常层次结构:
//! - Throwable (基类)
//!   - Error (严重错误)
//!     - AssertionError
//!     - OutOfMemoryError
//!     - StackOverflowError
//!   - Exception (可恢复异常)
//!     - RuntimeException
//!       - NullPointerException
//!       - IndexOutOfBoundsException
//!       - IllegalArgumentException
//!       - ArithmeticException
//!       - ClassCastException
//!       - ...
//!     - IOException
//!       - FileNotFoundException
//!       - ...

use super::StdlibModule;
use crate::vm::value::{Value, ClassInstance};
use parking_lot::Mutex;
use std::sync::Arc;

/// Throwable 类型列表（用于运行时类型检查）
pub const THROWABLE_TYPES: &[&str] = &[
    // Throwable 基类
    "Throwable",
    // Error 分支
    "Error",
    "AssertionError",
    "OutOfMemoryError",
    "StackOverflowError",
    // Exception 分支
    "Exception",
    // RuntimeException 分支
    "RuntimeException",
    "NullPointerException",
    "IndexOutOfBoundsException",
    "IllegalArgumentException",
    "ArithmeticException",
    "ClassCastException",
    "UnsupportedOperationException",
    "IllegalStateException",
    "NumberFormatException",
    // IOException 分支
    "IOException",
    "FileNotFoundException",
    "FileAlreadyExistsException",
    "PermissionDeniedException",
    "EOFException",
    "NetworkException",
    "TimeoutException",
];

/// 检查类型名是否是 Throwable 或其子类
pub fn is_throwable_type(type_name: &str) -> bool {
    THROWABLE_TYPES.contains(&type_name)
}

/// 获取异常类的父类
pub fn get_exception_parent(class_name: &str) -> Option<&'static str> {
    match class_name {
        // Throwable 是根
        "Throwable" => None,
        // Error 分支
        "Error" => Some("Throwable"),
        "AssertionError" | "OutOfMemoryError" | "StackOverflowError" => Some("Error"),
        // Exception 分支
        "Exception" => Some("Throwable"),
        // RuntimeException 分支
        "RuntimeException" => Some("Exception"),
        "NullPointerException" | "IndexOutOfBoundsException" | "IllegalArgumentException" |
        "ArithmeticException" | "ClassCastException" | "UnsupportedOperationException" |
        "IllegalStateException" | "NumberFormatException" => Some("RuntimeException"),
        // IOException 分支
        "IOException" => Some("Exception"),
        "FileNotFoundException" | "FileAlreadyExistsException" | "PermissionDeniedException" |
        "EOFException" | "NetworkException" | "TimeoutException" => Some("IOException"),
        _ => None,
    }
}

/// std.lang.Exception 标准库
pub struct ExceptionLib;

impl ExceptionLib {
    pub fn new() -> Self {
        Self
    }
    
    /// 创建异常类实例的辅助函数
    fn create_exception_instance(class_name: &str, message: String, cause: Option<Value>) -> Value {
        let mut fields = std::collections::HashMap::new();
        
        // message 字段
        fields.insert("message".to_string(), Value::string(message.clone()));
        
        // cause 字段（可选，用于异常链）
        if let Some(c) = cause {
            fields.insert("cause".to_string(), c);
        } else {
            fields.insert("cause".to_string(), Value::null());
        }
        
        // stackTrace 字段（空数组，运行时填充）
        // 注意：需要使用正确的数组创建方式
        fields.insert("stackTrace".to_string(), Value::null()); // 运行时填充
        
        // 创建类实例
        let instance = ClassInstance {
            class_name: class_name.to_string(),
            parent_class: get_exception_parent(class_name).map(|s| s.to_string()),
            fields,
        };
        
        Value::class(Arc::new(Mutex::new(instance)))
    }
    
    /// 创建 Throwable
    fn create_throwable(args: &[Value]) -> Result<Value, String> {
        let message = if !args.is_empty() {
            args[0].as_string().cloned().unwrap_or_default()
        } else {
            String::new()
        };
        
        let cause = args.get(1).cloned();
        Ok(Self::create_exception_instance("Throwable", message, cause))
    }
    
    /// 创建 Error
    fn create_error(args: &[Value]) -> Result<Value, String> {
        let message = if !args.is_empty() {
            args[0].as_string().cloned().unwrap_or_default()
        } else {
            String::new()
        };
        
        let cause = args.get(1).cloned();
        Ok(Self::create_exception_instance("Error", message, cause))
    }
    
    /// 创建一个异常对象
    fn create_exception(args: &[Value]) -> Result<Value, String> {
        let message = if !args.is_empty() {
            args[0].as_string().cloned().unwrap_or_default()
        } else {
            String::new()
        };
        
        let cause = args.get(1).cloned();
        Ok(Self::create_exception_instance("Exception", message, cause))
    }
    
    /// 创建 RuntimeException
    fn runtime_exception(args: &[Value]) -> Result<Value, String> {
        let message = if !args.is_empty() {
            args[0].as_string().cloned().unwrap_or_else(|| "".to_string())
        } else {
            "".to_string()
        };
        
        let cause = args.get(1).cloned();
        Ok(Self::create_exception_instance("RuntimeException", message, cause))
    }
    
    /// 创建 NullPointerException
    fn null_pointer_exception(args: &[Value]) -> Result<Value, String> {
        let message = if !args.is_empty() {
            args[0].as_string().cloned().unwrap_or_else(|| "null value accessed".to_string())
        } else {
            "null value accessed".to_string()
        };
        
        Ok(Self::create_exception_instance("NullPointerException", message, None))
    }
    
    /// 创建 IndexOutOfBoundsException
    fn index_out_of_bounds(args: &[Value]) -> Result<Value, String> {
        let (index, length) = if args.len() >= 2 {
            let idx = args[0].as_int().unwrap_or(0);
            let len = args[1].as_int().unwrap_or(0);
            (idx, len)
        } else {
            (0, 0)
        };
        
        let message = format!("Index {} out of bounds for length {}", index, length);
        Ok(Self::create_exception_instance("IndexOutOfBoundsException", message, None))
    }
    
    /// 创建 IllegalArgumentException
    fn illegal_argument(args: &[Value]) -> Result<Value, String> {
        let message = if !args.is_empty() {
            args[0].as_string().cloned().unwrap_or_else(|| "illegal argument".to_string())
        } else {
            "illegal argument".to_string()
        };
        
        Ok(Self::create_exception_instance("IllegalArgumentException", message, None))
    }
    
    /// 创建 ArithmeticException
    fn arithmetic_exception(args: &[Value]) -> Result<Value, String> {
        let message = if !args.is_empty() {
            args[0].as_string().cloned().unwrap_or_else(|| "arithmetic error".to_string())
        } else {
            "arithmetic error".to_string()
        };
        
        Ok(Self::create_exception_instance("ArithmeticException", message, None))
    }
    
    /// 创建 IOException
    fn io_exception(args: &[Value]) -> Result<Value, String> {
        let message = if !args.is_empty() {
            args[0].as_string().cloned().unwrap_or_else(|| "I/O error".to_string())
        } else {
            "I/O error".to_string()
        };
        
        let cause = args.get(1).cloned();
        Ok(Self::create_exception_instance("IOException", message, cause))
    }
    
    /// 检查值是否是 Throwable（通过字符串格式或类实例判断）
    fn is_throwable(args: &[Value]) -> Result<Value, String> {
        if args.is_empty() {
            return Ok(Value::bool(false));
        }
        
        // 检查是否是类实例
        if let Some(instance) = args[0].as_class() {
            let instance_ref: &Arc<Mutex<ClassInstance>> = instance;
            let class_name = &instance_ref.lock().class_name;
            return Ok(Value::bool(is_throwable_type(class_name)));
        }
        
        // 检查是否是字符串格式的异常
        if let Some(s) = args[0].as_string() {
            let s: &String = s;
            for throwable_type in THROWABLE_TYPES {
                let prefix = format!("{}:", throwable_type);
                if s.starts_with(&prefix) || s == *throwable_type {
                    return Ok(Value::bool(true));
                }
            }
        }
        
        Ok(Value::bool(false))
    }
    
    /// 检查值是否是 Exception（不包括 Error）
    fn is_exception(args: &[Value]) -> Result<Value, String> {
        if args.is_empty() {
            return Ok(Value::bool(false));
        }
        
        let exception_types = [
            "Exception", "RuntimeException", "NullPointerException",
            "IndexOutOfBoundsException", "IllegalArgumentException",
            "ArithmeticException", "ClassCastException",
            "UnsupportedOperationException", "IllegalStateException",
            "NumberFormatException", "IOException", "FileNotFoundException",
            "FileAlreadyExistsException", "PermissionDeniedException",
            "EOFException", "NetworkException", "TimeoutException",
        ];
        
        // 检查是否是类实例
        if let Some(instance) = args[0].as_class() {
            let instance_ref: &Arc<Mutex<ClassInstance>> = instance;
            let class_name = &instance_ref.lock().class_name;
            return Ok(Value::bool(exception_types.contains(&class_name.as_str())));
        }
        
        // 检查是否是字符串格式的异常
        if let Some(s) = args[0].as_string() {
            let s: &String = s;
            for et in exception_types {
                let prefix = format!("{}:", et);
                if s.starts_with(&prefix) || s == et {
                    return Ok(Value::bool(true));
                }
            }
        }
        
        Ok(Value::bool(false))
    }
    
    /// 获取异常类型
    fn get_exception_type(args: &[Value]) -> Result<Value, String> {
        if args.is_empty() {
            return Ok(Value::string("Unknown".to_string()));
        }
        
        // 优先检查类实例
        if let Some(instance) = args[0].as_class() {
            let instance_ref: &Arc<Mutex<ClassInstance>> = instance;
            let class_name = instance_ref.lock().class_name.clone();
            return Ok(Value::string(class_name));
        }
        
        // 兼容旧的字符串格式
        if let Some(s) = args[0].as_string() {
            let s: &String = s;
            if let Some(colon_pos) = s.find(':') {
                return Ok(Value::string(s[..colon_pos].to_string()));
            }
            return Ok(Value::string(s.clone()));
        }
        
        Ok(Value::string("Unknown".to_string()))
    }
    
    /// 获取异常消息
    fn get_exception_message(args: &[Value]) -> Result<Value, String> {
        if args.is_empty() {
            return Ok(Value::string("".to_string()));
        }
        
        // 优先检查类实例
        if let Some(instance) = args[0].as_class() {
            let instance_ref: &Arc<Mutex<ClassInstance>> = instance;
            let guard = instance_ref.lock();
            if let Some(msg) = guard.fields.get("message") {
                if let Some(s) = msg.as_string() {
                    return Ok(Value::string(s.clone()));
                }
            }
            return Ok(Value::string("".to_string()));
        }
        
        // 兼容旧的字符串格式
        if let Some(s) = args[0].as_string() {
            let s: &String = s;
            if let Some(colon_pos) = s.find(':') {
                let msg = s[colon_pos + 1..].trim();
                return Ok(Value::string(msg.to_string()));
            }
        }
        
        Ok(Value::string("".to_string()))
    }
    
    /// 获取异常原因（cause）
    fn get_exception_cause(args: &[Value]) -> Result<Value, String> {
        if args.is_empty() {
            return Ok(Value::null());
        }
        
        if let Some(instance) = args[0].as_class() {
            let instance_ref: &Arc<Mutex<ClassInstance>> = instance;
            let guard = instance_ref.lock();
            if let Some(cause) = guard.fields.get("cause") {
                return Ok(cause.clone());
            }
        }
        
        Ok(Value::null())
    }
    
    /// 检查异常是否是指定类型或其子类
    fn is_instance_of(args: &[Value]) -> Result<Value, String> {
        if args.len() < 2 {
            return Ok(Value::bool(false));
        }
        
        let target_type = if let Some(s) = args[1].as_string() {
            s.clone()
        } else {
            return Ok(Value::bool(false));
        };
        
        // 检查类实例
        if let Some(instance) = args[0].as_class() {
            let instance_ref: &Arc<Mutex<ClassInstance>> = instance;
            let guard = instance_ref.lock();
            let mut current_type = Some(guard.class_name.clone());
            
            while let Some(ct) = current_type {
                if ct == target_type {
                    return Ok(Value::bool(true));
                }
                // 查找父类
                current_type = get_exception_parent(&ct).map(|s| s.to_string());
            }
        }
        
        Ok(Value::bool(false))
    }
}

impl StdlibModule for ExceptionLib {
    fn name(&self) -> &'static str {
        "std.lang"
    }
    
    fn exports(&self) -> Vec<&'static str> {
        vec![
            // 基础类
            "Throwable",
            "Error",
            "Exception",
            // RuntimeException 分支
            "RuntimeException",
            "NullPointerException",
            "IndexOutOfBoundsException",
            "IllegalArgumentException",
            "ArithmeticException",
            // IOException 分支
            "IOException",
            // 工具函数
            "isThrowable",
            "isException",
            "isInstanceOf",
            "getExceptionType",
            "getExceptionMessage",
            "getExceptionCause",
        ]
    }
    
    fn call(&self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "Throwable" => Self::create_throwable(args),
            "Error" => Self::create_error(args),
            "Exception" => Self::create_exception(args),
            "RuntimeException" => Self::runtime_exception(args),
            "NullPointerException" => Self::null_pointer_exception(args),
            "IndexOutOfBoundsException" => Self::index_out_of_bounds(args),
            "IllegalArgumentException" => Self::illegal_argument(args),
            "ArithmeticException" => Self::arithmetic_exception(args),
            "IOException" => Self::io_exception(args),
            "isThrowable" => Self::is_throwable(args),
            "isException" => Self::is_exception(args),
            "isInstanceOf" => Self::is_instance_of(args),
            "getExceptionType" => Self::get_exception_type(args),
            "getExceptionMessage" => Self::get_exception_message(args),
            "getExceptionCause" => Self::get_exception_cause(args),
            _ => Err(format!("Unknown function: {}", name)),
        }
    }
}
