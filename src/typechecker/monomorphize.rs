//! 泛型单态化
//! 
//! 将泛型代码实例化为具体类型的代码

use std::collections::{HashMap, HashSet};
use crate::parser::{Stmt, Expr, Program};
use crate::parser::ast::{ClassMethod, StructMethod, TypeAnnotation};
use crate::types::{Type, Substitution, GenericParam};
use crate::lexer::Span;

/// 单态化实例的唯一标识
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MonoKey {
    /// 原始名称
    pub base_name: String,
    /// 类型参数的字符串表示（用于 Hash）
    pub type_args_str: Vec<String>,
}

impl MonoKey {
    /// 创建新的单态化键
    pub fn new(base_name: impl Into<String>, type_args: Vec<Type>) -> Self {
        let type_args_str = type_args.iter()
            .map(|t| mangle_type(t))
            .collect();
        Self {
            base_name: base_name.into(),
            type_args_str,
        }
    }
    
    /// 从类型参数创建
    pub fn from_types(base_name: impl Into<String>, type_args: &[Type]) -> Self {
        let type_args_str = type_args.iter()
            .map(|t| mangle_type(t))
            .collect();
        Self {
            base_name: base_name.into(),
            type_args_str,
        }
    }
    
    /// 生成单态化后的名称
    pub fn mangled_name(&self) -> String {
        if self.type_args_str.is_empty() {
            self.base_name.clone()
        } else {
            format!("{}$${}", self.base_name, self.type_args_str.join("_"))
        }
    }
}

/// 将类型名称转换为可用作标识符的字符串
fn mangle_type(ty: &Type) -> String {
    match ty {
        Type::Int => "int".to_string(),
        Type::Uint => "uint".to_string(),
        Type::I8 => "i8".to_string(),
        Type::I16 => "i16".to_string(),
        Type::I32 => "i32".to_string(),
        Type::I64 => "i64".to_string(),
        Type::U8 => "u8".to_string(),
        Type::U16 => "u16".to_string(),
        Type::U32 => "u32".to_string(),
        Type::U64 => "u64".to_string(),
        Type::F32 => "f32".to_string(),
        Type::F64 => "f64".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Byte => "byte".to_string(),
        Type::Char => "char".to_string(),
        Type::String => "string".to_string(),
        Type::Void => "void".to_string(),
        Type::Class(name) | Type::Struct(name) | Type::Enum(name) => name.clone(),
        Type::Array { element_type, size } => {
            format!("arr{}__{}", size, mangle_type(element_type))
        }
        Type::Slice { element_type } => {
            format!("slice__{}", mangle_type(element_type))
        }
        Type::Map { key_type, value_type } => {
            format!("map__{}_{}", mangle_type(key_type), mangle_type(value_type))
        }
        Type::Nullable(inner) => {
            format!("nullable__{}", mangle_type(inner))
        }
        Type::Generic { base_type, type_args } => {
            let base = mangle_type(base_type);
            let args: Vec<String> = type_args.iter().map(|t| mangle_type(t)).collect();
            format!("{}__{}", base, args.join("_"))
        }
        _ => format!("{:?}", ty).to_lowercase().replace(" ", "_"),
    }
}

/// 单态化后的类定义
#[derive(Debug, Clone)]
pub struct MonomorphizedClass {
    /// 单态化后的名称
    pub name: String,
    /// 原始名称
    pub original_name: String,
    /// 类型替换表
    pub substitution: Substitution,
    /// 字段（类型已替换）
    pub fields: Vec<MonomorphizedField>,
    /// 方法（类型已替换）
    pub methods: Vec<MonomorphizedMethod>,
    /// 父类（如果有）
    pub parent: Option<String>,
    /// 是否是抽象类
    pub is_abstract: bool,
}

/// 单态化后的结构体定义
#[derive(Debug, Clone)]
pub struct MonomorphizedStruct {
    /// 单态化后的名称
    pub name: String,
    /// 原始名称
    pub original_name: String,
    /// 类型替换表
    pub substitution: Substitution,
    /// 字段（类型已替换）
    pub fields: Vec<MonomorphizedField>,
    /// 方法（类型已替换）
    pub methods: Vec<MonomorphizedMethod>,
}

/// 单态化后的字段
#[derive(Debug, Clone)]
pub struct MonomorphizedField {
    pub name: String,
    pub ty: Type,
    pub is_mutable: bool,
}

/// 单态化后的方法
#[derive(Debug, Clone)]
pub struct MonomorphizedMethod {
    pub name: String,
    pub param_types: Vec<Type>,
    pub param_names: Vec<String>,
    pub return_type: Type,
    pub is_static: bool,
}

