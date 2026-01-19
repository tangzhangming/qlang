//! 类型定义
//! 
//! 定义语言中的所有类型，包括泛型、约束、Trait等

#![allow(dead_code)]

use std::fmt;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// 全局类型变量 ID 生成器
static TYPE_VAR_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 生成唯一的类型变量 ID
pub fn fresh_type_var_id() -> u64 {
    TYPE_VAR_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// 类型约束（如 T: Comparable<T> + Printable）
#[derive(Debug, Clone, PartialEq)]
pub struct TypeBound {
    /// 约束的 trait/interface 名称
    pub trait_name: String,
    /// 如果是泛型 trait，这里是类型参数（如 Comparable<T> 中的 T）
    pub type_args: Vec<Type>,
}

impl TypeBound {
    /// 创建简单约束（无泛型参数）
    pub fn simple(trait_name: impl Into<String>) -> Self {
        Self {
            trait_name: trait_name.into(),
            type_args: Vec::new(),
        }
    }
    
    /// 创建泛型约束
    pub fn generic(trait_name: impl Into<String>, type_args: Vec<Type>) -> Self {
        Self {
            trait_name: trait_name.into(),
            type_args,
        }
    }
}

impl fmt::Display for TypeBound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.trait_name)?;
        if !self.type_args.is_empty() {
            write!(f, "<")?;
            for (i, t) in self.type_args.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", t)?;
            }
            write!(f, ">")?;
        }
        Ok(())
    }
}

/// 泛型类型参数（带约束）
#[derive(Debug, Clone, PartialEq)]
pub struct GenericParam {
    /// 参数名（如 T、K、V）
    pub name: String,
    /// 类型约束列表（如 Comparable<T> + Printable）
    pub bounds: Vec<TypeBound>,
    /// 默认类型（可选）
    pub default: Option<Box<Type>>,
}

impl GenericParam {
    /// 创建无约束的泛型参数
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            bounds: Vec::new(),
            default: None,
        }
    }
    
    /// 创建带约束的泛型参数
    pub fn with_bounds(name: impl Into<String>, bounds: Vec<TypeBound>) -> Self {
        Self {
            name: name.into(),
            bounds,
            default: None,
        }
    }
    
    /// 设置默认类型
    pub fn with_default(mut self, default: Type) -> Self {
        self.default = Some(Box::new(default));
        self
    }
}

impl fmt::Display for GenericParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if !self.bounds.is_empty() {
            write!(f, ": ")?;
            for (i, bound) in self.bounds.iter().enumerate() {
                if i > 0 {
                    write!(f, " + ")?;
                }
                write!(f, "{}", bound)?;
            }
        }
        if let Some(ref default) = self.default {
            write!(f, " = {}", default)?;
        }
        Ok(())
    }
}

/// Where 子句中的约束项
#[derive(Debug, Clone, PartialEq)]
pub struct WhereClause {
    /// 被约束的类型参数名
    pub type_param: String,
    /// 约束列表
    pub bounds: Vec<TypeBound>,
}

impl fmt::Display for WhereClause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.type_param)?;
        for (i, bound) in self.bounds.iter().enumerate() {
            if i > 0 {
                write!(f, " + ")?;
            }
            write!(f, "{}", bound)?;
        }
        Ok(())
    }
}

/// 函数签名（用于 Trait/Interface 方法）
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSignature {
    /// 函数名
    pub name: String,
    /// 泛型参数
    pub type_params: Vec<GenericParam>,
    /// 参数类型
    pub param_types: Vec<Type>,
    /// 参数名（可选，用于文档）
    pub param_names: Vec<String>,
    /// 返回类型
    pub return_type: Type,
    /// where 子句
    pub where_clauses: Vec<WhereClause>,
}

impl FunctionSignature {
    /// 创建简单函数签名
    pub fn new(
        name: impl Into<String>,
        param_types: Vec<Type>,
        return_type: Type,
    ) -> Self {
        Self {
            name: name.into(),
            type_params: Vec::new(),
            param_types,
            param_names: Vec::new(),
            return_type,
            where_clauses: Vec::new(),
        }
    }
}

/// Trait 定义
#[derive(Debug, Clone, PartialEq)]
pub struct TraitDef {
    /// Trait 名称
    pub name: String,
    /// 泛型参数
    pub type_params: Vec<GenericParam>,
    /// 父 Trait（继承）
    pub super_traits: Vec<TypeBound>,
    /// 方法签名
    pub methods: Vec<FunctionSignature>,
    /// 关联类型
    pub associated_types: Vec<AssociatedType>,
    /// 关联常量
    pub associated_consts: Vec<AssociatedConst>,
}

