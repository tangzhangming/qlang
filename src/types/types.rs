//! 类型定义
//! 
//! 定义语言中的所有类型

#![allow(dead_code)]

use std::fmt;

/// 类型表示
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    // ============ 原始类型 ============
    /// 平台相关整数（32位系统为i32，64位系统为i64）
    Int,
    /// 平台相关无符号整数
    Uint,
    /// 8位有符号整数
    I8,
    /// 16位有符号整数
    I16,
    /// 32位有符号整数
    I32,
    /// 64位有符号整数
    I64,
    /// 8位无符号整数
    U8,
    /// 16位无符号整数
    U16,
    /// 32位无符号整数
    U32,
    /// 64位无符号整数
    U64,
    /// 32位浮点数
    F32,
    /// 64位浮点数
    F64,
    /// 布尔类型
    Bool,
    /// 字节类型（u8 的别名）
    Byte,
    /// Unicode 字符（4字节）
    Char,
    /// 字符串类型（引用类型，不可变）
    String,
    
    // ============ 特殊类型 ============
    /// 空类型（无返回值的函数）
    Void,
    /// 空值类型
    Null,
    /// 安全的顶类型，可接收任何值
    Unknown,
    /// 动态类型，跳过编译时类型检查
    Dynamic,
    
    // ============ 复合类型 ============
    /// 数组类型（固定长度）
    Array {
        element_type: Box<Type>,
        size: usize,
    },
    /// 切片类型（动态长度）
    Slice {
        element_type: Box<Type>,
    },
    /// Map 类型
    Map {
        key_type: Box<Type>,
        value_type: Box<Type>,
    },
    /// 函数/闭包类型
    Function {
        param_types: Vec<Type>,
        return_type: Box<Type>,
    },
    /// 可空类型
    Nullable(Box<Type>),
    /// 指针类型
    Pointer(Box<Type>),
    
    // ============ 用户定义类型 ============
    /// 类类型
    Class(String),
    /// 结构体类型
    Struct(String),
    /// 接口类型
    Interface(String),
    /// 枚举类型
    Enum(String),
    /// 类型别名（解析后的实际类型）
    Alias {
        name: String,
        actual_type: Box<Type>,
    },
    
    // ============ 泛型相关 ============
    /// 泛型参数（如 T）
    TypeParameter(String),
    /// 泛型实例化（如 List<int>）
    Generic {
        base_type: Box<Type>,
        type_args: Vec<Type>,
    },
    
    // ============ 特殊标记 ============
    /// 类型推导占位符（编译器内部使用）
    Infer,
    /// 错误类型（类型检查失败时使用）
    Error,
}

