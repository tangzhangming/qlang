//! 类型环境
//! 
//! 管理类型作用域、变量类型、函数签名等

use std::collections::HashMap;
use crate::types::{Type, TypeBound, GenericParam, FunctionSignature, TraitDef, InterfaceDef, TraitImpl};

/// 变量/常量信息
#[derive(Debug, Clone)]
pub struct VariableInfo {
    /// 变量名
    pub name: String,
    /// 变量类型
    pub ty: Type,
    /// 是否是常量
    pub is_const: bool,
    /// 是否已初始化
    pub initialized: bool,
}

/// 函数信息
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    /// 函数名
    pub name: String,
    /// 泛型参数
    pub type_params: Vec<GenericParam>,
    /// 参数类型
    pub param_types: Vec<Type>,
    /// 参数名
    pub param_names: Vec<String>,
    /// 必需参数数量（不包括有默认值的参数）
    pub required_params: usize,
    /// 返回类型
    pub return_type: Type,
    /// 是否是方法
    pub is_method: bool,
    /// 所属类型（如果是方法）
    pub owner_type: Option<String>,
}

/// 类信息
#[derive(Debug, Clone)]
pub struct ClassInfo {
    /// 类名
    pub name: String,
    /// 泛型参数
    pub type_params: Vec<GenericParam>,
    /// 父类
    pub parent: Option<String>,
    /// 实现的接口
    pub interfaces: Vec<String>,
    /// 使用的 Trait
    pub traits: Vec<String>,
    /// 字段
    pub fields: HashMap<String, FieldInfo>,
    /// 方法
    pub methods: HashMap<String, FunctionInfo>,
    /// 静态字段
    pub static_fields: HashMap<String, FieldInfo>,
    /// 静态方法
    pub static_methods: HashMap<String, FunctionInfo>,
    /// 是否是抽象类
    pub is_abstract: bool,
}

/// 结构体信息
#[derive(Debug, Clone)]
pub struct StructInfo {
    /// 结构体名
    pub name: String,
    /// 泛型参数
    pub type_params: Vec<GenericParam>,
    /// 实现的接口
    pub interfaces: Vec<String>,
    /// 字段
    pub fields: HashMap<String, FieldInfo>,
    /// 方法
    pub methods: HashMap<String, FunctionInfo>,
}

/// 字段信息
#[derive(Debug, Clone)]
pub struct FieldInfo {
    /// 字段名
    pub name: String,
    /// 字段类型
    pub ty: Type,
    /// 是否是可变的
    pub is_mutable: bool,
    /// 可见性
    pub visibility: Visibility,
}

/// 可见性
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
    Protected,
    Internal,
}

/// Trait 信息
#[derive(Debug, Clone)]
pub struct TraitInfo {
    /// Trait 名
    pub name: String,
    /// 泛型参数
    pub type_params: Vec<GenericParam>,
    /// 父 Trait
    pub super_traits: Vec<TypeBound>,
    /// 方法签名
    pub methods: HashMap<String, FunctionInfo>,
    /// 默认方法实现（方法名 -> 有默认实现）
    pub default_methods: HashMap<String, bool>,
}

/// 接口信息
#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    /// 接口名
    pub name: String,
    /// 泛型参数
    pub type_params: Vec<GenericParam>,
    /// 父接口
    pub super_interfaces: Vec<String>,
    /// 方法签名
    pub methods: HashMap<String, FunctionInfo>,
}

/// 枚举信息
#[derive(Debug, Clone)]
pub struct EnumInfo {
    /// 枚举名
    pub name: String,
    /// 变体
    pub variants: HashMap<String, EnumVariantInfo>,
    /// 方法
    pub methods: HashMap<String, FunctionInfo>,
}

/// 枚举变体信息
#[derive(Debug, Clone)]
pub struct EnumVariantInfo {
    /// 变体名
    pub name: String,
    /// 关联值类型（如果有）
    pub value_type: Option<Type>,
    /// 关联数据字段
    pub fields: Vec<(String, Type)>,
}

/// 类型信息（聚合所有类型定义）
#[derive(Debug, Clone)]
pub enum TypeInfo {
    Class(ClassInfo),
    Struct(StructInfo),
    Interface(InterfaceInfo),
    Trait(TraitInfo),
    Enum(EnumInfo),
    /// 类型别名
    Alias {
        name: String,
        actual_type: Type,
    },
}

/// 类型作用域
#[derive(Debug, Clone)]
pub struct TypeScope {
    /// 变量表
    variables: HashMap<String, VariableInfo>,
    /// 函数表（当前作用域定义的函数）
    functions: HashMap<String, FunctionInfo>,
    /// 泛型参数（当前作用域的类型参数）
    type_params: HashMap<String, GenericParam>,
    /// 父作用域索引（-1 表示全局）
    parent: Option<usize>,
}

