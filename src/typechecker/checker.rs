//! 类型检查器
//! 
//! 对 AST 进行类型检查和推导

use std::collections::HashMap;
use crate::parser::{Expr, Stmt, Program, BinOp, UnaryOp, AssignOp, MatchPattern};
use crate::parser::ast::{TypeParam, WhereClause, FnParam, TypeAnnotation};
use crate::types::{Type, TypeBound, GenericParam, Substitution};
use crate::lexer::Span;
use super::environment::{TypeEnvironment, TypeInfo, FunctionInfo, ClassInfo, StructInfo, FieldInfo, Visibility};
use super::constraint::{Constraint, ConstraintSolver};
use super::unify::Unifier;
use super::error::{TypeError, TypeErrorKind};

/// 类型检查器
pub struct TypeChecker {
    /// 类型环境
    env: TypeEnvironment,
    /// 约束求解器
    solver: ConstraintSolver,
    /// 错误列表
    errors: Vec<TypeError>,
    /// 是否在函数内部
    in_function: bool,
    /// 是否在循环内部
    in_loop: bool,
}

impl TypeChecker {
    /// 创建新的类型检查器
    pub fn new() -> Self {
        Self {
            env: TypeEnvironment::new(),
            solver: ConstraintSolver::new(),
            errors: Vec::new(),
            in_function: false,
            in_loop: false,
        }
    }
    
    /// 检查是否是内置函数
    fn is_builtin_function(name: &str) -> bool {
        matches!(name, "print" | "println" | "typeof" | "typeinfo" | "sizeof" | "panic" | "time")
    }
    
    /// 获取内置函数的类型
    fn builtin_function_type(name: &str) -> Type {
        match name {
            "print" | "println" => Type::Function {
                param_types: vec![Type::Dynamic],
                return_type: Box::new(Type::Void),
            },
            "typeof" => Type::Function {
                param_types: vec![Type::Dynamic],
                return_type: Box::new(Type::String),
            },
            "typeinfo" => Type::Function {
                param_types: vec![Type::Dynamic],
                return_type: Box::new(Type::Dynamic), // 返回 RuntimeTypeInfo 对象
            },
            "sizeof" => Type::Function {
                param_types: vec![Type::Dynamic],
                return_type: Box::new(Type::Int),
            },
            "panic" => Type::Function {
                param_types: vec![Type::String],
                return_type: Box::new(Type::Never),
            },
            "time" => Type::Function {
                param_types: vec![],
                return_type: Box::new(Type::Int),
            },
            _ => Type::Unknown,
        }
    }
    
    /// 检查整个程序
    pub fn check_program(&mut self, program: &Program) -> Result<(), Vec<TypeError>> {
        // 第一遍：收集所有类型定义
        for stmt in &program.statements {
            self.collect_type_definitions(stmt);
        }
        
        // 第二遍：检查类型实现
        for stmt in &program.statements {
            self.check_type_implementations(stmt);
        }
        
        // 第三遍：检查所有语句
        for stmt in &program.statements {
            if let Err(e) = self.check_stmt(stmt) {
                self.errors.push(e);
            }
        }
        
        // 求解约束
        if let Err(mut errs) = self.solver.solve() {
            self.errors.append(&mut errs);
        }
        
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }
    
