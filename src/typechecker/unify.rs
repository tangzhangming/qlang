//! 类型统一算法
//! 
//! 实现 Robinson 统一算法的变体

use std::collections::HashMap;
use crate::types::{Type, TypeVar, Substitution};
use crate::lexer::Span;
use super::error::{TypeError, TypeErrorKind};

/// 统一结果
pub type UnifyResult = Result<Substitution, TypeError>;

/// 类型统一器
pub struct Unifier {
    /// 当前替换
    substitution: Substitution,
}

impl Unifier {
    /// 创建新的统一器
    pub fn new() -> Self {
        Self {
            substitution: Substitution::new(),
        }
    }
    
    /// 使用现有替换创建统一器
    pub fn with_substitution(substitution: Substitution) -> Self {
        Self { substitution }
    }
    
    /// 统一两个类型
    pub fn unify(&mut self, t1: &Type, t2: &Type, span: Span) -> UnifyResult {
        // 应用当前替换
        let t1 = self.apply(&t1);
        let t2 = self.apply(&t2);
        
        // 如果相等则成功
        if t1 == t2 {
            return Ok(self.substitution.clone());
        }
        
        self.unify_internal(&t1, &t2, span)
    }
    
    /// 内部统一实现
    fn unify_internal(&mut self, t1: &Type, t2: &Type, span: Span) -> UnifyResult {
        use Type::*;
        
        match (t1, t2) {
            // Error 类型总是成功（避免级联错误）
            (Error, _) | (_, Error) => Ok(self.substitution.clone()),
            
            // Never 类型可以统一到任何类型
            (Never, _) => Ok(self.substitution.clone()),
            
            // 类型变量
            (TypeVar(v), ty) | (ty, TypeVar(v)) => {
                self.unify_var(v, ty, span)
            }
            
            // 类型参数
            (TypeParameter { name: n1, bounds: b1 }, TypeParameter { name: n2, bounds: b2 }) => {
                if n1 == n2 {
                    Ok(self.substitution.clone())
                } else {
                    // 不同的类型参数，添加替换
                    self.substitution.insert(n1.clone(), t2.clone());
                    Ok(self.substitution.clone())
                }
            }
            (TypeParameter { name, .. }, ty) | (ty, TypeParameter { name, .. }) => {
                self.substitution.insert(name.clone(), ty.clone());
                Ok(self.substitution.clone())
            }
            
            // 函数类型
            (
                Function { param_types: p1, return_type: r1 },
                Function { param_types: p2, return_type: r2 },
            ) => {
                if p1.len() != p2.len() {
                    return Err(TypeError::new(
                        TypeErrorKind::ArgumentCountMismatch {
                            expected: p1.len(),
                            actual: p2.len(),
                        },
                        span,
                    ));
                }
                
                for (pt1, pt2) in p1.iter().zip(p2.iter()) {
                    self.unify(pt1, pt2, span)?;
                }
                
                self.unify(r1, r2, span)
            }
            
            // 数组类型
            (
                Array { element_type: e1, size: s1 },
                Array { element_type: e2, size: s2 },
            ) => {
                if s1 != s2 {
                    return Err(TypeError::type_mismatch(t1.clone(), t2.clone(), span));
                }
                self.unify(e1, e2, span)
            }
            
            // 切片类型
            (Slice { element_type: e1 }, Slice { element_type: e2 }) => {
                self.unify(e1, e2, span)
            }
            
            // Map 类型
            (
                Map { key_type: k1, value_type: v1 },
                Map { key_type: k2, value_type: v2 },
            ) => {
                self.unify(k1, k2, span)?;
                self.unify(v1, v2, span)
            }
            
            // 元组类型
            (Tuple(ts1), Tuple(ts2)) => {
                if ts1.len() != ts2.len() {
                    return Err(TypeError::type_mismatch(t1.clone(), t2.clone(), span));
                }
                for (tt1, tt2) in ts1.iter().zip(ts2.iter()) {
                    self.unify(tt1, tt2, span)?;
                }
                Ok(self.substitution.clone())
            }
            
            // 可空类型
            (Nullable(n1), Nullable(n2)) => {
                self.unify(n1, n2, span)
            }
            
            // 非空类型到可空类型的统一
            (ty, Nullable(inner)) if !ty.is_nullable() => {
                self.unify(ty, inner, span)
            }
            
            // 指针类型
            (Pointer(p1), Pointer(p2)) => {
                self.unify(p1, p2, span)
            }
            
            // 泛型类型
            (
                Generic { base_type: b1, type_args: a1 },
                Generic { base_type: b2, type_args: a2 },
            ) => {
                self.unify(b1, b2, span)?;
                
                if a1.len() != a2.len() {
                    return Err(TypeError::new(
                        TypeErrorKind::TypeArgumentCountMismatch {
                            expected: a1.len(),
                            actual: a2.len(),
                        },
                        span,
                    ));
                }
                
                for (arg1, arg2) in a1.iter().zip(a2.iter()) {
                    self.unify(arg1, arg2, span)?;
                }
                
                Ok(self.substitution.clone())
            }
            
            // 关联类型
            (
                AssociatedType { base_type: b1, name: n1 },
                AssociatedType { base_type: b2, name: n2 },
            ) => {
                if n1 != n2 {
                    return Err(TypeError::type_mismatch(t1.clone(), t2.clone(), span));
                }
                self.unify(b1, b2, span)
            }
            
            // 类型别名（展开后统一）
            (Alias { actual_type: a1, .. }, ty) | (ty, Alias { actual_type: a1, .. }) => {
                self.unify(a1, ty, span)
            }
            
            // Infer 类型（等待推导）
            (Infer, ty) | (ty, Infer) => {
                // 创建新的类型变量
                let var = crate::types::TypeVar::fresh();
                self.unify_var(&var, ty, span)
            }
            
            // 相同的具体类型
            (Class(n1), Class(n2)) if n1 == n2 => Ok(self.substitution.clone()),
            (Struct(n1), Struct(n2)) if n1 == n2 => Ok(self.substitution.clone()),
            (Interface(n1), Interface(n2)) if n1 == n2 => Ok(self.substitution.clone()),
            (Trait(n1), Trait(n2)) if n1 == n2 => Ok(self.substitution.clone()),
            (Enum(n1), Enum(n2)) if n1 == n2 => Ok(self.substitution.clone()),
            
            // Dynamic 类型可以统一到任何类型
            (Dynamic, _) | (_, Dynamic) => Ok(self.substitution.clone()),
            
            // Unknown 类型可以接受任何类型
            (Unknown, _) => Ok(self.substitution.clone()),
            (_, Unknown) => Ok(self.substitution.clone()),
            
            // 其他情况：类型不匹配
            _ => Err(TypeError::type_mismatch(t1.clone(), t2.clone(), span)),
        }
    }
    
