//! std.Vmtest 模块
//! 
//! 提供虚拟机级别的测试功能

use super::StdlibModule;
use crate::vm::value::Value;

/// std.Vmtest 标准库
pub struct VmTestLib;

impl VmTestLib {
    pub fn new() -> Self {
        Self
    }
    
    /// 断言条件为真
    fn assert(args: &[Value]) -> Result<Value, String> {
        if args.is_empty() {
            return Err("assert requires at least 1 argument".to_string());
        }
        
        let condition = args[0].as_bool()
            .ok_or("assert first argument must be a boolean")?;
        
        if !condition {
            let message = if args.len() > 1 {
                args[1].as_string().cloned().unwrap_or_else(|| "Assertion failed".to_string())
            } else {
                "Assertion failed".to_string()
            };
            return Err(message);
        }
        
        Ok(Value::null())
    }
    
    /// 断言两个值相等
    fn assert_equal(args: &[Value]) -> Result<Value, String> {
        if args.len() < 2 {
            return Err("assertEqual requires 2 arguments".to_string());
        }
        
        let actual = &args[0];
        let expected = &args[1];
        
        if actual != expected {
            let message = if args.len() > 2 {
                args[2].as_string().cloned()
                    .unwrap_or_else(|| format!("Expected {:?}, but got {:?}", expected, actual))
            } else {
                format!("Expected {:?}, but got {:?}", expected, actual)
            };
            return Err(message);
        }
        
        Ok(Value::null())
    }
    
    /// 断言条件为真
    fn assert_true(args: &[Value]) -> Result<Value, String> {
        if args.is_empty() {
            return Err("assertTrue requires at least 1 argument".to_string());
        }
        
        let condition = args[0].as_bool()
            .ok_or("assertTrue argument must be a boolean")?;
        
        if !condition {
            let message = if args.len() > 1 {
                args[1].as_string().cloned().unwrap_or_else(|| "Expected true, but got false".to_string())
            } else {
                "Expected true, but got false".to_string()
            };
            return Err(message);
        }
        
        Ok(Value::null())
    }
    
    /// 断言条件为假
    fn assert_false(args: &[Value]) -> Result<Value, String> {
        if args.is_empty() {
            return Err("assertFalse requires at least 1 argument".to_string());
        }
        
        let condition = args[0].as_bool()
            .ok_or("assertFalse argument must be a boolean")?;
        
        if condition {
            let message = if args.len() > 1 {
                args[1].as_string().cloned().unwrap_or_else(|| "Expected false, but got true".to_string())
            } else {
                "Expected false, but got true".to_string()
            };
            return Err(message);
        }
        
        Ok(Value::null())
    }
    
    /// 断言值为 null
    fn assert_null(args: &[Value]) -> Result<Value, String> {
        if args.is_empty() {
            return Err("assertNull requires at least 1 argument".to_string());
        }
        
        if !args[0].is_null() {
            let message = if args.len() > 1 {
                args[1].as_string().cloned().unwrap_or_else(|| "Expected null".to_string())
            } else {
                format!("Expected null, but got {:?}", args[0])
            };
            return Err(message);
        }
        
        Ok(Value::null())
    }
    
    /// 断言值不为 null
    fn assert_not_null(args: &[Value]) -> Result<Value, String> {
        if args.is_empty() {
            return Err("assertNotNull requires at least 1 argument".to_string());
        }
        
        if args[0].is_null() {
            let message = if args.len() > 1 {
                args[1].as_string().cloned().unwrap_or_else(|| "Expected non-null value".to_string())
            } else {
                "Expected non-null value, but got null".to_string()
            };
            return Err(message);
        }
        
        Ok(Value::null())
    }
    
    /// 测试失败
    fn fail(args: &[Value]) -> Result<Value, String> {
        let message = if !args.is_empty() {
            args[0].as_string().cloned().unwrap_or_else(|| "Test failed".to_string())
        } else {
            "Test failed".to_string()
        };
        Err(message)
    }
}

impl StdlibModule for VmTestLib {
    fn name(&self) -> &'static str {
        "std.Vmtest"
    }
    
    fn exports(&self) -> Vec<&'static str> {
        vec![
            "assert",
            "assertEqual",
            "assertTrue",
            "assertFalse",
            "assertNull",
            "assertNotNull",
            "fail",
        ]
    }
    
    fn call(&self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "assert" => Self::assert(args),
            "assertEqual" => Self::assert_equal(args),
            "assertTrue" => Self::assert_true(args),
            "assertFalse" => Self::assert_false(args),
            "assertNull" => Self::assert_null(args),
            "assertNotNull" => Self::assert_not_null(args),
            "fail" => Self::fail(args),
            _ => Err(format!("Unknown function: {}", name)),
        }
    }
}