/// 单态化后的函数
#[derive(Debug, Clone)]
pub struct MonomorphizedFunction {
    /// 单态化后的名称
    pub name: String,
    /// 原始名称
    pub original_name: String,
    /// 类型替换表
    pub substitution: Substitution,
    /// 参数类型
    pub param_types: Vec<Type>,
    /// 参数名
    pub param_names: Vec<String>,
    /// 返回类型
    pub return_type: Type,
}

/// 待单态化请求
#[derive(Debug, Clone)]
struct PendingRequest {
    key: MonoKey,
    type_args: Vec<Type>,
}

/// 单态化器
pub struct Monomorphizer {
    /// 已单态化的类
    monomorphized_classes: HashMap<MonoKey, MonomorphizedClass>,
    /// 已单态化的结构体
    monomorphized_structs: HashMap<MonoKey, MonomorphizedStruct>,
    /// 已单态化的函数
    monomorphized_functions: HashMap<MonoKey, MonomorphizedFunction>,
    /// 待处理的实例化请求
    pending: Vec<PendingRequest>,
    /// 原始类定义
    class_defs: HashMap<String, ClassDefInfo>,
    /// 原始结构体定义
    struct_defs: HashMap<String, StructDefInfo>,
    /// 原始函数定义
    function_defs: HashMap<String, FunctionDefInfo>,
}

/// 类定义信息（用于单态化）
#[derive(Debug, Clone)]
struct ClassDefInfo {
    name: String,
    type_params: Vec<GenericParam>,
    fields: Vec<(String, Type, bool)>, // (name, type, is_mutable)
    methods: Vec<MethodInfo>,
    parent: Option<String>,
    is_abstract: bool,
}

/// 结构体定义信息
#[derive(Debug, Clone)]
struct StructDefInfo {
    name: String,
    type_params: Vec<GenericParam>,
    fields: Vec<(String, Type, bool)>,
    methods: Vec<MethodInfo>,
}

/// 函数定义信息
#[derive(Debug, Clone)]
struct FunctionDefInfo {
    name: String,
    type_params: Vec<GenericParam>,
    param_types: Vec<Type>,
    param_names: Vec<String>,
    return_type: Type,
}

/// 方法信息
#[derive(Debug, Clone)]
struct MethodInfo {
    name: String,
    param_types: Vec<Type>,
    param_names: Vec<String>,
    return_type: Type,
    is_static: bool,
}

impl Monomorphizer {
    /// 创建新的单态化器
    pub fn new() -> Self {
        Self {
            monomorphized_classes: HashMap::new(),
            monomorphized_structs: HashMap::new(),
            monomorphized_functions: HashMap::new(),
            pending: Vec::new(),
            class_defs: HashMap::new(),
            struct_defs: HashMap::new(),
            function_defs: HashMap::new(),
        }
    }
    
    /// 收集程序中的泛型定义
    pub fn collect_definitions(&mut self, program: &Program) {
        for stmt in &program.statements {
            match stmt {
                Stmt::ClassDef { name, type_params, fields, methods, parent, is_abstract, .. } => {
                    if !type_params.is_empty() {
                        let info = ClassDefInfo {
                            name: name.clone(),
                            type_params: type_params.iter().map(|p| GenericParam {
                                name: p.name.clone(),
                                bounds: p.bounds.clone(),
                                default: p.default_type.as_ref().map(|t| Box::new(t.ty.clone())),
                            }).collect(),
                            fields: fields.iter().map(|f| {
                                (f.name.clone(), f.type_ann.as_ref().map(|t| t.ty.clone()).unwrap_or(Type::Dynamic), !f.is_const)
                            }).collect(),
                            methods: methods.iter().map(|m| MethodInfo {
                                name: m.name.clone(),
                                param_types: m.params.iter().map(|p| p.type_ann.ty.clone()).collect(),
                                param_names: m.params.iter().map(|p| p.name.clone()).collect(),
                                return_type: m.return_type.as_ref().map(|t| t.ty.clone()).unwrap_or(Type::Void),
                                is_static: m.is_static,
                            }).collect(),
                            parent: parent.clone(),
                            is_abstract: *is_abstract,
                        };
                        self.class_defs.insert(name.clone(), info);
                    }
                }
                Stmt::StructDef { name, type_params, fields, methods, .. } => {
                    if !type_params.is_empty() {
                        let info = StructDefInfo {
                            name: name.clone(),
                            type_params: type_params.iter().map(|p| GenericParam {
                                name: p.name.clone(),
                                bounds: p.bounds.clone(),
                                default: p.default_type.as_ref().map(|t| Box::new(t.ty.clone())),
                            }).collect(),
                            fields: fields.iter().map(|f| {
                                (f.name.clone(), f.type_ann.ty.clone(), true)
                            }).collect(),
                            methods: methods.iter().map(|m| MethodInfo {
                                name: m.name.clone(),
                                param_types: m.params.iter().map(|p| p.type_ann.ty.clone()).collect(),
                                param_names: m.params.iter().map(|p| p.name.clone()).collect(),
                                return_type: m.return_type.as_ref().map(|t| t.ty.clone()).unwrap_or(Type::Void),
                                is_static: false,
                            }).collect(),
                        };
                        self.struct_defs.insert(name.clone(), info);
                    }
                }
                Stmt::FnDef { name, type_params, params, return_type, .. } => {
                    if !type_params.is_empty() {
                        let info = FunctionDefInfo {
                            name: name.clone(),
                            type_params: type_params.iter().map(|p| GenericParam {
                                name: p.name.clone(),
                                bounds: p.bounds.clone(),
                                default: p.default_type.as_ref().map(|t| Box::new(t.ty.clone())),
                            }).collect(),
                            param_types: params.iter().map(|p| p.type_ann.ty.clone()).collect(),
                            param_names: params.iter().map(|p| p.name.clone()).collect(),
                            return_type: return_type.as_ref().map(|t| t.ty.clone()).unwrap_or(Type::Void),
                        };
                        self.function_defs.insert(name.clone(), info);
                    }
                }
                _ => {}
            }
        }
    }
    