impl TypeScope {
    /// 创建新的作用域
    pub fn new(parent: Option<usize>) -> Self {
        Self {
            variables: HashMap::new(),
            functions: HashMap::new(),
            type_params: HashMap::new(),
            parent,
        }
    }
    
    /// 定义变量
    pub fn define_variable(&mut self, name: String, ty: Type, is_const: bool) -> Result<(), String> {
        if self.variables.contains_key(&name) {
            return Err(format!("变量 '{}' 已在当前作用域中定义", name));
        }
        self.variables.insert(name.clone(), VariableInfo {
            name,
            ty,
            is_const,
            initialized: true,
        });
        Ok(())
    }
    
    /// 查找变量（仅当前作用域）
    pub fn get_variable(&self, name: &str) -> Option<&VariableInfo> {
        self.variables.get(name)
    }
    
    /// 定义类型参数
    pub fn define_type_param(&mut self, param: GenericParam) {
        self.type_params.insert(param.name.clone(), param);
    }
    
    /// 查找类型参数
    pub fn get_type_param(&self, name: &str) -> Option<&GenericParam> {
        self.type_params.get(name)
    }
}

/// 类型环境
#[derive(Debug)]
pub struct TypeEnvironment {
    /// 作用域栈
    scopes: Vec<TypeScope>,
    /// 当前作用域索引
    current_scope: usize,
    /// 类型定义表
    types: HashMap<String, TypeInfo>,
    /// 全局函数表
    functions: HashMap<String, FunctionInfo>,
    /// Trait 实现表
    trait_impls: Vec<TraitImpl>,
    /// 当前 this 类型（在方法内部使用）
    current_this_type: Option<Type>,
    /// 当前函数返回类型
    current_return_type: Option<Type>,
}

impl TypeEnvironment {
    /// 创建新的类型环境
    pub fn new() -> Self {
        let mut env = Self {
            scopes: vec![TypeScope::new(None)],
            current_scope: 0,
            types: HashMap::new(),
            functions: HashMap::new(),
            trait_impls: Vec::new(),
            current_this_type: None,
            current_return_type: None,
        };
        
        // 注册内置类型
        env.register_builtin_types();
        
        env
    }
    
    /// 注册内置类型
    fn register_builtin_types(&mut self) {
        // 可以在这里注册标准库类型
    }
    
    /// 进入新的作用域
    pub fn enter_scope(&mut self) {
        let new_scope = TypeScope::new(Some(self.current_scope));
        self.scopes.push(new_scope);
        self.current_scope = self.scopes.len() - 1;
    }
    
    /// 离开当前作用域
    pub fn leave_scope(&mut self) {
        if let Some(parent) = self.scopes[self.current_scope].parent {
            self.current_scope = parent;
        }
    }
    
    /// 定义变量
    pub fn define_variable(&mut self, name: String, ty: Type, is_const: bool) -> Result<(), String> {
        self.scopes[self.current_scope].define_variable(name, ty, is_const)
    }
    
    /// 查找变量（向上搜索所有作用域）
    pub fn lookup_variable(&self, name: &str) -> Option<&VariableInfo> {
        let mut scope_idx = Some(self.current_scope);
        while let Some(idx) = scope_idx {
            if let Some(var) = self.scopes[idx].get_variable(name) {
                return Some(var);
            }
            scope_idx = self.scopes[idx].parent;
        }
        None
    }
    
    /// 定义类型参数（在当前作用域）
    pub fn define_type_param(&mut self, param: GenericParam) {
        self.scopes[self.current_scope].define_type_param(param);
    }
    
    /// 查找类型参数（向上搜索）
    pub fn lookup_type_param(&self, name: &str) -> Option<&GenericParam> {
        let mut scope_idx = Some(self.current_scope);
        while let Some(idx) = scope_idx {
            if let Some(param) = self.scopes[idx].get_type_param(name) {
                return Some(param);
            }
            scope_idx = self.scopes[idx].parent;
        }
        None
    }
    
    /// 注册类型定义
    pub fn register_type(&mut self, name: String, info: TypeInfo) -> Result<(), String> {
        if self.types.contains_key(&name) {
            return Err(format!("类型 '{}' 已定义", name));
        }
        self.types.insert(name, info);
        Ok(())
    }
    
    /// 查找类型定义
    pub fn lookup_type(&self, name: &str) -> Option<&TypeInfo> {
        self.types.get(name)
    }
    
