//! 约束系统
//! 
//! 用于类型推导的约束生成和求解

use std::collections::HashMap;
use crate::types::{Type, TypeBound, TypeVar, Substitution};
use crate::lexer::Span;
use super::error::{TypeError, TypeErrorKind};

/// 约束种类
#[derive(Debug, Clone, PartialEq)]
pub enum ConstraintKind {
    /// 类型相等约束 T1 = T2
    Equal(Type, Type),
    /// 子类型约束 T1 <: T2
    Subtype {
        sub: Type,
        super_: Type,
    },
    /// Trait 约束 T: Trait
    TraitBound {
        ty: Type,
        bound: TypeBound,
    },
    /// 实例化约束 T<A, B> 实例化自 T<T1, T2>
    Instantiate {
        generic: Type,
        args: Vec<Type>,
    },
}

/// 约束
#[derive(Debug, Clone)]
pub struct Constraint {
    /// 约束种类
    pub kind: ConstraintKind,
    /// 约束来源位置
    pub span: Span,
    /// 约束原因描述
    pub reason: String,
}

impl Constraint {
    /// 创建新的约束
    pub fn new(kind: ConstraintKind, span: Span, reason: impl Into<String>) -> Self {
        Self {
            kind,
            span,
            reason: reason.into(),
        }
    }
    
    /// 创建相等约束
    pub fn equal(t1: Type, t2: Type, span: Span, reason: impl Into<String>) -> Self {
        Self::new(ConstraintKind::Equal(t1, t2), span, reason)
    }
    
    /// 创建子类型约束
    pub fn subtype(sub: Type, super_: Type, span: Span, reason: impl Into<String>) -> Self {
        Self::new(
            ConstraintKind::Subtype { sub, super_ },
            span,
            reason,
        )
    }
    
    /// 创建 Trait 约束
    pub fn trait_bound(ty: Type, bound: TypeBound, span: Span, reason: impl Into<String>) -> Self {
        Self::new(
            ConstraintKind::TraitBound { ty, bound },
            span,
            reason,
        )
    }
}

/// 约束求解器
pub struct ConstraintSolver {
    /// 待解约束
    constraints: Vec<Constraint>,
    /// 当前替换
    substitution: Substitution,
    /// 类型变量到约束的映射
    var_constraints: HashMap<u64, Vec<TypeBound>>,
    /// 错误列表
    errors: Vec<TypeError>,
}

impl ConstraintSolver {
    /// 创建新的约束求解器
    pub fn new() -> Self {
        Self {
            constraints: Vec::new(),
            substitution: Substitution::new(),
            var_constraints: HashMap::new(),
            errors: Vec::new(),
        }
    }
    
    /// 添加约束
    pub fn add_constraint(&mut self, constraint: Constraint) {
        self.constraints.push(constraint);
    }
    
    /// 添加多个约束
    pub fn add_constraints(&mut self, constraints: Vec<Constraint>) {
        self.constraints.extend(constraints);
    }
    
