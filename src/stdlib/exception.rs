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
use crate::vm::value::Value;

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

/// std.lang.Exception 标准库
pub struct ExceptionLib;

impl ExceptionLib {
    pub fn new() -> Self {
        Self
    }
    
    /// 创建 Throwable
    fn create_throwable(args: &[Value]) -> Result<Value, String> {
        let message = if !args.is_empty() {
            args[0].as_string().map(|s| s.clone()).unwrap_or_default()
        } else {
            String::new()
        };
        
        Ok(Value::string(format!("Throwable: {}", message)))
    }
    
    /// 创建 Error
    fn create_error(args: &[Value]) -> Result<Value, String> {
        let message = if !args.is_empty() {
            args[0].as_string().map(|s| s.clone()).unwrap_or_default()
        } else {
            String::new()
        };
        
        Ok(Value::string(format!("Error: {}", message)))
    }
    
    /// 创建一个异常对象
    fn create_exception(args: &[Value]) -> Result<Value, String> {
        let message = if !args.is_empty() {
            args[0].as_string().map(|s| s.clone()).unwrap_or_default()
        } else {
            String::new()
        };
        
        Ok(Value::string(format!("Exception: {}", message)))
    }
    
    /// 创建 RuntimeException
    fn runtime_exception(args: &[Value]) -> Result<Value, String> {
        let message = if !args.is_empty() {
            args[0].as_string().map(|s| s.clone()).unwrap_or_else(|| "".to_string())
        } else {
            "".to_string()
        };
        
        Ok(Value::string(format!("RuntimeException: {}", message)))
    }
    
    /// 创建 NullPointerException
    fn null_pointer_exception(args: &[Value]) -> Result<Value, String> {
        let message = if !args.is_empty() {
            args[0].as_string().map(|s| s.clone()).unwrap_or_else(|| "null value accessed".to_string())
        } else {
            "null value accessed".to_string()
        };
        
        Ok(Value::string(format!("NullPointerException: {}", message)))
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
        
        Ok(Value::string(format!(
            "IndexOutOfBoundsException: Index {} out of bounds for length {}",
            index, length
        )))
    }
    
    /// 创建 IllegalArgumentException
    fn illegal_argument(args: &[Value]) -> Result<Value, String> {
        let message = if !args.is_empty() {
            args[0].as_string().map(|s| s.clone()).unwrap_or_else(|| "illegal argument".to_string())
        } else {
            "illegal argument".to_string()
        };
        
        Ok(Value::string(format!("IllegalArgumentException: {}", message)))
    }
    
    /// 创建 ArithmeticException
    fn arithmetic_exception(args: &[Value]) -> Result<Value, String> {
        let message = if !args.is_empty() {
            args[0].as_string().map(|s| s.clone()).unwrap_or_else(|| "arithmetic error".to_string())
        } else {
            "arithmetic error".to_string()
        };
        
        Ok(Value::string(format!("ArithmeticException: {}", message)))
    }
    
    /// 创建 IOException
    fn io_exception(args: &[Value]) -> Result<Value, String> {
        let message = if !args.is_empty() {
            args[0].as_string().map(|s| s.clone()).unwrap_or_else(|| "I/O error".to_string())
        } else {
            "I/O error".to_string()
        };
        
        Ok(Value::string(format!("IOException: {}", message)))
    }
    
    /// 检查值是否是 Throwable（通过字符串格式或类实例判断）
    fn is_throwable(args: &[Value]) -> Result<Value, String> {
        if args.is_empty() {
            return Ok(Value::bool(false));
        }
        
        // 检查是否是类实例
        if let Some(instance) = args[0].as_class() {
            let class_name = &instance.borrow().class_name;
            return Ok(Value::bool(is_throwable_type(class_name)));
        }
        
        // 检查是否是字符串格式的异常
        if let Some(s) = args[0].as_string() {
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
            let class_name = &instance.borrow().class_name;
            return Ok(Value::bool(exception_types.contains(&class_name.as_str())));
        }
        
        // 检查是否是字符串格式的异常
        if let Some(s) = args[0].as_string() {
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
        
        if let Some(s) = args[0].as_string() {
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
        
        if let Some(s) = args[0].as_string() {
            if let Some(colon_pos) = s.find(':') {
                let msg = s[colon_pos + 1..].trim();
                return Ok(Value::string(msg.to_string()));
            }
        }
        
        Ok(Value::string("".to_string()))
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
            "getExceptionType",
            "getExceptionMessage",
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
            "getExceptionType" => Self::get_exception_type(args),
            "getExceptionMessage" => Self::get_exception_message(args),
            _ => Err(format!("Unknown function: {}", name)),
        }
    }
}