    /// 注册函数
    pub fn register_function(&mut self, name: String, info: FunctionInfo) -> Result<(), String> {
        if self.functions.contains_key(&name) {
            return Err(format!("函数 '{}' 已定义", name));
        }
        self.functions.insert(name, info);
        Ok(())
    }
    
    /// 查找函数
    pub fn lookup_function(&self, name: &str) -> Option<&FunctionInfo> {
        // 先查找当前作用域的函数
        let mut scope_idx = Some(self.current_scope);
        while let Some(idx) = scope_idx {
            if let Some(func) = self.scopes[idx].functions.get(name) {
                return Some(func);
            }
            scope_idx = self.scopes[idx].parent;
        }
        // 再查找全局函数
        self.functions.get(name)
    }
    
    /// 注册 Trait 实现
    pub fn register_trait_impl(&mut self, impl_: TraitImpl) {
        self.trait_impls.push(impl_);
    }
    
    /// 查找类型是否实现了某个 Trait
    pub fn find_trait_impl(&self, ty: &Type, trait_name: &str) -> Option<&TraitImpl> {
        self.trait_impls.iter().find(|impl_| {
            impl_.trait_bound.trait_name == trait_name && self.type_matches(&impl_.for_type, ty)
        })
    }
    
    /// 检查两个类型是否匹配（简化版）
    fn type_matches(&self, pattern: &Type, actual: &Type) -> bool {
        // 简化实现：只检查基本匹配
        pattern == actual || matches!(pattern, Type::TypeParameter { .. })
    }
    
    /// 设置当前 this 类型
    pub fn set_this_type(&mut self, ty: Option<Type>) {
        self.current_this_type = ty;
    }
    
    /// 获取当前 this 类型
    pub fn get_this_type(&self) -> Option<&Type> {
        self.current_this_type.as_ref()
    }
    
    /// 设置当前函数返回类型
    pub fn set_return_type(&mut self, ty: Option<Type>) {
        self.current_return_type = ty;
    }
    
    /// 获取当前函数返回类型
    pub fn get_return_type(&self) -> Option<&Type> {
        self.current_return_type.as_ref()
    }
    
    /// 解析类型名称到完整类型
    pub fn resolve_type(&self, name: &str) -> Option<Type> {
        // 先检查是否是类型参数
        if let Some(param) = self.lookup_type_param(name) {
            return Some(Type::TypeParameter {
                name: param.name.clone(),
                bounds: param.bounds.clone(),
            });
        }
        
        // 检查是否是已定义的类型
        if let Some(type_info) = self.lookup_type(name) {
            return Some(match type_info {
                TypeInfo::Class(info) => Type::Class(info.name.clone()),
                TypeInfo::Struct(info) => Type::Struct(info.name.clone()),
                TypeInfo::Interface(info) => Type::Interface(info.name.clone()),
                TypeInfo::Trait(info) => Type::Trait(info.name.clone()),
                TypeInfo::Enum(info) => Type::Enum(info.name.clone()),
                TypeInfo::Alias { actual_type, .. } => actual_type.clone(),
            });
        }
        
        None
    }
    
    /// 获取类型的字段
    pub fn get_field(&self, ty: &Type, field_name: &str) -> Option<&FieldInfo> {
        let type_name = match ty {
            Type::Class(name) | Type::Struct(name) => name,
            Type::Generic { base_type, .. } => match base_type.as_ref() {
                Type::Class(name) | Type::Struct(name) => name,
                _ => return None,
            },
            _ => return None,
        };
        
        match self.lookup_type(type_name)? {
            TypeInfo::Class(info) => info.fields.get(field_name),
            TypeInfo::Struct(info) => info.fields.get(field_name),
            _ => None,
        }
    }
    
    /// 获取类型的方法
    pub fn get_method(&self, ty: &Type, method_name: &str) -> Option<&FunctionInfo> {
        let type_name = match ty {
            Type::Class(name) | Type::Struct(name) => name,
            Type::Generic { base_type, .. } => match base_type.as_ref() {
                Type::Class(name) | Type::Struct(name) => name,
                _ => return None,
            },
            _ => return None,
        };
        
        match self.lookup_type(type_name)? {
            TypeInfo::Class(info) => info.methods.get(method_name),
            TypeInfo::Struct(info) => info.methods.get(method_name),
            TypeInfo::Trait(info) => info.methods.get(method_name),
            TypeInfo::Interface(info) => info.methods.get(method_name),
            _ => None,
        }
    }
}

impl Default for TypeEnvironment {
    fn default() -> Self {
        Self::new()
    }
}