    /// 请求单态化一个泛型类
    pub fn request_class(&mut self, name: &str, type_args: Vec<Type>) -> String {
        let key = MonoKey::new(name, type_args.clone());
        
        // 如果已经单态化过，直接返回
        if self.monomorphized_classes.contains_key(&key) {
            return key.mangled_name();
        }
        
        // 添加到待处理队列
        let already_pending = self.pending.iter().any(|r| r.key == key);
        if !already_pending {
            self.pending.push(PendingRequest { key: key.clone(), type_args });
        }
        
        key.mangled_name()
    }
    
    /// 请求单态化一个泛型结构体
    pub fn request_struct(&mut self, name: &str, type_args: Vec<Type>) -> String {
        let key = MonoKey::new(name, type_args.clone());
        
        if self.monomorphized_structs.contains_key(&key) {
            return key.mangled_name();
        }
        
        let already_pending = self.pending.iter().any(|r| r.key == key);
        if !already_pending {
            self.pending.push(PendingRequest { key: key.clone(), type_args });
        }
        
        key.mangled_name()
    }
    
    /// 请求单态化一个泛型函数
    pub fn request_function(&mut self, name: &str, type_args: Vec<Type>) -> String {
        let key = MonoKey::new(name, type_args.clone());
        
        if self.monomorphized_functions.contains_key(&key) {
            return key.mangled_name();
        }
        
        let already_pending = self.pending.iter().any(|r| r.key == key);
        if !already_pending {
            self.pending.push(PendingRequest { key: key.clone(), type_args });
        }
        
        key.mangled_name()
    }
    
    /// 处理所有待单态化的请求
    pub fn process_all(&mut self) {
        while let Some(request) = self.pending.pop() {
            self.monomorphize(&request);
        }
    }
    
    /// 执行单态化
    fn monomorphize(&mut self, request: &PendingRequest) {
        let key = &request.key;
        let type_args = &request.type_args;
        
        // 尝试作为类单态化
        if let Some(class_def) = self.class_defs.get(&key.base_name).cloned() {
            self.monomorphize_class(key, type_args, &class_def);
            return;
        }
        
        // 尝试作为结构体单态化
        if let Some(struct_def) = self.struct_defs.get(&key.base_name).cloned() {
            self.monomorphize_struct(key, type_args, &struct_def);
            return;
        }
        
        // 尝试作为函数单态化
        if let Some(func_def) = self.function_defs.get(&key.base_name).cloned() {
            self.monomorphize_function(key, type_args, &func_def);
        }
    }
    