/// 关联类型定义
#[derive(Debug, Clone, PartialEq)]
pub struct AssociatedType {
    /// 类型名
    pub name: String,
    /// 约束
    pub bounds: Vec<TypeBound>,
    /// 默认类型
    pub default: Option<Type>,
}

/// 关联常量定义
#[derive(Debug, Clone, PartialEq)]
pub struct AssociatedConst {
    /// 常量名
    pub name: String,
    /// 类型
    pub ty: Type,
}

/// 接口定义
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceDef {
    /// 接口名称
    pub name: String,
    /// 泛型参数
    pub type_params: Vec<GenericParam>,
    /// 父接口
    pub super_interfaces: Vec<TypeBound>,
    /// 方法签名
    pub methods: Vec<FunctionSignature>,
}

/// Trait 实现
#[derive(Debug, Clone, PartialEq)]
pub struct TraitImpl {
    /// 实现的 Trait
    pub trait_bound: TypeBound,
    /// 实现 Trait 的类型
    pub for_type: Type,
    /// 泛型参数（如 impl<T> Trait for Vec<T>）
    pub type_params: Vec<GenericParam>,
    /// where 子句
    pub where_clauses: Vec<WhereClause>,
    /// 关联类型具体化
    pub associated_types: HashMap<String, Type>,
}

/// 类型变量（用于类型推导）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeVar {
    /// 唯一 ID
    pub id: u64,
    /// 调试名称（可选）
    pub name: Option<String>,
}

impl TypeVar {
    /// 创建新的类型变量
    pub fn fresh() -> Self {
        Self {
            id: fresh_type_var_id(),
            name: None,
        }
    }
    
    /// 创建带名称的类型变量
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            id: fresh_type_var_id(),
            name: Some(name.into()),
        }
    }
}

impl fmt::Display for TypeVar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref name) = self.name {
            write!(f, "?{}", name)
        } else {
            write!(f, "?T{}", self.id)
        }
    }
}

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
    /// Never 类型（永不返回，如 panic）
    Never,
    
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
    /// 元组类型
    Tuple(Vec<Type>),
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
    /// Trait 类型（作为类型约束使用）
    Trait(String),
    /// 枚举类型
    Enum(String),
    /// 类型别名（解析后的实际类型）
    Alias {
        name: String,
        actual_type: Box<Type>,
    },
    
    // ============ 泛型相关 ============
    /// 泛型参数（如 T）- 带约束信息
    TypeParameter {
        name: String,
        bounds: Vec<TypeBound>,
    },
    /// 泛型实例化（如 List<int>）
    Generic {
        base_type: Box<Type>,
        type_args: Vec<Type>,
    },
    /// 类型变量（用于类型推导）
    TypeVar(TypeVar),
    /// 关联类型路径（如 T::Output）
    AssociatedType {
        /// 基类型（如 T）
        base_type: Box<Type>,
        /// 关联类型名（如 Output）
        name: String,
    },
    
    // ============ 特殊标记 ============
    /// 类型推导占位符（编译器内部使用）
    Infer,
    /// 错误类型（类型检查失败时使用）
    Error,
}

/// 类型替换表（用于泛型实例化）
pub type Substitution = HashMap<String, Type>;

impl Type {
    /// 创建简单的泛型参数类型（无约束）
    pub fn type_param(name: impl Into<String>) -> Self {
        Type::TypeParameter {
            name: name.into(),
            bounds: Vec::new(),
        }
    }
    
    /// 创建带约束的泛型参数类型
    pub fn type_param_with_bounds(name: impl Into<String>, bounds: Vec<TypeBound>) -> Self {
        Type::TypeParameter {
            name: name.into(),
            bounds,
        }
    }
    
    /// 创建泛型实例化类型
    pub fn generic(base: Type, args: Vec<Type>) -> Self {
        Type::Generic {
            base_type: Box::new(base),
            type_args: args,
        }
    }
    
    /// 创建类型变量
    pub fn fresh_var() -> Self {
        Type::TypeVar(TypeVar::fresh())
    }
    
    /// 创建带名称的类型变量
    pub fn named_var(name: impl Into<String>) -> Self {
        Type::TypeVar(TypeVar::with_name(name))
    }
    
