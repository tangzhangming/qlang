//! 类型错误定义
//! 
//! 定义类型检查过程中可能产生的所有错误

use crate::lexer::Span;
use crate::types::Type;
use std::fmt;

/// 类型错误种类
#[derive(Debug, Clone, PartialEq)]
pub enum TypeErrorKind {
    /// 类型不匹配
    TypeMismatch {
        expected: Type,
        actual: Type,
    },
    /// 未知类型
    UnknownType(String),
    /// 未定义的变量
    UndefinedVariable(String),
    /// 未定义的函数
    UndefinedFunction(String),
    /// 未定义的类型
    UndefinedType(String),
    /// 未定义的字段
    UndefinedField {
        type_name: String,
        field_name: String,
    },
    /// 未定义的方法
    UndefinedMethod {
        type_name: String,
        method_name: String,
    },
    /// 重复定义
    DuplicateDefinition(String),
    /// 参数数量不匹配
    ArgumentCountMismatch {
        expected: usize,
        actual: usize,
    },
    /// 类型参数数量不匹配
    TypeArgumentCountMismatch {
        expected: usize,
        actual: usize,
    },
    /// 不可调用的类型
    NotCallable(Type),
    /// 不可索引的类型
    NotIndexable(Type),
    /// 不可迭代的类型
    NotIterable(Type),
    /// 类型约束不满足
    ConstraintNotSatisfied {
        type_param: String,
        constraint: String,
        actual_type: Type,
    },
    /// 无法推导类型
    CannotInferType,
    /// 循环类型依赖
    CyclicTypeDependency(String),
    /// 不可空类型赋值 null
    NullNotAllowed(Type),
    /// 无效的类型转换
    InvalidCast {
        from: Type,
        to: Type,
    },
    /// 常量重新赋值
    ConstantReassignment(String),
    /// 缺少返回值
    MissingReturn(Type),
    /// 不可达代码
    UnreachableCode,
    /// 抽象类不能实例化
    CannotInstantiateAbstract(String),
    /// 缺少接口方法实现
    MissingInterfaceMethod {
        interface_name: String,
        method_name: String,
    },
    /// 缺少 Trait 方法实现
    MissingTraitMethod {
        trait_name: String,
        method_name: String,
    },
    /// 类型不兼容
    IncompatibleTypes {
        types: Vec<Type>,
        context: String,
    },
    /// 递归类型无限展开
    InfiniteType,
    /// 其他错误
    Other(String),
}

/// 类型错误
#[derive(Debug, Clone)]
pub struct TypeError {
    /// 错误种类
    pub kind: TypeErrorKind,
    /// 错误位置
    pub span: Span,
    /// 附加信息
    pub notes: Vec<String>,
}

impl TypeError {
    /// 创建新的类型错误
    pub fn new(kind: TypeErrorKind, span: Span) -> Self {
        Self {
            kind,
            span,
            notes: Vec::new(),
        }
    }
    
    /// 添加注释
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
    
    /// 创建类型不匹配错误
    pub fn type_mismatch(expected: Type, actual: Type, span: Span) -> Self {
        Self::new(
            TypeErrorKind::TypeMismatch { expected, actual },
            span,
        )
    }
    
    /// 创建未定义变量错误
    pub fn undefined_variable(name: impl Into<String>, span: Span) -> Self {
        Self::new(TypeErrorKind::UndefinedVariable(name.into()), span)
    }
    
    /// 创建未定义函数错误
    pub fn undefined_function(name: impl Into<String>, span: Span) -> Self {
        Self::new(TypeErrorKind::UndefinedFunction(name.into()), span)
    }
    
    /// 创建未定义类型错误
    pub fn undefined_type(name: impl Into<String>, span: Span) -> Self {
        Self::new(TypeErrorKind::UndefinedType(name.into()), span)
    }
    
    /// 创建参数数量不匹配错误
    pub fn argument_count_mismatch(expected: usize, actual: usize, span: Span) -> Self {
        Self::new(
            TypeErrorKind::ArgumentCountMismatch { expected, actual },
            span,
        )
    }
    
    /// 创建不可调用错误
    pub fn not_callable(ty: Type, span: Span) -> Self {
        Self::new(TypeErrorKind::NotCallable(ty), span)
    }
    