    /// 单态化类
    fn monomorphize_class(&mut self, key: &MonoKey, type_args: &[Type], class_def: &ClassDefInfo) {
        // 构建类型替换表
        let substitution = self.build_substitution(&class_def.type_params, type_args);
        
        // 替换字段类型
        let fields: Vec<MonomorphizedField> = class_def.fields.iter().map(|(name, ty, is_mutable)| {
            MonomorphizedField {
                name: name.clone(),
                ty: ty.substitute(&substitution),
                is_mutable: *is_mutable,
            }
        }).collect();
        
        // 替换方法类型
        let methods: Vec<MonomorphizedMethod> = class_def.methods.iter().map(|m| {
            MonomorphizedMethod {
                name: m.name.clone(),
                param_types: m.param_types.iter().map(|t| t.substitute(&substitution)).collect(),
                param_names: m.param_names.clone(),
                return_type: m.return_type.substitute(&substitution),
                is_static: m.is_static,
            }
        }).collect();
        
        let mono_class = MonomorphizedClass {
            name: key.mangled_name(),
            original_name: key.base_name.clone(),
            substitution,
            fields,
            methods,
            parent: class_def.parent.clone(),
            is_abstract: class_def.is_abstract,
        };
        
        self.monomorphized_classes.insert(key.clone(), mono_class);
    }
    
    /// 单态化结构体
    fn monomorphize_struct(&mut self, key: &MonoKey, type_args: &[Type], struct_def: &StructDefInfo) {
        let substitution = self.build_substitution(&struct_def.type_params, type_args);
        
        let fields: Vec<MonomorphizedField> = struct_def.fields.iter().map(|(name, ty, is_mutable)| {
            MonomorphizedField {
                name: name.clone(),
                ty: ty.substitute(&substitution),
                is_mutable: *is_mutable,
            }
        }).collect();
        
        let methods: Vec<MonomorphizedMethod> = struct_def.methods.iter().map(|m| {
            MonomorphizedMethod {
                name: m.name.clone(),
                param_types: m.param_types.iter().map(|t| t.substitute(&substitution)).collect(),
                param_names: m.param_names.clone(),
                return_type: m.return_type.substitute(&substitution),
                is_static: m.is_static,
            }
        }).collect();
        
        let mono_struct = MonomorphizedStruct {
            name: key.mangled_name(),
            original_name: key.base_name.clone(),
            substitution,
            fields,
            methods,
        };
        
        self.monomorphized_structs.insert(key.clone(), mono_struct);
    }
    
    /// 单态化函数
    fn monomorphize_function(&mut self, key: &MonoKey, type_args: &[Type], func_def: &FunctionDefInfo) {
        let substitution = self.build_substitution(&func_def.type_params, type_args);
        
        let mono_func = MonomorphizedFunction {
            name: key.mangled_name(),
            original_name: key.base_name.clone(),
            substitution: substitution.clone(),
            param_types: func_def.param_types.iter().map(|t| t.substitute(&substitution)).collect(),
            param_names: func_def.param_names.clone(),
            return_type: func_def.return_type.substitute(&substitution),
        };
        
        self.monomorphized_functions.insert(key.clone(), mono_func);
    }
    
    /// 构建类型替换表
    fn build_substitution(&self, type_params: &[GenericParam], type_args: &[Type]) -> Substitution {
        let mut subst = Substitution::new();
        
        for (param, arg) in type_params.iter().zip(type_args) {
            subst.insert(param.name.clone(), arg.clone());
        }
        
        // 处理默认类型参数
        for param in type_params.iter().skip(type_args.len()) {
            if let Some(default) = &param.default {
                subst.insert(param.name.clone(), default.as_ref().clone());
            }
        }
        
        subst
    }
    
    /// 获取单态化后的类
    pub fn get_monomorphized_class(&self, key: &MonoKey) -> Option<&MonomorphizedClass> {
        self.monomorphized_classes.get(key)
    }
    
    /// 获取单态化后的结构体
    pub fn get_monomorphized_struct(&self, key: &MonoKey) -> Option<&MonomorphizedStruct> {
        self.monomorphized_structs.get(key)
    }
    
    /// 获取单态化后的函数
    pub fn get_monomorphized_function(&self, key: &MonoKey) -> Option<&MonomorphizedFunction> {
        self.monomorphized_functions.get(key)
    }
    
    /// 获取所有单态化的类
    pub fn all_classes(&self) -> impl Iterator<Item = &MonomorphizedClass> {
        self.monomorphized_classes.values()
    }
    
    /// 获取所有单态化的结构体
    pub fn all_structs(&self) -> impl Iterator<Item = &MonomorphizedStruct> {
        self.monomorphized_structs.values()
    }
    
    /// 获取所有单态化的函数
    pub fn all_functions(&self) -> impl Iterator<Item = &MonomorphizedFunction> {
        self.monomorphized_functions.values()
    }
}

impl Default for Monomorphizer {
    fn default() -> Self {
        Self::new()
    }
}