    /// 应用类型替换
    pub fn substitute(&self, subst: &Substitution) -> Type {
        match self {
            // 原始类型不变
            Type::Int | Type::Uint | Type::I8 | Type::I16 | Type::I32 | Type::I64 |
            Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::F32 | Type::F64 |
            Type::Bool | Type::Byte | Type::Char | Type::String | Type::Void |
            Type::Null | Type::Unknown | Type::Dynamic | Type::Never |
            Type::Infer | Type::Error => self.clone(),
            
            // 类型参数：查找替换
            Type::TypeParameter { name, bounds } => {
                if let Some(replacement) = subst.get(name) {
                    replacement.clone()
                } else {
                    Type::TypeParameter {
                        name: name.clone(),
                        bounds: bounds.iter().map(|b| TypeBound {
                            trait_name: b.trait_name.clone(),
                            type_args: b.type_args.iter().map(|t| t.substitute(subst)).collect(),
                        }).collect(),
                    }
                }
            }
            
            // 泛型实例化：递归替换
            Type::Generic { base_type, type_args } => {
                Type::Generic {
                    base_type: Box::new(base_type.substitute(subst)),
                    type_args: type_args.iter().map(|t| t.substitute(subst)).collect(),
                }
            }
            
            // 复合类型：递归替换
            Type::Array { element_type, size } => {
                Type::Array {
                    element_type: Box::new(element_type.substitute(subst)),
                    size: *size,
                }
            }
            Type::Slice { element_type } => {
                Type::Slice {
                    element_type: Box::new(element_type.substitute(subst)),
                }
            }
            Type::Map { key_type, value_type } => {
                Type::Map {
                    key_type: Box::new(key_type.substitute(subst)),
                    value_type: Box::new(value_type.substitute(subst)),
                }
            }
            Type::Tuple(types) => {
                Type::Tuple(types.iter().map(|t| t.substitute(subst)).collect())
            }
            Type::Function { param_types, return_type } => {
                Type::Function {
                    param_types: param_types.iter().map(|t| t.substitute(subst)).collect(),
                    return_type: Box::new(return_type.substitute(subst)),
                }
            }
            Type::Nullable(inner) => {
                Type::Nullable(Box::new(inner.substitute(subst)))
            }
            Type::Pointer(inner) => {
                Type::Pointer(Box::new(inner.substitute(subst)))
            }
            
            // 用户定义类型：保持不变
            Type::Class(_) | Type::Struct(_) | Type::Interface(_) | 
            Type::Trait(_) | Type::Enum(_) => self.clone(),
            
            // 类型别名：替换实际类型
            Type::Alias { name, actual_type } => {
                Type::Alias {
                    name: name.clone(),
                    actual_type: Box::new(actual_type.substitute(subst)),
                }
            }
            
            // 类型变量：保持不变（由 unification 处理）
            Type::TypeVar(_) => self.clone(),
            
            // 关联类型：递归替换基类型
            Type::AssociatedType { base_type, name } => {
                Type::AssociatedType {
                    base_type: Box::new(base_type.substitute(subst)),
                    name: name.clone(),
                }
            }
        }
    }
    
    /// 收集类型中的所有自由类型变量
    pub fn free_type_vars(&self) -> Vec<TypeVar> {
        let mut vars = Vec::new();
        self.collect_type_vars(&mut vars);
        vars
    }
    
    fn collect_type_vars(&self, vars: &mut Vec<TypeVar>) {
        match self {
            Type::TypeVar(v) => {
                if !vars.contains(v) {
                    vars.push(v.clone());
                }
            }
            Type::Array { element_type, .. } | Type::Slice { element_type } | 
            Type::Nullable(element_type) | Type::Pointer(element_type) => {
                element_type.collect_type_vars(vars);
            }
            Type::Map { key_type, value_type } => {
                key_type.collect_type_vars(vars);
                value_type.collect_type_vars(vars);
            }
            Type::Tuple(types) => {
                for t in types {
                    t.collect_type_vars(vars);
                }
            }
            Type::Function { param_types, return_type } => {
                for t in param_types {
                    t.collect_type_vars(vars);
                }
                return_type.collect_type_vars(vars);
            }
            Type::Generic { base_type, type_args } => {
                base_type.collect_type_vars(vars);
                for t in type_args {
                    t.collect_type_vars(vars);
                }
            }
            Type::Alias { actual_type, .. } => {
                actual_type.collect_type_vars(vars);
            }
            Type::AssociatedType { base_type, .. } => {
                base_type.collect_type_vars(vars);
            }
            Type::TypeParameter { bounds, .. } => {
                for bound in bounds {
                    for t in &bound.type_args {
                        t.collect_type_vars(vars);
                    }
                }
            }
            _ => {}
        }
    }
    
    /// 判断类型是否包含类型变量
    pub fn has_type_vars(&self) -> bool {
        !self.free_type_vars().is_empty()
    }
    
    /// 判断类型是否包含类型参数
    pub fn has_type_params(&self) -> bool {
        match self {
            Type::TypeParameter { .. } => true,
            Type::Array { element_type, .. } | Type::Slice { element_type } |
            Type::Nullable(element_type) | Type::Pointer(element_type) => {
                element_type.has_type_params()
            }
            Type::Map { key_type, value_type } => {
                key_type.has_type_params() || value_type.has_type_params()
            }
            Type::Tuple(types) => types.iter().any(|t| t.has_type_params()),
            Type::Function { param_types, return_type } => {
                param_types.iter().any(|t| t.has_type_params()) || return_type.has_type_params()
            }
            Type::Generic { base_type, type_args } => {
                base_type.has_type_params() || type_args.iter().any(|t| t.has_type_params())
            }
            Type::Alias { actual_type, .. } => actual_type.has_type_params(),
            Type::AssociatedType { base_type, .. } => base_type.has_type_params(),
            _ => false,
        }
    }
    
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
            // 元组是值类型
            Type::Tuple(_) => true,
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
    