    /// 创建约束不满足错误
    pub fn constraint_not_satisfied(
        type_param: impl Into<String>,
        constraint: impl Into<String>,
        actual_type: Type,
        span: Span,
    ) -> Self {
        Self::new(
            TypeErrorKind::ConstraintNotSatisfied {
                type_param: type_param.into(),
                constraint: constraint.into(),
                actual_type,
            },
            span,
        )
    }
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            TypeErrorKind::TypeMismatch { expected, actual } => {
                write!(f, "类型不匹配: 期望 {}, 实际 {}", expected, actual)
            }
            TypeErrorKind::UnknownType(name) => {
                write!(f, "未知类型: {}", name)
            }
            TypeErrorKind::UndefinedVariable(name) => {
                write!(f, "未定义的变量: {}", name)
            }
            TypeErrorKind::UndefinedFunction(name) => {
                write!(f, "未定义的函数: {}", name)
            }
            TypeErrorKind::UndefinedType(name) => {
                write!(f, "未定义的类型: {}", name)
            }
            TypeErrorKind::UndefinedField { type_name, field_name } => {
                write!(f, "类型 {} 没有字段 {}", type_name, field_name)
            }
            TypeErrorKind::UndefinedMethod { type_name, method_name } => {
                write!(f, "类型 {} 没有方法 {}", type_name, method_name)
            }
            TypeErrorKind::DuplicateDefinition(name) => {
                write!(f, "重复定义: {}", name)
            }
            TypeErrorKind::ArgumentCountMismatch { expected, actual } => {
                write!(f, "参数数量不匹配: 期望 {}, 实际 {}", expected, actual)
            }
            TypeErrorKind::TypeArgumentCountMismatch { expected, actual } => {
                write!(f, "类型参数数量不匹配: 期望 {}, 实际 {}", expected, actual)
            }
            TypeErrorKind::NotCallable(ty) => {
                write!(f, "类型 {} 不可调用", ty)
            }
            TypeErrorKind::NotIndexable(ty) => {
                write!(f, "类型 {} 不可索引", ty)
            }
            TypeErrorKind::NotIterable(ty) => {
                write!(f, "类型 {} 不可迭代", ty)
            }
            TypeErrorKind::ConstraintNotSatisfied { type_param, constraint, actual_type } => {
                write!(f, "类型 {} 不满足约束 {}: {}", actual_type, type_param, constraint)
            }
            TypeErrorKind::CannotInferType => {
                write!(f, "无法推导类型")
            }
            TypeErrorKind::CyclicTypeDependency(name) => {
                write!(f, "循环类型依赖: {}", name)
            }
            TypeErrorKind::NullNotAllowed(ty) => {
                write!(f, "不能将 null 赋值给非空类型 {}", ty)
            }
            TypeErrorKind::InvalidCast { from, to } => {
                write!(f, "无效的类型转换: {} 到 {}", from, to)
            }
            TypeErrorKind::ConstantReassignment(name) => {
                write!(f, "不能重新赋值常量: {}", name)
            }
            TypeErrorKind::MissingReturn(ty) => {
                write!(f, "缺少返回值: 期望 {}", ty)
            }
            TypeErrorKind::UnreachableCode => {
                write!(f, "不可达代码")
            }
            TypeErrorKind::CannotInstantiateAbstract(name) => {
                write!(f, "不能实例化抽象类: {}", name)
            }
            TypeErrorKind::MissingInterfaceMethod { interface_name, method_name } => {
                write!(f, "缺少接口 {} 的方法实现: {}", interface_name, method_name)
            }
            TypeErrorKind::MissingTraitMethod { trait_name, method_name } => {
                write!(f, "缺少 Trait {} 的方法实现: {}", trait_name, method_name)
            }
            TypeErrorKind::IncompatibleTypes { types, context } => {
                let type_strs: Vec<_> = types.iter().map(|t| t.to_string()).collect();
                write!(f, "类型不兼容 ({}): {}", context, type_strs.join(", "))
            }
            TypeErrorKind::InfiniteType => {
                write!(f, "无限类型")
            }
            TypeErrorKind::Other(msg) => {
                write!(f, "{}", msg)
            }
        }
    }
}

impl std::error::Error for TypeError {}