impl Type {
    /// 判断是否是整数类型
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            Type::Int | Type::Uint | 
            Type::I8 | Type::I16 | Type::I32 | Type::I64 |
            Type::U8 | Type::U16 | Type::U32 | Type::U64 |
            Type::Byte
        )
    }
    
    /// 判断是否是有符号整数
    pub fn is_signed_integer(&self) -> bool {
        matches!(
            self,
            Type::Int | Type::I8 | Type::I16 | Type::I32 | Type::I64
        )
    }
    
    /// 判断是否是无符号整数
    pub fn is_unsigned_integer(&self) -> bool {
        matches!(
            self,
            Type::Uint | Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::Byte
        )
    }
    
    /// 判断是否是浮点类型
    pub fn is_float(&self) -> bool {
        matches!(self, Type::F32 | Type::F64)
    }
    
    /// 判断是否是数值类型
    pub fn is_numeric(&self) -> bool {
        self.is_integer() || self.is_float()
    }
    
    /// 判断是否是原始类型
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            Type::Int | Type::Uint |
            Type::I8 | Type::I16 | Type::I32 | Type::I64 |
            Type::U8 | Type::U16 | Type::U32 | Type::U64 |
            Type::F32 | Type::F64 |
            Type::Bool | Type::Byte | Type::Char | Type::String
        )
    }
    
    /// 判断是否是值类型（分配在栈上）
    pub fn is_value_type(&self) -> bool {
        match self {
            // 原始类型是值类型
            Type::Int | Type::Uint |
            Type::I8 | Type::I16 | Type::I32 | Type::I64 |
            Type::U8 | Type::U16 | Type::U32 | Type::U64 |
            Type::F32 | Type::F64 |
            Type::Bool | Type::Byte | Type::Char => true,
            // 数组是值类型
            Type::Array { .. } => true,
            // 结构体是值类型
            Type::Struct(_) => true,
            // 其他都是引用类型
            _ => false,
        }
    }
    
    /// 判断是否是引用类型（分配在堆上）
    pub fn is_reference_type(&self) -> bool {
        !self.is_value_type()
    }
    
    /// 判断是否是可空类型
    pub fn is_nullable(&self) -> bool {
        matches!(self, Type::Nullable(_))
    }
    
    /// 获取可空类型的内部类型
    pub fn unwrap_nullable(&self) -> Option<&Type> {
        if let Type::Nullable(inner) = self {
            Some(inner)
        } else {
            None
        }
    }
    
    /// 判断类型是否可以赋值给目标类型
    pub fn is_assignable_to(&self, target: &Type) -> bool {
        // 相同类型可以赋值
        if self == target {
            return true;
        }
        
        // null 可以赋值给可空类型
        if matches!(self, Type::Null) && target.is_nullable() {
            return true;
        }
        
        // 非空类型可以赋值给对应的可空类型
        if let Type::Nullable(inner) = target {
            if self == inner.as_ref() {
                return true;
            }
        }
        
        // unknown 可以接收任何类型
        if matches!(target, Type::Unknown) {
            return true;
        }
        
        // dynamic 可以接收任何类型
        if matches!(target, Type::Dynamic) {
            return true;
        }
        
        // dynamic 可以赋值给任何类型（运行时检查）
        if matches!(self, Type::Dynamic) {
            return true;
        }
        
        // 数值类型之间的隐式转换（只允许扩展转换）
        if self.is_numeric() && target.is_numeric() {
            return self.can_implicit_convert_to(target);
        }
        
        false
    }
    
    /// 判断是否可以隐式转换到目标类型
    fn can_implicit_convert_to(&self, target: &Type) -> bool {
        // 获取类型的"大小"等级
        let self_rank = self.numeric_rank();
        let target_rank = target.numeric_rank();
        
        // 只允许向更大的类型转换
        if let (Some(s), Some(t)) = (self_rank, target_rank) {
            // 整数到浮点数总是允许
            if self.is_integer() && target.is_float() {
                return true;
            }
            // 同类型族内，只允许扩展
            if self.is_integer() && target.is_integer() {
                // 有符号到无符号需要显式转换
                if self.is_signed_integer() != target.is_signed_integer() {
                    return false;
                }
                return s <= t;
            }
            if self.is_float() && target.is_float() {
                return s <= t;
            }
        }
        false
    }
    
    /// 获取数值类型的等级（用于隐式转换判断）
    fn numeric_rank(&self) -> Option<u8> {
        match self {
            Type::I8 | Type::U8 | Type::Byte => Some(1),
            Type::I16 | Type::U16 => Some(2),
            Type::I32 | Type::U32 => Some(3),
            Type::Int | Type::Uint => Some(4), // 平台相关，假设为最大
            Type::I64 | Type::U64 => Some(5),
            Type::F32 => Some(6),
            Type::F64 => Some(7),
            _ => None,
        }
    }
    
    /// 获取类型的默认值（用于变量初始化）
    pub fn default_value_description(&self) -> &'static str {
        match self {
            Type::Int | Type::Uint |
            Type::I8 | Type::I16 | Type::I32 | Type::I64 |
            Type::U8 | Type::U16 | Type::U32 | Type::U64 |
            Type::Byte => "0",
            Type::F32 | Type::F64 => "0.0",
            Type::Bool => "false",
            Type::Char => "'\\0'",
            Type::String => "\"\"",
            _ => "null",
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Uint => write!(f, "uint"),
            Type::I8 => write!(f, "i8"),
            Type::I16 => write!(f, "i16"),
            Type::I32 => write!(f, "i32"),
            Type::I64 => write!(f, "i64"),
            Type::U8 => write!(f, "u8"),
            Type::U16 => write!(f, "u16"),
            Type::U32 => write!(f, "u32"),
            Type::U64 => write!(f, "u64"),
            Type::F32 => write!(f, "f32"),
            Type::F64 => write!(f, "f64"),
            Type::Bool => write!(f, "bool"),
            Type::Byte => write!(f, "byte"),
            Type::Char => write!(f, "char"),
            Type::String => write!(f, "string"),
            Type::Void => write!(f, "void"),
            Type::Null => write!(f, "null"),
            Type::Unknown => write!(f, "unknown"),
            Type::Dynamic => write!(f, "dynamic"),
            Type::Array { element_type, size } => write!(f, "{}[{}]", element_type, size),
            Type::Slice { element_type } => write!(f, "{}[]", element_type),
            Type::Map { key_type, value_type } => write!(f, "map[{}]{}", key_type, value_type),
            Type::Function { param_types, return_type } => {
                write!(f, "fn(")?;
                for (i, t) in param_types.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", t)?;
                }
                write!(f, ")")?;
                if **return_type != Type::Void {
                    write!(f, " {}", return_type)?;
                }
                Ok(())
            }
            Type::Nullable(inner) => write!(f, "{}?", inner),
            Type::Pointer(inner) => write!(f, "*{}", inner),
            Type::Class(name) => write!(f, "{}", name),
            Type::Struct(name) => write!(f, "{}", name),
            Type::Interface(name) => write!(f, "{}", name),
            Type::Enum(name) => write!(f, "{}", name),
            Type::Alias { name, .. } => write!(f, "{}", name),
            Type::TypeParameter(name) => write!(f, "{}", name),
            Type::Generic { base_type, type_args } => {
                write!(f, "{}<", base_type)?;
                for (i, t) in type_args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", t)?;
                }
                write!(f, ">")
            }
            Type::Infer => write!(f, "_"),
            Type::Error => write!(f, "<error>"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_is_integer() {
        assert!(Type::Int.is_integer());
        assert!(Type::I32.is_integer());
        assert!(Type::U64.is_integer());
        assert!(!Type::F64.is_integer());
        assert!(!Type::Bool.is_integer());
    }
    
    #[test]
    fn test_is_numeric() {
        assert!(Type::Int.is_numeric());
        assert!(Type::F64.is_numeric());
        assert!(!Type::Bool.is_numeric());
        assert!(!Type::String.is_numeric());
    }
    
    #[test]
    fn test_nullable() {
        let nullable_int = Type::Nullable(Box::new(Type::Int));
        assert!(nullable_int.is_nullable());
        assert_eq!(nullable_int.unwrap_nullable(), Some(&Type::Int));
        assert!(!Type::Int.is_nullable());
    }
    
    #[test]
    fn test_assignable() {
        // 相同类型
        assert!(Type::Int.is_assignable_to(&Type::Int));
        
        // null 到可空类型
        let nullable_int = Type::Nullable(Box::new(Type::Int));
        assert!(Type::Null.is_assignable_to(&nullable_int));
        
        // 非空到可空
        assert!(Type::Int.is_assignable_to(&nullable_int));
        
        // 任何类型到 unknown
        assert!(Type::Int.is_assignable_to(&Type::Unknown));
        assert!(Type::String.is_assignable_to(&Type::Unknown));
    }
    
    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Type::Int), "int");
        assert_eq!(format!("{}", Type::Nullable(Box::new(Type::String))), "string?");
        assert_eq!(
            format!("{}", Type::Array { element_type: Box::new(Type::Int), size: 10 }),
            "int[10]"
        );
        assert_eq!(
            format!("{}", Type::Map { 
                key_type: Box::new(Type::String), 
                value_type: Box::new(Type::Int) 
            }),
            "map[string]int"
        );
    }
}