    /// 判断类型是否可以赋值给目标类型（基本检查）
    pub fn is_assignable_to(&self, target: &Type) -> bool {
        // 相同类型可以赋值
        if self == target {
            return true;
        }
        
        // Error 类型可以赋值给任何类型（避免级联错误）
        if matches!(self, Type::Error) || matches!(target, Type::Error) {
            return true;
        }
        
        // Never 类型可以赋值给任何类型
        if matches!(self, Type::Never) {
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
            // 递归检查
            if self.is_assignable_to(inner) {
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
    
    /// 获取类型的名称（用于错误消息等）
    pub fn type_name(&self) -> String {
        format!("{}", self)
    }
    
    /// 判断是否是泛型类型（包含类型参数）
    pub fn is_generic(&self) -> bool {
        matches!(self, Type::Generic { .. }) || self.has_type_params()
    }
    
    /// 获取泛型基类型
    pub fn get_base_type(&self) -> &Type {
        match self {
            Type::Generic { base_type, .. } => base_type.get_base_type(),
            _ => self,
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
            Type::Never => write!(f, "never"),
            Type::Array { element_type, size } => write!(f, "{}[{}]", element_type, size),
            Type::Slice { element_type } => write!(f, "{}[]", element_type),
            Type::Map { key_type, value_type } => write!(f, "map[{}]{}", key_type, value_type),
            Type::Tuple(types) => {
                write!(f, "(")?;
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", t)?;
                }
                write!(f, ")")
            }
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
            Type::Trait(name) => write!(f, "{}", name),
            Type::Enum(name) => write!(f, "{}", name),
            Type::Alias { name, .. } => write!(f, "{}", name),
            Type::TypeParameter { name, bounds } => {
                write!(f, "{}", name)?;
                if !bounds.is_empty() {
                    write!(f, ": ")?;
                    for (i, bound) in bounds.iter().enumerate() {
                        if i > 0 {
                            write!(f, " + ")?;
                        }
                        write!(f, "{}", bound)?;
                    }
                }
                Ok(())
            }
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
            Type::TypeVar(v) => write!(f, "{}", v),
            Type::AssociatedType { base_type, name } => {
                write!(f, "{}::{}", base_type, name)
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
    
    #[test]
    fn test_type_bound() {
        let bound = TypeBound::simple("Comparable");
        assert_eq!(format!("{}", bound), "Comparable");
        
        let generic_bound = TypeBound::generic("Comparable", vec![Type::type_param("T")]);
        assert_eq!(format!("{}", generic_bound), "Comparable<T>");
    }
    
    #[test]
    fn test_generic_param() {
        let param = GenericParam::new("T");
        assert_eq!(format!("{}", param), "T");
        
        let bounded_param = GenericParam::with_bounds(
            "T",
            vec![TypeBound::simple("Comparable"), TypeBound::simple("Printable")],
        );
        assert_eq!(format!("{}", bounded_param), "T: Comparable + Printable");
    }
    
    #[test]
    fn test_substitution() {
        let mut subst = Substitution::new();
        subst.insert("T".to_string(), Type::Int);
        
        let ty = Type::type_param("T");
        let substituted = ty.substitute(&subst);
        assert_eq!(substituted, Type::Int);
        
        // 嵌套替换
        let array_ty = Type::Slice {
            element_type: Box::new(Type::type_param("T")),
        };
        let substituted_array = array_ty.substitute(&subst);
        assert_eq!(
            substituted_array,
            Type::Slice { element_type: Box::new(Type::Int) }
        );
    }
    
    #[test]
    fn test_type_vars() {
        let var1 = TypeVar::fresh();
        let var2 = TypeVar::fresh();
        assert_ne!(var1.id, var2.id);
        
        let ty = Type::TypeVar(TypeVar::fresh());
        assert!(ty.has_type_vars());
        assert!(!Type::Int.has_type_vars());
    }
    
    #[test]
    fn test_generic_type() {
        let list_int = Type::generic(Type::Class("List".to_string()), vec![Type::Int]);
        assert_eq!(format!("{}", list_int), "List<int>");
        
        let map_type = Type::generic(
            Type::Class("Map".to_string()),
            vec![Type::String, Type::Int],
        );
        assert_eq!(format!("{}", map_type), "Map<string, int>");
    }
}