    /// 统一类型变量
    fn unify_var(&mut self, var: &TypeVar, ty: &Type, span: Span) -> UnifyResult {
        let var_name = format!("?T{}", var.id);
        
        // 检查是否已有替换
        if let Some(existing) = self.substitution.get(&var_name).cloned() {
            return self.unify(&existing, ty, span);
        }
        
        // 如果 ty 也是同一个类型变量，则成功
        if let Type::TypeVar(v2) = ty {
            if var.id == v2.id {
                return Ok(self.substitution.clone());
            }
        }
        
        // 发生检查
        if self.occurs_in(var, ty) {
            return Err(TypeError::new(TypeErrorKind::InfiniteType, span));
        }
        
        // 添加替换
        self.substitution.insert(var_name, ty.clone());
        Ok(self.substitution.clone())
    }
    
    /// 发生检查：检查类型变量是否出现在类型中
    fn occurs_in(&self, var: &TypeVar, ty: &Type) -> bool {
        let ty = self.apply(ty);
        
        match ty {
            Type::TypeVar(v) => v.id == var.id,
            Type::Function { param_types, return_type } => {
                param_types.iter().any(|t| self.occurs_in(var, t)) ||
                self.occurs_in(var, &return_type)
            }
            Type::Array { element_type, .. } | Type::Slice { element_type } |
            Type::Nullable(element_type) | Type::Pointer(element_type) => {
                self.occurs_in(var, &element_type)
            }
            Type::Map { key_type, value_type } => {
                self.occurs_in(var, &key_type) || self.occurs_in(var, &value_type)
            }
            Type::Tuple(types) => types.iter().any(|t| self.occurs_in(var, t)),
            Type::Generic { base_type, type_args } => {
                self.occurs_in(var, &base_type) ||
                type_args.iter().any(|t| self.occurs_in(var, t))
            }
            Type::AssociatedType { base_type, .. } => self.occurs_in(var, &base_type),
            Type::Alias { actual_type, .. } => self.occurs_in(var, &actual_type),
            _ => false,
        }
    }
    
    /// 应用当前替换到类型
    pub fn apply(&self, ty: &Type) -> Type {
        ty.substitute(&self.substitution)
    }
    
    /// 获取当前替换
    pub fn get_substitution(&self) -> &Substitution {
        &self.substitution
    }
    
    /// 获取并消耗替换
    pub fn into_substitution(self) -> Substitution {
        self.substitution
    }
}

impl Default for Unifier {
    fn default() -> Self {
        Self::new()
    }
}

/// 便捷函数：统一两个类型
pub fn unify(t1: &Type, t2: &Type, span: Span) -> UnifyResult {
    let mut unifier = Unifier::new();
    unifier.unify(t1, t2, span)
}

/// 便捷函数：使用现有替换统一两个类型
pub fn unify_with_subst(
    t1: &Type,
    t2: &Type,
    span: Span,
    substitution: Substitution,
) -> UnifyResult {
    let mut unifier = Unifier::with_substitution(substitution);
    unifier.unify(t1, t2, span)
}
