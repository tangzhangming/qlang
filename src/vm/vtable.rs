//! VTable（虚方法表）实现
//! 
//! 用于实现动态方法派发和 trait 对象

use std::collections::HashMap;
use std::sync::Arc;

/// 类型 ID（用于运行时类型识别）
pub type TypeId = u32;

/// 方法索引
pub type MethodIndex = usize;

/// VTable 条目
#[derive(Debug, Clone)]
pub struct VTableEntry {
    /// 方法名
    pub name: String,
    /// 方法在常量池中的索引
    pub func_index: usize,
}

/// 类型的 VTable
#[derive(Debug, Clone)]
pub struct VTable {
    /// 类型 ID
    pub type_id: TypeId,
    /// 类型名称
    pub type_name: String,
    /// 方法表（方法名 -> 函数索引）
    pub methods: HashMap<String, MethodIndex>,
    /// 有序的方法列表（用于虚方法调用）
    pub method_slots: Vec<VTableEntry>,
    /// 实现的 Trait（trait名 -> TraitVTable）
    pub trait_impls: HashMap<String, TraitVTable>,
    /// 父类 VTable（用于继承）
    pub parent: Option<Box<VTable>>,
}

impl VTable {
    /// 创建新的 VTable
    pub fn new(type_id: TypeId, type_name: impl Into<String>) -> Self {
        Self {
            type_id,
            type_name: type_name.into(),
            methods: HashMap::new(),
            method_slots: Vec::new(),
            trait_impls: HashMap::new(),
            parent: None,
        }
    }
    
    /// 创建带父类的 VTable
    pub fn with_parent(type_id: TypeId, type_name: impl Into<String>, parent: VTable) -> Self {
        let mut vtable = Self {
            type_id,
            type_name: type_name.into(),
            methods: parent.methods.clone(),
            method_slots: parent.method_slots.clone(),
            trait_impls: HashMap::new(),
            parent: Some(Box::new(parent)),
        };
        vtable
    }
    
    /// 注册方法
    pub fn register_method(&mut self, name: impl Into<String>, func_index: usize) {
        let name = name.into();
        
        // 如果方法已存在（覆盖），更新索引
        if let Some(&slot_index) = self.methods.get(&name) {
            self.method_slots[slot_index].func_index = func_index;
        } else {
            // 新方法，添加到末尾
            let slot_index = self.method_slots.len();
            self.method_slots.push(VTableEntry {
                name: name.clone(),
                func_index,
            });
            self.methods.insert(name, slot_index);
        }
    }
    
    /// 查找方法
    pub fn lookup_method(&self, name: &str) -> Option<MethodIndex> {
        self.methods.get(name).copied()
    }
    
    /// 获取方法的函数索引
    pub fn get_method_func_index(&self, name: &str) -> Option<usize> {
        if let Some(&slot_index) = self.methods.get(name) {
            Some(self.method_slots[slot_index].func_index)
        } else {
            None
        }
    }
    
    /// 获取方法槽的函数索引
    pub fn get_slot_func_index(&self, slot_index: MethodIndex) -> Option<usize> {
        self.method_slots.get(slot_index).map(|e| e.func_index)
    }
    
    /// 注册 Trait 实现
    pub fn register_trait_impl(&mut self, trait_name: impl Into<String>, trait_vtable: TraitVTable) {
        self.trait_impls.insert(trait_name.into(), trait_vtable);
    }
    
    /// 查找 Trait 实现
    pub fn lookup_trait(&self, trait_name: &str) -> Option<&TraitVTable> {
        self.trait_impls.get(trait_name)
    }
    
    /// 检查是否实现了 Trait
    pub fn implements_trait(&self, trait_name: &str) -> bool {
        self.trait_impls.contains_key(trait_name)
    }
    
    /// 获取父类的方法（用于 super 调用）
    pub fn get_parent_method(&self, name: &str) -> Option<usize> {
        self.parent.as_ref().and_then(|p| p.get_method_func_index(name))
    }
}

/// Trait 的 VTable
#[derive(Debug, Clone)]
pub struct TraitVTable {
    /// Trait 名称
    pub trait_name: String,
    /// 方法映射（trait 方法名 -> 实现方法的函数索引）
    pub methods: HashMap<String, usize>,
}

impl TraitVTable {
    /// 创建新的 TraitVTable
    pub fn new(trait_name: impl Into<String>) -> Self {
        Self {
            trait_name: trait_name.into(),
            methods: HashMap::new(),
        }
    }
    
    /// 注册方法实现
    pub fn register_method(&mut self, name: impl Into<String>, func_index: usize) {
        self.methods.insert(name.into(), func_index);
    }
    
    /// 查找方法实现
    pub fn lookup_method(&self, name: &str) -> Option<usize> {
        self.methods.get(name).copied()
    }
}

/// VTable 注册表（全局管理所有类型的 VTable）
#[derive(Debug, Default)]
pub struct VTableRegistry {
    /// 类型名到 VTable 的映射
    vtables: HashMap<String, Arc<VTable>>,
    /// 类型 ID 到 VTable 的映射（用于快速查找）
    vtables_by_id: HashMap<TypeId, Arc<VTable>>,
    /// 下一个类型 ID
    next_type_id: TypeId,
}

impl VTableRegistry {
    /// 创建新的注册表
    pub fn new() -> Self {
        Self {
            vtables: HashMap::new(),
            vtables_by_id: HashMap::new(),
            next_type_id: 1, // 0 保留给 null
        }
    }
    