    /// 收集类型定义（第一遍）
    fn collect_type_definitions(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::StructDef { name, type_params, interfaces, fields, methods, .. } => {
                let info = StructInfo {
                    name: name.clone(),
                    type_params: self.convert_type_params(type_params),
                    interfaces: interfaces.clone(),
                    fields: self.collect_struct_fields(fields),
                    methods: self.collect_struct_methods(methods),
                };
                if let Err(e) = self.env.register_type(name.clone(), TypeInfo::Struct(info)) {
                    self.errors.push(TypeError::new(
                        TypeErrorKind::DuplicateDefinition(name.clone()),
                        Span::default(),
                    ));
                }
            }
            Stmt::ClassDef { name, type_params, is_abstract, parent, interfaces, traits, fields, methods, .. } => {
                let info = ClassInfo {
                    name: name.clone(),
                    type_params: self.convert_type_params(type_params),
                    parent: parent.clone(),
                    interfaces: interfaces.clone(),
                    traits: traits.clone(),
                    fields: self.collect_class_fields(fields),
                    methods: self.collect_class_methods(methods),
                    static_fields: self.collect_class_static_fields(fields),
                    static_methods: self.collect_class_static_methods(methods),
                    is_abstract: *is_abstract,
                };
                if let Err(e) = self.env.register_type(name.clone(), TypeInfo::Class(info)) {
                    self.errors.push(TypeError::new(
                        TypeErrorKind::DuplicateDefinition(name.clone()),
                        Span::default(),
                    ));
                }
            }
            Stmt::InterfaceDef { name, type_params, super_interfaces, methods, .. } => {
                let info = super::environment::InterfaceInfo {
                    name: name.clone(),
                    type_params: self.convert_type_params(type_params),
                    super_interfaces: super_interfaces.clone(),
                    methods: self.collect_interface_methods(methods),
                };
                if let Err(e) = self.env.register_type(name.clone(), TypeInfo::Interface(info)) {
                    self.errors.push(TypeError::new(
                        TypeErrorKind::DuplicateDefinition(name.clone()),
                        Span::default(),
                    ));
                }
            }
            Stmt::TraitDef { name, type_params, super_traits, methods, .. } => {
                let info = super::environment::TraitInfo {
                    name: name.clone(),
                    type_params: self.convert_type_params(type_params),
                    super_traits: super_traits.clone(),
                    methods: self.collect_trait_methods(methods),
                    default_methods: methods.iter()
                        .map(|m| (m.name.clone(), m.default_body.is_some()))
                        .collect(),
                };
                if let Err(e) = self.env.register_type(name.clone(), TypeInfo::Trait(info)) {
                    self.errors.push(TypeError::new(
                        TypeErrorKind::DuplicateDefinition(name.clone()),
                        Span::default(),
                    ));
                }
            }
            Stmt::EnumDef { name, variants, .. } => {
                let info = super::environment::EnumInfo {
                    name: name.clone(),
                    variants: variants.iter().map(|v| {
                        (v.name.clone(), super::environment::EnumVariantInfo {
                            name: v.name.clone(),
                            value_type: v.value.as_ref().map(|_| Type::Int), // 简化处理
                            fields: v.fields.iter()
                                .map(|(n, t)| (n.clone(), t.ty.clone()))
                                .collect(),
                        })
                    }).collect(),
                    methods: HashMap::new(),
                };
                if let Err(e) = self.env.register_type(name.clone(), TypeInfo::Enum(info)) {
                    self.errors.push(TypeError::new(
                        TypeErrorKind::DuplicateDefinition(name.clone()),
                        Span::default(),
                    ));
                }
            }
            Stmt::TypeAlias { name, target_type, .. } => {
                if let Err(e) = self.env.register_type(
                    name.clone(),
                    TypeInfo::Alias {
                        name: name.clone(),
                        actual_type: target_type.ty.clone(),
                    },
                ) {
                    self.errors.push(TypeError::new(
                        TypeErrorKind::DuplicateDefinition(name.clone()),
                        Span::default(),
                    ));
                }
            }
            Stmt::FnDef { name, type_params, params, return_type, .. } => {
                let info = FunctionInfo {
                    name: name.clone(),
                    type_params: self.convert_type_params(type_params),
                    param_types: params.iter().map(|p| p.type_ann.ty.clone()).collect(),
                    param_names: params.iter().map(|p| p.name.clone()).collect(),
                    return_type: return_type.as_ref().map(|t| t.ty.clone()).unwrap_or(Type::Void),
                    is_method: false,
                    owner_type: None,
                };
                if let Err(e) = self.env.register_function(name.clone(), info) {
                    self.errors.push(TypeError::new(
                        TypeErrorKind::DuplicateDefinition(name.clone()),
                        Span::default(),
                    ));
                }
            }
            _ => {}
        }
    }
    
    /// 检查类型实现（第二遍）
    fn check_type_implementations(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::ClassDef { name, interfaces, traits, methods, span, .. } => {
                // 检查接口实现
                for interface_name in interfaces {
                    if let Some(TypeInfo::Interface(interface_info)) = self.env.lookup_type(interface_name) {
                        let defined_methods: std::collections::HashSet<_> = methods.iter()
                            .map(|m| m.name.clone())
                            .collect();
                        
                        for (method_name, _) in &interface_info.methods {
                            if !defined_methods.contains(method_name) {
                                self.errors.push(TypeError::new(
                                    TypeErrorKind::MissingInterfaceMethod {
                                        interface_name: interface_name.clone(),
                                        method_name: method_name.clone(),
                                    },
                                    *span,
                                ));
                            }
                        }
                    }
                }
                
                // 检查 Trait 实现
                for trait_name in traits {
                    if let Some(TypeInfo::Trait(trait_info)) = self.env.lookup_type(trait_name) {
                        let defined_methods: std::collections::HashSet<_> = methods.iter()
                            .map(|m| m.name.clone())
                            .collect();
                        
                        for (method_name, has_default) in &trait_info.default_methods {
                            if !has_default && !defined_methods.contains(method_name) {
                                self.errors.push(TypeError::new(
                                    TypeErrorKind::MissingTraitMethod {
                                        trait_name: trait_name.clone(),
                                        method_name: method_name.clone(),
                                    },
                                    *span,
                                ));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    
    /// 检查语句
    fn check_stmt(&mut self, stmt: &Stmt) -> Result<(), TypeError> {
        match stmt {
            Stmt::Expression { expr, span } => {
                self.infer_expr(expr)?;
                Ok(())
            }
            Stmt::Print { expr, span, .. } => {
                self.infer_expr(expr)?;
                Ok(())
            }
            Stmt::VarDecl { name, type_ann, initializer, span } => {
                let ty = if let Some(init) = initializer {
                    let init_ty = self.infer_expr(init)?;
                    
                    if let Some(ann) = type_ann {
                        // 检查初始化类型与声明类型是否兼容
                        if !init_ty.is_assignable_to(&ann.ty) {
                            return Err(TypeError::type_mismatch(ann.ty.clone(), init_ty, *span));
                        }
                        ann.ty.clone()
                    } else {
                        init_ty
                    }
                } else if let Some(ann) = type_ann {
                    ann.ty.clone()
                } else {
                    return Err(TypeError::new(TypeErrorKind::CannotInferType, *span));
                };
                
                self.env.define_variable(name.clone(), ty, false)
                    .map_err(|e| TypeError::new(TypeErrorKind::DuplicateDefinition(name.clone()), *span))?;
                
                Ok(())
            }
            Stmt::ConstDecl { name, type_ann, initializer, span } => {
                let init_ty = self.infer_expr(initializer)?;
                
                let ty = if let Some(ann) = type_ann {
                    if !init_ty.is_assignable_to(&ann.ty) {
                        return Err(TypeError::type_mismatch(ann.ty.clone(), init_ty, *span));
                    }
                    ann.ty.clone()
                } else {
                    init_ty
                };
                
                self.env.define_variable(name.clone(), ty, true)
                    .map_err(|e| TypeError::new(TypeErrorKind::DuplicateDefinition(name.clone()), *span))?;
                
                Ok(())
            }
            Stmt::Block { statements, .. } => {
                self.env.enter_scope();
                for stmt in statements {
                    self.check_stmt(stmt)?;
                }
                self.env.leave_scope();
                Ok(())
            }
            Stmt::If { condition, then_branch, else_branch, span } => {
                let cond_ty = self.infer_expr(condition)?;
                if cond_ty != Type::Bool {
                    return Err(TypeError::type_mismatch(Type::Bool, cond_ty, *span));
                }
                
                self.check_stmt(then_branch)?;
                if let Some(else_stmt) = else_branch {
                    self.check_stmt(else_stmt)?;
                }
                Ok(())
            }
            Stmt::ForLoop { initializer, condition, increment, body, .. } => {
                self.env.enter_scope();
                let was_in_loop = self.in_loop;
                self.in_loop = true;
                
                if let Some(init) = initializer {
                    self.check_stmt(init)?;
                }
                if let Some(cond) = condition {
                    let cond_ty = self.infer_expr(cond)?;
                    if cond_ty != Type::Bool {
                        return Err(TypeError::type_mismatch(Type::Bool, cond_ty, cond.span()));
                    }
                }
                if let Some(incr) = increment {
                    self.infer_expr(incr)?;
                }
                self.check_stmt(body)?;
                
                self.in_loop = was_in_loop;
                self.env.leave_scope();
                Ok(())
            }
            Stmt::ForIn { variables, iterable, body, span, .. } => {
                self.env.enter_scope();
                let was_in_loop = self.in_loop;
                self.in_loop = true;
                
                let iter_ty = self.infer_expr(iterable)?;
                let elem_ty = self.get_iterator_element_type(&iter_ty, *span)?;
                
                // 定义循环变量
                if variables.len() == 1 {
                    self.env.define_variable(variables[0].clone(), elem_ty, false)
                        .map_err(|_| TypeError::new(
                            TypeErrorKind::DuplicateDefinition(variables[0].clone()),
                            *span,
                        ))?;
                } else if variables.len() == 2 {
                    // index, value 形式
                    self.env.define_variable(variables[0].clone(), Type::Int, false)
                        .map_err(|_| TypeError::new(
                            TypeErrorKind::DuplicateDefinition(variables[0].clone()),
                            *span,
                        ))?;
                    self.env.define_variable(variables[1].clone(), elem_ty, false)
                        .map_err(|_| TypeError::new(
                            TypeErrorKind::DuplicateDefinition(variables[1].clone()),
                            *span,
                        ))?;
                }
                
                self.check_stmt(body)?;
                
                self.in_loop = was_in_loop;
                self.env.leave_scope();
                Ok(())
            }
            Stmt::While { condition, body, .. } => {
                let was_in_loop = self.in_loop;
                self.in_loop = true;
                
                if let Some(cond) = condition {
                    let cond_ty = self.infer_expr(cond)?;
                    if cond_ty != Type::Bool {
                        return Err(TypeError::type_mismatch(Type::Bool, cond_ty, cond.span()));
                    }
                }
                self.check_stmt(body)?;
                
                self.in_loop = was_in_loop;
                Ok(())
            }
            Stmt::Break { span, .. } | Stmt::Continue { span, .. } => {
                if !self.in_loop {
                    return Err(TypeError::new(
                        TypeErrorKind::Other("break/continue must be inside a loop".to_string()),
                        *span,
                    ));
                }
                Ok(())
            }
            Stmt::Return { value, span } => {
                let return_ty = if let Some(val) = value {
                    self.infer_expr(val)?
                } else {
                    Type::Void
                };
                
                if let Some(expected) = self.env.get_return_type() {
                    if !return_ty.is_assignable_to(expected) {
                        return Err(TypeError::type_mismatch(expected.clone(), return_ty, *span));
                    }
                }
                Ok(())
            }
            Stmt::Match { expr, arms, span } => {
                let match_ty = self.infer_expr(expr)?;
                
                for arm in arms {
                    self.env.enter_scope();
                    self.check_pattern(&arm.pattern, &match_ty, *span)?;
                    
                    if let Some(guard) = &arm.guard {
                        let guard_ty = self.infer_expr(guard)?;
                        if guard_ty != Type::Bool {
                            return Err(TypeError::type_mismatch(Type::Bool, guard_ty, guard.span()));
                        }
                    }
                    
                    self.check_stmt(&arm.body)?;
                    self.env.leave_scope();
                }
                Ok(())
            }
            Stmt::FnDef { name, type_params, params, return_type, body, span, .. } => {
                self.env.enter_scope();
                let was_in_function = self.in_function;
                self.in_function = true;
                
                // 定义类型参数
                for param in type_params {
                    self.env.define_type_param(GenericParam {
                        name: param.name.clone(),
                        bounds: param.bounds.clone(),
                        default: param.default_type.as_ref().map(|t| Box::new(t.ty.clone())),
                    });
                }
                
                // 定义参数变量
                for param in params {
                    self.env.define_variable(param.name.clone(), param.type_ann.ty.clone(), false)
                        .map_err(|_| TypeError::new(
                            TypeErrorKind::DuplicateDefinition(param.name.clone()),
                            param.span,
                        ))?;
                }
                
                // 设置返回类型
                let ret_ty = return_type.as_ref().map(|t| t.ty.clone()).unwrap_or(Type::Void);
                self.env.set_return_type(Some(ret_ty));
                
                // 检查函数体
                self.check_stmt(body)?;
                
                self.env.set_return_type(None);
                self.in_function = was_in_function;
                self.env.leave_scope();
                Ok(())
            }
            Stmt::TryCatch { try_block, catch_param, catch_block, finally_block, span } => {
                self.check_stmt(try_block)?;
                
                self.env.enter_scope();
                if let Some(param_name) = catch_param {
                    // 异常参数类型为 dynamic（或 Exception）
                    self.env.define_variable(param_name.clone(), Type::Dynamic, false)
                        .map_err(|_| TypeError::new(
                            TypeErrorKind::DuplicateDefinition(param_name.clone()),
                            *span,
                        ))?;
                }
                self.check_stmt(catch_block)?;
                self.env.leave_scope();
                
                if let Some(finally) = finally_block {
                    self.check_stmt(finally)?;
                }
                Ok(())
            }
            Stmt::Throw { value, span } => {
                self.infer_expr(value)?;
                Ok(())
            }
            // 其他语句类型已在第一遍处理
            _ => Ok(()),
        }
    }
    
    /// 推导表达式类型
    fn infer_expr(&mut self, expr: &Expr) -> Result<Type, TypeError> {
        match expr {
            Expr::Integer { .. } => Ok(Type::Int),
            Expr::Float { .. } => Ok(Type::F64),
            Expr::String { .. } | Expr::StringInterpolation { .. } => Ok(Type::String),
            Expr::Bool { .. } => Ok(Type::Bool),
            Expr::Char { .. } => Ok(Type::Char),
            Expr::Null { .. } => Ok(Type::Null),
            
            Expr::Identifier { name, span } => {
                if let Some(var) = self.env.lookup_variable(name) {
                    Ok(var.ty.clone())
                } else if let Some(func) = self.env.lookup_function(name) {
                    Ok(Type::Function {
                        param_types: func.param_types.clone(),
                        return_type: Box::new(func.return_type.clone()),
                    })
                } else if Self::is_builtin_function(name) {
                    // 内置函数返回特殊的函数类型
                    Ok(Self::builtin_function_type(name))
                } else {
                    Err(TypeError::undefined_variable(name.clone(), *span))
                }
            }
            
            Expr::Binary { left, op, right, span } => {
                let left_ty = self.infer_expr(left)?;
                let right_ty = self.infer_expr(right)?;
                self.infer_binary_op(&left_ty, op, &right_ty, *span)
            }
            
            Expr::Unary { op, operand, span } => {
                let operand_ty = self.infer_expr(operand)?;
                self.infer_unary_op(op, &operand_ty, *span)
            }
            
            Expr::Grouping { expr, .. } => self.infer_expr(expr),
            
            Expr::Call { callee, args, span } => {
                let callee_ty = self.infer_expr(callee)?;
                // 提取参数表达式（命名参数的名称在类型检查时忽略）
                let arg_exprs: Vec<&Expr> = args.iter().map(|(_, e)| e).collect();
                self.infer_call(&callee_ty, &arg_exprs, *span)
            }
            
            Expr::Index { object, index, span } => {
                let obj_ty = self.infer_expr(object)?;
                let idx_ty = self.infer_expr(index)?;
                self.infer_index(&obj_ty, &idx_ty, *span)
            }
            
            Expr::Member { object, member, span } => {
                let obj_ty = self.infer_expr(object)?;
                self.infer_member(&obj_ty, member, *span)
            }
            
            Expr::SafeMember { object, member, span } => {
                let obj_ty = self.infer_expr(object)?;
                let inner_ty = match &obj_ty {
                    Type::Nullable(inner) => inner.as_ref().clone(),
                    _ => obj_ty.clone(),
                };
                let member_ty = self.infer_member(&inner_ty, member, *span)?;
                Ok(Type::Nullable(Box::new(member_ty)))
            }
            
            Expr::NullCoalesce { left, right, span } => {
                let left_ty = self.infer_expr(left)?;
                let right_ty = self.infer_expr(right)?;
                
                let inner_ty = match &left_ty {
                    Type::Nullable(inner) => inner.as_ref().clone(),
                    _ => left_ty.clone(),
                };
                
                // 右侧类型应该与左侧内部类型兼容
                if !right_ty.is_assignable_to(&inner_ty) {
                    return Err(TypeError::type_mismatch(inner_ty, right_ty, *span));
                }
                
                Ok(inner_ty)
            }
            
            Expr::Assign { target, op, value, span } => {
                let target_ty = self.infer_expr(target)?;
                let value_ty = self.infer_expr(value)?;
                
                // 检查赋值目标是否是左值
                if !target.is_lvalue() {
                    return Err(TypeError::new(
                        TypeErrorKind::Other("Cannot assign to non-lvalue".to_string()),
                        *span,
                    ));
                }
                
                // 检查常量重新赋值
                if let Expr::Identifier { name, .. } = target.as_ref() {
                    if let Some(var) = self.env.lookup_variable(name) {
                        if var.is_const {
                            return Err(TypeError::new(
                                TypeErrorKind::ConstantReassignment(name.clone()),
                                *span,
                            ));
                        }
                    }
                }
                
                // 检查类型兼容性
                match op {
                    AssignOp::Assign => {
                        if !value_ty.is_assignable_to(&target_ty) {
                            return Err(TypeError::type_mismatch(target_ty, value_ty, *span));
                        }
                    }
                    // 复合赋值运算符
                    _ => {
                        // 检查操作数类型
                        if !target_ty.is_numeric() {
                            return Err(TypeError::new(
                                TypeErrorKind::Other("Compound assignment requires numeric type".to_string()),
                                *span,
                            ));
                        }
                    }
                }
                
                Ok(target_ty)
            }
            
            Expr::Array { elements, span } => {
                if elements.is_empty() {
                    // 空数组，使用类型变量
                    Ok(Type::Slice { element_type: Box::new(Type::fresh_var()) })
                } else {
                    let first_ty = self.infer_expr(&elements[0])?;
                    for elem in &elements[1..] {
                        let elem_ty = self.infer_expr(elem)?;
                        if elem_ty != first_ty {
                            return Err(TypeError::new(
                                TypeErrorKind::IncompatibleTypes {
                                    types: vec![first_ty, elem_ty],
                                    context: "array literal".to_string(),
                                },
                                *span,
                            ));
                        }
                    }
                    Ok(Type::Slice { element_type: Box::new(first_ty) })
                }
            }
            
            Expr::MapLiteral { entries, span } => {
                if entries.is_empty() {
                    Ok(Type::Map {
                        key_type: Box::new(Type::fresh_var()),
                        value_type: Box::new(Type::fresh_var()),
                    })
                } else {
                    let (first_key, first_val) = &entries[0];
                    let key_ty = self.infer_expr(first_key)?;
                    let val_ty = self.infer_expr(first_val)?;
                    
                    for (k, v) in &entries[1..] {
                        let k_ty = self.infer_expr(k)?;
                        let v_ty = self.infer_expr(v)?;
                        if k_ty != key_ty || v_ty != val_ty {
                            return Err(TypeError::new(
                                TypeErrorKind::IncompatibleTypes {
                                    types: vec![key_ty, val_ty, k_ty, v_ty],
                                    context: "map literal".to_string(),
                                },
                                *span,
                            ));
                        }
                    }
                    
                    Ok(Type::Map {
                        key_type: Box::new(key_ty),
                        value_type: Box::new(val_ty),
                    })
                }
            }
            
            Expr::Range { start, end, inclusive, span } => {
                // Range 类型暂时简化处理
                Ok(Type::Slice { element_type: Box::new(Type::Int) })
            }
            
            Expr::Closure { params, return_type, body, span } => {
                self.env.enter_scope();
                
                let param_types: Vec<Type> = params.iter()
                    .map(|p| p.type_ann.ty.clone())
                    .collect();
                
                for param in params {
                    self.env.define_variable(param.name.clone(), param.type_ann.ty.clone(), false)
                        .map_err(|_| TypeError::new(
                            TypeErrorKind::DuplicateDefinition(param.name.clone()),
                            param.span,
                        ))?;
                }
                
                let ret_ty = return_type.as_ref().map(|t| t.ty.clone()).unwrap_or(Type::Void);
                self.env.set_return_type(Some(ret_ty.clone()));
                
                self.check_stmt(body)?;
                
                self.env.set_return_type(None);
                self.env.leave_scope();
                
                Ok(Type::Function {
                    param_types,
                    return_type: Box::new(ret_ty),
                })
            }
            
            Expr::StructLiteral { name, fields, span } => {
                // 先克隆 struct 信息以避免借用冲突
                let struct_fields = if let Some(TypeInfo::Struct(info)) = self.env.lookup_type(name) {
                    info.fields.clone()
                } else {
                    return Err(TypeError::undefined_type(name.clone(), *span));
                };
                
                // 检查字段
                for (field_name, field_expr) in fields {
                    if let Some(field_info) = struct_fields.get(field_name) {
                        let expr_ty = self.infer_expr(field_expr)?;
                        if !expr_ty.is_assignable_to(&field_info.ty) {
                            return Err(TypeError::type_mismatch(
                                field_info.ty.clone(),
                                expr_ty,
                                field_expr.span(),
                            ));
                        }
                    } else {
                        return Err(TypeError::new(
                            TypeErrorKind::UndefinedField {
                                type_name: name.clone(),
                                field_name: field_name.clone(),
                            },
                            *span,
                        ));
                    }
                }
                Ok(Type::Struct(name.clone()))
            }
            
            Expr::New { class_name, args, span } => {
                // 先克隆 class 信息以避免借用冲突
                let (is_abstract, init_param_types) = if let Some(TypeInfo::Class(info)) = self.env.lookup_type(class_name) {
                    let init_types = info.methods.get("init").map(|m| m.param_types.clone());
                    (info.is_abstract, init_types)
                } else {
                    return Err(TypeError::undefined_type(class_name.clone(), *span));
                };
                
                if is_abstract {
                    return Err(TypeError::new(
                        TypeErrorKind::CannotInstantiateAbstract(class_name.clone()),
                        *span,
                    ));
                }
                
                // 检查构造函数参数
                if let Some(param_types) = init_param_types {
                    if args.len() != param_types.len() {
                        return Err(TypeError::argument_count_mismatch(
                            param_types.len(),
                            args.len(),
                            *span,
                        ));
                    }
                    
                    for (arg, param_ty) in args.iter().zip(&param_types) {
                        let arg_ty = self.infer_expr(arg)?;
                        if !arg_ty.is_assignable_to(param_ty) {
                            return Err(TypeError::type_mismatch(
                                param_ty.clone(),
                                arg_ty,
                                arg.span(),
                            ));
                        }
                    }
                }
                
                Ok(Type::Class(class_name.clone()))
            }
            
            Expr::This { span } => {
                if let Some(ty) = self.env.get_this_type() {
                    Ok(ty.clone())
                } else {
                    Err(TypeError::new(
                        TypeErrorKind::Other("'this' can only be used inside a method".to_string()),
                        *span,
                    ))
                }
            }
            
            Expr::Super { span } => {
                if let Some(ty) = self.env.get_this_type() {
                    // 获取父类类型
                    if let Type::Class(name) = ty {
                        if let Some(TypeInfo::Class(info)) = self.env.lookup_type(name) {
                            if let Some(parent_name) = &info.parent {
                                return Ok(Type::Class(parent_name.clone()));
                            }
                        }
                    }
                    Err(TypeError::new(
                        TypeErrorKind::Other("'super' requires a parent class".to_string()),
                        *span,
                    ))
                } else {
                    Err(TypeError::new(
                        TypeErrorKind::Other("'super' can only be used inside a method".to_string()),
                        *span,
                    ))
                }
            }
            
            Expr::Cast { expr, target_type, force, span } => {
                let expr_ty = self.infer_expr(expr)?;
                // 简化：允许所有显式转换
                Ok(target_type.ty.clone())
            }
            
            Expr::TypeCheck { expr, check_type, span } => {
                self.infer_expr(expr)?;
                Ok(Type::Bool)
            }
            
            Expr::Go { call, span } => {
                // go 表达式返回 void
                self.infer_expr(call)?;
                Ok(Type::Void)
            }
            
            _ => Ok(Type::Dynamic),
        }
    }
    
    /// 推导二元运算结果类型
    fn infer_binary_op(&self, left: &Type, op: &BinOp, right: &Type, span: Span) -> Result<Type, TypeError> {
        use BinOp::*;
        
        match op {
            Add | Sub | Mul | Div | Mod | Pow => {
                if left.is_numeric() && right.is_numeric() {
                    // 返回更宽的类型
                    if left.is_float() || right.is_float() {
                        Ok(Type::F64)
                    } else {
                        Ok(Type::Int)
                    }
                } else if matches!(op, Add) && (left == &Type::String || right == &Type::String) {
                    Ok(Type::String)
                } else {
                    Err(TypeError::new(
                        TypeErrorKind::IncompatibleTypes {
                            types: vec![left.clone(), right.clone()],
                            context: format!("binary operator {:?}", op),
                        },
                        span,
                    ))
                }
            }
            Lt | Le | Gt | Ge => {
                if left.is_numeric() && right.is_numeric() {
                    Ok(Type::Bool)
                } else {
                    Err(TypeError::new(
                        TypeErrorKind::IncompatibleTypes {
                            types: vec![left.clone(), right.clone()],
                            context: "comparison".to_string(),
                        },
                        span,
                    ))
                }
            }
            Eq | Ne => {
                // 相等比较允许更宽泛的类型
                Ok(Type::Bool)
            }
            And | Or => {
                if left == &Type::Bool && right == &Type::Bool {
                    Ok(Type::Bool)
                } else {
                    Err(TypeError::type_mismatch(Type::Bool, left.clone(), span))
                }
            }
            BitAnd | BitOr | BitXor | Shl | Shr => {
                if left.is_integer() && right.is_integer() {
                    Ok(Type::Int)
                } else {
                    Err(TypeError::new(
                        TypeErrorKind::IncompatibleTypes {
                            types: vec![left.clone(), right.clone()],
                            context: "bitwise operator".to_string(),
                        },
                        span,
                    ))
                }
            }
        }
    }
    
    /// 推导一元运算结果类型
    fn infer_unary_op(&self, op: &UnaryOp, operand: &Type, span: Span) -> Result<Type, TypeError> {
        use UnaryOp::*;
        
        match op {
            Neg => {
                if operand.is_numeric() {
                    Ok(operand.clone())
                } else {
                    Err(TypeError::type_mismatch(Type::Int, operand.clone(), span))
                }
            }
            Not => {
                if operand == &Type::Bool {
                    Ok(Type::Bool)
                } else {
                    Err(TypeError::type_mismatch(Type::Bool, operand.clone(), span))
                }
            }
            BitNot => {
                if operand.is_integer() {
                    Ok(operand.clone())
                } else {
                    Err(TypeError::type_mismatch(Type::Int, operand.clone(), span))
                }
            }
        }
    }
    
    /// 推导函数调用结果类型
    fn infer_call(&mut self, callee: &Type, args: &[&Expr], span: Span) -> Result<Type, TypeError> {
        match callee {
            Type::Function { param_types, return_type } => {
                if args.len() != param_types.len() {
                    return Err(TypeError::argument_count_mismatch(
                        param_types.len(),
                        args.len(),
                        span,
                    ));
                }
                
                for (arg, param_ty) in args.iter().zip(param_types) {
                    let arg_ty = self.infer_expr(arg)?;
                    if !arg_ty.is_assignable_to(param_ty) {
                        return Err(TypeError::type_mismatch(param_ty.clone(), arg_ty, arg.span()));
                    }
                }
                
                Ok(return_type.as_ref().clone())
            }
            Type::Generic { base_type, type_args: _ } => {
                // 泛型函数调用
                // TODO: 实现泛型参数推导
                match base_type.as_ref() {
                    Type::Function { return_type, .. } => Ok(return_type.as_ref().clone()),
                    _ => Err(TypeError::not_callable(callee.clone(), span)),
                }
            }
            _ => Err(TypeError::not_callable(callee.clone(), span)),
        }
    }
    
    /// 推导索引访问结果类型
    fn infer_index(&self, obj: &Type, idx: &Type, span: Span) -> Result<Type, TypeError> {
        match obj {
            Type::Array { element_type, .. } | Type::Slice { element_type } => {
                if !idx.is_integer() {
                    return Err(TypeError::type_mismatch(Type::Int, idx.clone(), span));
                }
                Ok(element_type.as_ref().clone())
            }
            Type::Map { key_type, value_type } => {
                if !idx.is_assignable_to(key_type) {
                    return Err(TypeError::type_mismatch(key_type.as_ref().clone(), idx.clone(), span));
                }
                Ok(Type::Nullable(value_type.clone()))
            }
            Type::String => {
                if !idx.is_integer() {
                    return Err(TypeError::type_mismatch(Type::Int, idx.clone(), span));
                }
                Ok(Type::Char)
            }
            _ => Err(TypeError::new(TypeErrorKind::NotIndexable(obj.clone()), span)),
        }
    }
    
    /// 推导成员访问结果类型
    fn infer_member(&self, obj: &Type, member: &str, span: Span) -> Result<Type, TypeError> {
        // 首先检查是否是方法
        if let Some(method) = self.env.get_method(obj, member) {
            return Ok(Type::Function {
                param_types: method.param_types.clone(),
                return_type: Box::new(method.return_type.clone()),
            });
        }
        
        // 然后检查字段
        if let Some(field) = self.env.get_field(obj, member) {
            return Ok(field.ty.clone());
        }
        
        // 内置方法
        match obj {
            Type::String => {
                match member {
                    "length" => Ok(Type::Int),
                    "isEmpty" => Ok(Type::Bool),
                    "toUpperCase" | "toLowerCase" | "trim" => Ok(Type::String),
                    "charAt" => Ok(Type::Function {
                        param_types: vec![Type::Int],
                        return_type: Box::new(Type::Char),
                    }),
                    "substring" => Ok(Type::Function {
                        param_types: vec![Type::Int, Type::Int],
                        return_type: Box::new(Type::String),
                    }),
                    _ => Err(TypeError::new(
                        TypeErrorKind::UndefinedMethod {
                            type_name: "string".to_string(),
                            method_name: member.to_string(),
                        },
                        span,
                    ))
                }
            }
            Type::Array { element_type, .. } | Type::Slice { element_type } => {
                match member {
                    "length" => Ok(Type::Int),
                    "isEmpty" => Ok(Type::Bool),
                    "push" | "append" => Ok(Type::Function {
                        param_types: vec![element_type.as_ref().clone()],
                        return_type: Box::new(Type::Void),
                    }),
                    "pop" => Ok(Type::Function {
                        param_types: vec![],
                        return_type: Box::new(Type::Nullable(element_type.clone())),
                    }),
                    _ => Err(TypeError::new(
                        TypeErrorKind::UndefinedMethod {
                            type_name: obj.to_string(),
                            method_name: member.to_string(),
                        },
                        span,
                    ))
                }
            }
            _ => Err(TypeError::new(
                TypeErrorKind::UndefinedField {
                    type_name: obj.to_string(),
                    field_name: member.to_string(),
                },
                span,
            ))
        }
    }
    
    /// 获取迭代器元素类型
    fn get_iterator_element_type(&self, ty: &Type, span: Span) -> Result<Type, TypeError> {
        match ty {
            Type::Array { element_type, .. } | Type::Slice { element_type } => {
                Ok(element_type.as_ref().clone())
            }
            Type::String => Ok(Type::Char),
            Type::Map { key_type, value_type } => {
                // Map 迭代返回 (key, value) 元组
                Ok(Type::Tuple(vec![
                    key_type.as_ref().clone(),
                    value_type.as_ref().clone(),
                ]))
            }
            _ => Err(TypeError::new(TypeErrorKind::NotIterable(ty.clone()), span))
        }
    }
    
    /// 检查模式
    fn check_pattern(&mut self, pattern: &MatchPattern, expected_ty: &Type, span: Span) -> Result<(), TypeError> {
        match pattern {
            MatchPattern::Literal(expr) => {
                let lit_ty = self.infer_expr(expr)?;
                if !lit_ty.is_assignable_to(expected_ty) {
                    return Err(TypeError::type_mismatch(expected_ty.clone(), lit_ty, span));
                }
            }
            MatchPattern::Variable(name) => {
                self.env.define_variable(name.clone(), expected_ty.clone(), false)
                    .map_err(|_| TypeError::new(
                        TypeErrorKind::DuplicateDefinition(name.clone()),
                        span,
                    ))?;
            }
            MatchPattern::Wildcard => {}
            MatchPattern::Or(patterns) => {
                for p in patterns {
                    self.check_pattern(p, expected_ty, span)?;
                }
            }
            MatchPattern::Range { start, end, .. } => {
                let start_ty = self.infer_expr(start)?;
                let end_ty = self.infer_expr(end)?;
                if !start_ty.is_assignable_to(expected_ty) || !end_ty.is_assignable_to(expected_ty) {
                    return Err(TypeError::type_mismatch(expected_ty.clone(), start_ty, span));
                }
            }
            MatchPattern::Type { name, type_ann } => {
                self.env.define_variable(name.clone(), type_ann.ty.clone(), false)
                    .map_err(|_| TypeError::new(
                        TypeErrorKind::DuplicateDefinition(name.clone()),
                        span,
                    ))?;
            }
        }
        Ok(())
    }
    
    // 辅助方法：转换类型参数
    fn convert_type_params(&self, params: &[TypeParam]) -> Vec<GenericParam> {
        params.iter().map(|p| GenericParam {
            name: p.name.clone(),
            bounds: p.bounds.clone(),
            default: p.default_type.as_ref().map(|t| Box::new(t.ty.clone())),
        }).collect()
    }
    
    // 辅助方法：收集 struct 字段
    fn collect_struct_fields(&self, fields: &[crate::parser::ast::StructField]) -> HashMap<String, FieldInfo> {
        fields.iter().map(|f| (f.name.clone(), FieldInfo {
            name: f.name.clone(),
            ty: f.type_ann.ty.clone(),
            is_mutable: true,
            visibility: match f.visibility {
                crate::parser::ast::Visibility::Public => Visibility::Public,
                crate::parser::ast::Visibility::Private => Visibility::Private,
                crate::parser::ast::Visibility::Protected => Visibility::Protected,
                crate::parser::ast::Visibility::Internal => Visibility::Internal,
            },
        })).collect()
    }
    
    // 辅助方法：收集 struct 方法
    fn collect_struct_methods(&self, methods: &[crate::parser::ast::StructMethod]) -> HashMap<String, FunctionInfo> {
        methods.iter().map(|m| (m.name.clone(), FunctionInfo {
            name: m.name.clone(),
            type_params: Vec::new(),
            param_types: m.params.iter().map(|p| p.type_ann.ty.clone()).collect(),
            param_names: m.params.iter().map(|p| p.name.clone()).collect(),
            return_type: m.return_type.as_ref().map(|t| t.ty.clone()).unwrap_or(Type::Void),
            is_method: true,
            owner_type: None,
        })).collect()
    }
    
    // 辅助方法：收集 class 字段
    fn collect_class_fields(&self, fields: &[crate::parser::ast::ClassField]) -> HashMap<String, FieldInfo> {
        fields.iter()
            .filter(|f| !f.is_static)
            .map(|f| (f.name.clone(), FieldInfo {
                name: f.name.clone(),
                ty: f.type_ann.as_ref().map(|t| t.ty.clone()).unwrap_or(Type::Dynamic),
                is_mutable: !f.is_const,
                visibility: match f.visibility {
                    crate::parser::ast::Visibility::Public => Visibility::Public,
                    crate::parser::ast::Visibility::Private => Visibility::Private,
                    crate::parser::ast::Visibility::Protected => Visibility::Protected,
                    crate::parser::ast::Visibility::Internal => Visibility::Internal,
                },
            }))
            .collect()
    }
    
    // 辅助方法：收集 class 静态字段
    fn collect_class_static_fields(&self, fields: &[crate::parser::ast::ClassField]) -> HashMap<String, FieldInfo> {
        fields.iter()
            .filter(|f| f.is_static)
            .map(|f| (f.name.clone(), FieldInfo {
                name: f.name.clone(),
                ty: f.type_ann.as_ref().map(|t| t.ty.clone()).unwrap_or(Type::Dynamic),
                is_mutable: !f.is_const,
                visibility: match f.visibility {
                    crate::parser::ast::Visibility::Public => Visibility::Public,
                    crate::parser::ast::Visibility::Private => Visibility::Private,
                    crate::parser::ast::Visibility::Protected => Visibility::Protected,
                    crate::parser::ast::Visibility::Internal => Visibility::Internal,
                },
            }))
            .collect()
    }
    
    // 辅助方法：收集 class 方法
    fn collect_class_methods(&self, methods: &[crate::parser::ast::ClassMethod]) -> HashMap<String, FunctionInfo> {
        methods.iter()
            .filter(|m| !m.is_static)
            .map(|m| (m.name.clone(), FunctionInfo {
                name: m.name.clone(),
                type_params: Vec::new(),
                param_types: m.params.iter().map(|p| p.type_ann.ty.clone()).collect(),
                param_names: m.params.iter().map(|p| p.name.clone()).collect(),
                return_type: m.return_type.as_ref().map(|t| t.ty.clone()).unwrap_or(Type::Void),
                is_method: true,
                owner_type: None,
            }))
            .collect()
    }
    
    // 辅助方法：收集 class 静态方法
    fn collect_class_static_methods(&self, methods: &[crate::parser::ast::ClassMethod]) -> HashMap<String, FunctionInfo> {
        methods.iter()
            .filter(|m| m.is_static)
            .map(|m| (m.name.clone(), FunctionInfo {
                name: m.name.clone(),
                type_params: Vec::new(),
                param_types: m.params.iter().map(|p| p.type_ann.ty.clone()).collect(),
                param_names: m.params.iter().map(|p| p.name.clone()).collect(),
                return_type: m.return_type.as_ref().map(|t| t.ty.clone()).unwrap_or(Type::Void),
                is_method: false,
                owner_type: None,
            }))
            .collect()
    }
    
    // 辅助方法：收集 interface 方法
    fn collect_interface_methods(&self, methods: &[crate::parser::ast::InterfaceMethod]) -> HashMap<String, FunctionInfo> {
        methods.iter().map(|m| (m.name.clone(), FunctionInfo {
            name: m.name.clone(),
            type_params: Vec::new(),
            param_types: m.params.iter().map(|p| p.type_ann.ty.clone()).collect(),
            param_names: m.params.iter().map(|p| p.name.clone()).collect(),
            return_type: m.return_type.as_ref().map(|t| t.ty.clone()).unwrap_or(Type::Void),
            is_method: true,
            owner_type: None,
        })).collect()
    }
    
    // 辅助方法：收集 trait 方法
    fn collect_trait_methods(&self, methods: &[crate::parser::ast::TraitMethod]) -> HashMap<String, FunctionInfo> {
        methods.iter().map(|m| (m.name.clone(), FunctionInfo {
            name: m.name.clone(),
            type_params: Vec::new(),
            param_types: m.params.iter().map(|p| p.type_ann.ty.clone()).collect(),
            param_names: m.params.iter().map(|p| p.name.clone()).collect(),
            return_type: m.return_type.as_ref().map(|t| t.ty.clone()).unwrap_or(Type::Void),
            is_method: true,
            owner_type: None,
        })).collect()
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}