    /// 求解所有约束
    pub fn solve(&mut self) -> Result<Substitution, Vec<TypeError>> {
        // 循环直到没有新的替换产生
        let mut changed = true;
        let mut iterations = 0;
        const MAX_ITERATIONS: usize = 1000;
        
        while changed && iterations < MAX_ITERATIONS {
            changed = false;
            iterations += 1;
            
            let constraints = std::mem::take(&mut self.constraints);
            
            for constraint in constraints {
                match self.solve_constraint(&constraint) {
                    Ok(new_substitution) => {
                        if !new_substitution.is_empty() {
                            changed = true;
                            // 合并替换
                            for (name, ty) in new_substitution {
                                self.substitution.insert(name, ty);
                            }
                        }
                    }
                    Err(err) => {
                        self.errors.push(err);
                    }
                }
            }
            
            // 应用当前替换到所有剩余约束
            self.apply_substitution_to_constraints();
        }
        
        if iterations >= MAX_ITERATIONS {
            self.errors.push(TypeError::new(
                TypeErrorKind::InfiniteType,
                Span::default(),
            ));
        }
        
        if self.errors.is_empty() {
            Ok(std::mem::take(&mut self.substitution))
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }
    
    /// 求解单个约束
    fn solve_constraint(&mut self, constraint: &Constraint) -> Result<Substitution, TypeError> {
        match &constraint.kind {
            ConstraintKind::Equal(t1, t2) => {
                self.unify(t1, t2, constraint.span)
            }
            ConstraintKind::Subtype { sub, super_ } => {
                // 子类型检查（简化：目前只检查相等或可空）
                if sub.is_assignable_to(super_) {
                    Ok(Substitution::new())
                } else {
                    // 尝试统一
                    self.unify(sub, super_, constraint.span)
                }
            }
            ConstraintKind::TraitBound { ty, bound } => {
                // Trait 约束检查（需要类型环境支持）
                // 暂时标记为待检查
                if let Type::TypeVar(var) = ty {
                    self.var_constraints
                        .entry(var.id)
                        .or_insert_with(Vec::new)
                        .push(bound.clone());
                }
                Ok(Substitution::new())
            }
            ConstraintKind::Instantiate { generic, args } => {
                // 泛型实例化约束
                // TODO: 实现实例化检查
                Ok(Substitution::new())
            }
        }
    }
    
    /// 统一两个类型
    fn unify(&self, t1: &Type, t2: &Type, span: Span) -> Result<Substitution, TypeError> {
        use Type::*;
        
        let mut subst = Substitution::new();
        
        // 应用当前替换
        let t1 = t1.substitute(&self.substitution);
        let t2 = t2.substitute(&self.substitution);
        
        // 如果相等则成功
        if t1 == t2 {
            return Ok(subst);
        }
        
        match (&t1, &t2) {
            // 类型变量统一
            (TypeVar(v), _) => {
                if self.occurs_check(v, &t2) {
                    return Err(TypeError::new(TypeErrorKind::InfiniteType, span));
                }
                subst.insert(format!("?T{}", v.id), t2.clone());
                Ok(subst)
            }
            (_, TypeVar(v)) => {
                if self.occurs_check(v, &t1) {
                    return Err(TypeError::new(TypeErrorKind::InfiniteType, span));
                }
                subst.insert(format!("?T{}", v.id), t1.clone());
                Ok(subst)
            }
            
            // 类型参数统一
            (TypeParameter { name: n1, .. }, TypeParameter { name: n2, .. }) if n1 == n2 => {
                Ok(subst)
            }
            (TypeParameter { name, .. }, _) => {
                subst.insert(name.clone(), t2.clone());
                Ok(subst)
            }
            (_, TypeParameter { name, .. }) => {
                subst.insert(name.clone(), t1.clone());
                Ok(subst)
            }
            
            // 函数类型统一
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
                
                // 统一参数类型
                for (pt1, pt2) in p1.iter().zip(p2.iter()) {
                    let param_subst = self.unify(pt1, pt2, span)?;
                    for (k, v) in param_subst {
                        subst.insert(k, v);
                    }
                }
                
                // 统一返回类型
                let ret_subst = self.unify(r1, r2, span)?;
                for (k, v) in ret_subst {
                    subst.insert(k, v);
                }
                
                Ok(subst)
            }
            
            // 数组类型统一
            (Array { element_type: e1, size: s1 }, Array { element_type: e2, size: s2 }) => {
                if s1 != s2 {
                    return Err(TypeError::type_mismatch(t1.clone(), t2.clone(), span));
                }
                self.unify(e1, e2, span)
            }
            
            // 切片类型统一
            (Slice { element_type: e1 }, Slice { element_type: e2 }) => {
                self.unify(e1, e2, span)
            }
            
            // Map 类型统一
            (
                Map { key_type: k1, value_type: v1 },
                Map { key_type: k2, value_type: v2 },
            ) => {
                let key_subst = self.unify(k1, k2, span)?;
                for (k, v) in key_subst {
                    subst.insert(k, v);
                }
                
                let val_subst = self.unify(v1, v2, span)?;
                for (k, v) in val_subst {
                    subst.insert(k, v);
                }
                
                Ok(subst)
            }
            
            // 可空类型统一
            (Nullable(n1), Nullable(n2)) => {
                self.unify(n1, n2, span)
            }
            
            // 泛型类型统一
            (
                Generic { base_type: b1, type_args: a1 },
                Generic { base_type: b2, type_args: a2 },
            ) => {
                // 统一基类型
                let base_subst = self.unify(b1, b2, span)?;
                for (k, v) in base_subst {
                    subst.insert(k, v);
                }
                
                // 统一类型参数
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
                    let arg_subst = self.unify(arg1, arg2, span)?;
                    for (k, v) in arg_subst {
                        subst.insert(k, v);
                    }
                }
                
                Ok(subst)
            }
            
            // Error 类型总是统一成功（避免级联错误）
            (Error, _) | (_, Error) => Ok(subst),
            
            // Infer 类型（等待推导）
            (Infer, _) | (_, Infer) => Ok(subst),
            
            // 其他情况：类型不匹配
            _ => Err(TypeError::type_mismatch(t1.clone(), t2.clone(), span)),
        }
    }
    
    /// 发生检查：检查类型变量是否出现在类型中
    fn occurs_check(&self, var: &TypeVar, ty: &Type) -> bool {
        match ty {
            Type::TypeVar(v) => v.id == var.id,
            Type::Function { param_types, return_type } => {
                param_types.iter().any(|t| self.occurs_check(var, t)) ||
                self.occurs_check(var, return_type)
            }
            Type::Array { element_type, .. } | Type::Slice { element_type } |
            Type::Nullable(element_type) | Type::Pointer(element_type) => {
                self.occurs_check(var, element_type)
            }
            Type::Map { key_type, value_type } => {
                self.occurs_check(var, key_type) || self.occurs_check(var, value_type)
            }
            Type::Tuple(types) => types.iter().any(|t| self.occurs_check(var, t)),
            Type::Generic { base_type, type_args } => {
                self.occurs_check(var, base_type) ||
                type_args.iter().any(|t| self.occurs_check(var, t))
            }
            _ => false,
        }
    }
    
    /// 将当前替换应用到所有约束
    fn apply_substitution_to_constraints(&mut self) {
        for constraint in &mut self.constraints {
            match &mut constraint.kind {
                ConstraintKind::Equal(t1, t2) => {
                    *t1 = t1.substitute(&self.substitution);
                    *t2 = t2.substitute(&self.substitution);
                }
                ConstraintKind::Subtype { sub, super_ } => {
                    *sub = sub.substitute(&self.substitution);
                    *super_ = super_.substitute(&self.substitution);
                }
                ConstraintKind::TraitBound { ty, bound } => {
                    *ty = ty.substitute(&self.substitution);
                    for arg in &mut bound.type_args {
                        *arg = arg.substitute(&self.substitution);
                    }
                }
                ConstraintKind::Instantiate { generic, args } => {
                    *generic = generic.substitute(&self.substitution);
                    for arg in args {
                        *arg = arg.substitute(&self.substitution);
                    }
                }
            }
        }
    }
    
    /// 获取当前替换
    pub fn get_substitution(&self) -> &Substitution {
        &self.substitution
    }
    
    /// 获取类型变量的约束
    pub fn get_var_constraints(&self, var_id: u64) -> Option<&Vec<TypeBound>> {
        self.var_constraints.get(&var_id)
    }
}

impl Default for ConstraintSolver {
    fn default() -> Self {
        Self::new()
    }
}