    /// 分配类型 ID
    pub fn allocate_type_id(&mut self) -> TypeId {
        let id = self.next_type_id;
        self.next_type_id += 1;
        id
    }
    
    /// 注册 VTable
    pub fn register(&mut self, vtable: VTable) -> Arc<VTable> {
        let vtable = Arc::new(vtable);
        self.vtables.insert(vtable.type_name.clone(), Arc::clone(&vtable));
        self.vtables_by_id.insert(vtable.type_id, Arc::clone(&vtable));
        vtable
    }
    
    /// 通过名称查找 VTable
    pub fn lookup_by_name(&self, name: &str) -> Option<Arc<VTable>> {
        self.vtables.get(name).cloned()
    }
    
    /// 通过 ID 查找 VTable
    pub fn lookup_by_id(&self, id: TypeId) -> Option<Arc<VTable>> {
        self.vtables_by_id.get(&id).cloned()
    }
    
    /// 获取或创建 VTable
    pub fn get_or_create(&mut self, type_name: &str) -> Arc<VTable> {
        if let Some(vtable) = self.vtables.get(type_name) {
            Arc::clone(vtable)
        } else {
            let type_id = self.allocate_type_id();
            self.register(VTable::new(type_id, type_name))
        }
    }
    
    /// 创建带父类的 VTable
    pub fn create_with_parent(&mut self, type_name: &str, parent_name: &str) -> Option<Arc<VTable>> {
        let parent = self.lookup_by_name(parent_name)?;
        let type_id = self.allocate_type_id();
        let vtable = VTable::with_parent(type_id, type_name, (*parent).clone());
        Some(self.register(vtable))
    }
}

/// 运行时类型信息
#[derive(Debug, Clone)]
pub struct RuntimeTypeInfo {
    /// 类型 ID
    pub type_id: TypeId,
    /// 类型名称
    pub type_name: String,
    /// 字段信息
    pub fields: Vec<FieldInfo>,
    /// VTable 引用
    pub vtable: Option<Arc<VTable>>,
}

/// 字段信息
#[derive(Debug, Clone)]
pub struct FieldInfo {
    /// 字段名
    pub name: String,
    /// 字段索引
    pub index: usize,
    /// 是否可变
    pub is_mutable: bool,
}

impl RuntimeTypeInfo {
    /// 创建新的类型信息
    pub fn new(type_id: TypeId, type_name: impl Into<String>) -> Self {
        Self {
            type_id,
            type_name: type_name.into(),
            fields: Vec::new(),
            vtable: None,
        }
    }
    
    /// 添加字段
    pub fn add_field(&mut self, name: impl Into<String>, is_mutable: bool) -> usize {
        let index = self.fields.len();
        self.fields.push(FieldInfo {
            name: name.into(),
            index,
            is_mutable,
        });
        index
    }
    
    /// 查找字段索引
    pub fn get_field_index(&self, name: &str) -> Option<usize> {
        self.fields.iter()
            .find(|f| f.name == name)
            .map(|f| f.index)
    }
    
    /// 设置 VTable
    pub fn set_vtable(&mut self, vtable: Arc<VTable>) {
        self.vtable = Some(vtable);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_vtable_basic() {
        let mut vtable = VTable::new(1, "TestClass");
        vtable.register_method("foo", 10);
        vtable.register_method("bar", 20);
        
        assert_eq!(vtable.get_method_func_index("foo"), Some(10));
        assert_eq!(vtable.get_method_func_index("bar"), Some(20));
        assert_eq!(vtable.get_method_func_index("baz"), None);
    }
    
    #[test]
    fn test_vtable_override() {
        let mut parent = VTable::new(1, "Parent");
        parent.register_method("foo", 10);
        parent.register_method("bar", 20);
        
        let mut child = VTable::with_parent(2, "Child", parent);
        // 覆盖 foo 方法
        child.register_method("foo", 30);
        // 添加新方法
        child.register_method("baz", 40);
        
        assert_eq!(child.get_method_func_index("foo"), Some(30)); // 覆盖后
        assert_eq!(child.get_method_func_index("bar"), Some(20)); // 继承
        assert_eq!(child.get_method_func_index("baz"), Some(40)); // 新增
        
        // 可以获取父类方法
        assert_eq!(child.get_parent_method("foo"), Some(10));
    }
    
    #[test]
    fn test_trait_vtable() {
        let mut vtable = VTable::new(1, "MyClass");
        vtable.register_method("toString", 10);
        
        let mut trait_vtable = TraitVTable::new("Printable");
        trait_vtable.register_method("print", 10);
        
        vtable.register_trait_impl("Printable", trait_vtable);
        
        assert!(vtable.implements_trait("Printable"));
        assert!(!vtable.implements_trait("Comparable"));
        
        let printable = vtable.lookup_trait("Printable").unwrap();
        assert_eq!(printable.lookup_method("print"), Some(10));
    }
    
    #[test]
    fn test_registry() {
        let mut registry = VTableRegistry::new();
        
        let type_id = registry.allocate_type_id();
        let mut vtable = VTable::new(type_id, "MyClass");
        vtable.register_method("foo", 10);
        
        let vtable = registry.register(vtable);
        
        let found = registry.lookup_by_name("MyClass").unwrap();
        assert_eq!(found.type_id, type_id);
        assert_eq!(found.get_method_func_index("foo"), Some(10));
        
        let found_by_id = registry.lookup_by_id(type_id).unwrap();
        assert_eq!(found_by_id.type_name, "MyClass");
    }
}
