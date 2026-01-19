//! 运行时值定义 - NaN-Boxing 优化版本
//! 
//! 使用 NaN-Boxing 技术将 Value 压缩到 8 字节
//! 
//! 布局设计：
//! - 普通浮点数：直接存储 f64
//! - 特殊值使用 Quiet NaN 空间编码：
//!   - QNAN | 0x01 = Null
//!   - QNAN | 0x02 = False
//!   - QNAN | 0x03 = True
//!   - QNAN | 0x04_xxxx_xxxx = Int32 (低 32 位)
//!   - QNAN | 0x05_xxxx_xxxx_xxxx = Pointer (低 48 位)
//!   - QNAN | 0x06_xxxx_xxxx_xxxx = Int64 boxed pointer

#![allow(dead_code)]

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use parking_lot::Mutex;
use std::collections::HashMap;
use dashmap::DashMap;
use std::sync::OnceLock;

// ============================================================================
// GC 集成
// ============================================================================

/// GC 是否启用（默认禁用，以保持最佳性能）
static GC_ENABLED: AtomicBool = AtomicBool::new(false);

/// 启用 GC
#[inline]
pub fn enable_gc() {
    GC_ENABLED.store(true, Ordering::Release);
}

/// 禁用 GC
#[inline]
pub fn disable_gc() {
    GC_ENABLED.store(false, Ordering::Release);
}

/// 检查 GC 是否启用
#[inline]
pub fn is_gc_enabled() -> bool {
    GC_ENABLED.load(Ordering::Relaxed)
}

/// 注册堆对象到 GC（如果启用）
#[inline(always)]
fn gc_register_object(ptr: u64, tag: HeapTag, size: usize) {
    if GC_ENABLED.load(Ordering::Relaxed) {
        super::gc::gc_register(ptr, tag, size);
    }
}

// ============================================================================
// 字符串驻留池 (String Interning)
// ============================================================================

/// 全局字符串池
/// 使用 DashMap 实现线程安全的并发访问
static STRING_POOL: OnceLock<DashMap<String, u64>> = OnceLock::new();

/// 获取字符串池实例
#[inline]
fn get_string_pool() -> &'static DashMap<String, u64> {
    STRING_POOL.get_or_init(|| DashMap::with_capacity(1024))
}

/// 字符串驻留阈值（超过此长度的字符串不进行驻留）
const INTERN_THRESHOLD: usize = 64;

// ============================================================================
// NaN-Boxing 常量定义
// ============================================================================

/// Quiet NaN 掩码（IEEE 754: 指数全1 + 最高尾数位为1）
const QNAN: u64 = 0x7FFC_0000_0000_0000;

/// 符号位
const SIGN_BIT: u64 = 0x8000_0000_0000_0000;

/// 类型标签掩码
const TAG_MASK: u64 = 0x000F_0000_0000_0000;

/// Null 值
const VAL_NULL: u64 = QNAN | 0x0001_0000_0000_0000;

/// False 值
const VAL_FALSE: u64 = QNAN | 0x0002_0000_0000_0000;

/// True 值
const VAL_TRUE: u64 = QNAN | 0x0003_0000_0000_0000;

/// Int32 标签（低 32 位存储整数）
const TAG_INT32: u64 = QNAN | 0x0004_0000_0000_0000;

/// Pointer 标签（低 48 位存储指针）
const TAG_PTR: u64 = QNAN | 0x0005_0000_0000_0000;

/// Int64 标签（指向堆上的 i64）
const TAG_INT64: u64 = QNAN | 0x0006_0000_0000_0000;

/// Char 标签（低 32 位存储 char）
const TAG_CHAR: u64 = QNAN | 0x0007_0000_0000_0000;

/// 指针掩码（低 48 位）
const PTR_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

/// Int32 范围
const INT32_MIN: i64 = i32::MIN as i64;
const INT32_MAX: i64 = i32::MAX as i64;

// ============================================================================
// 堆对象类型定义
// ============================================================================

/// 堆对象类型标签
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HeapTag {
    String = 0,
    Function = 1,
    Array = 2,
    Map = 3,
    Range = 4,
    Iterator = 5,
    Struct = 6,
    Class = 7,
    Enum = 8,
    TypeRef = 9,
    Int64 = 10,
    Channel = 11,
    MutexValue = 12,
    WaitGroup = 13,
    Set = 14,
    ArraySlice = 15,
    RuntimeTypeInfo = 16,
}

/// 堆对象头部
#[repr(C)]
pub struct HeapObject {
    pub tag: HeapTag,
}

/// 堆上的字符串
#[repr(C)]
pub struct HeapString {
    pub header: HeapObject,
    pub data: String,
}

/// 堆上的 Int64
#[repr(C)]
pub struct HeapInt64 {
    pub header: HeapObject,
    pub value: i64,
}

/// 堆上的 Range
#[repr(C)]
pub struct HeapRange {
    pub header: HeapObject,
    pub start: i64,
    pub end: i64,
    pub inclusive: bool,
}

/// 函数对象
#[derive(Debug, Clone)]
pub struct Function {
    /// 函数名（闭包可能没有名字）
    pub name: Option<String>,
    /// 总参数数量（包括可变参数本身作为一个参数）
    pub arity: usize,
    /// 必需参数数量（无默认值的参数，不包括可变参数）
    pub required_params: usize,
    /// 默认值列表（按参数顺序，只包含有默认值的参数）
    pub defaults: Vec<Value>,
    /// 是否有可变参数
    pub has_variadic: bool,
    /// 函数体的字节码起始位置（在主 chunk 中）
    pub chunk_index: usize,
    /// 局部变量数量
    pub local_count: usize,
    /// Upvalue 描述符（闭包捕获的变量）
    pub upvalues: Vec<UpvalueDescriptor>,
}

/// Upvalue 描述符
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpvalueDescriptor {
    /// 在父作用域中的索引
    pub index: u16,
    /// true: 捕获的是局部变量; false: 捕获的是 upvalue
    pub is_local: bool,
}

impl PartialEq for Function {
    fn eq(&self, other: &Self) -> bool {
        self.chunk_index == other.chunk_index
    }
}

/// 堆上的函数
#[repr(C)]
pub struct HeapFunction {
    pub header: HeapObject,
    pub data: Arc<Function>,
}

/// 堆上的数组
#[repr(C)]
pub struct HeapArray {
    pub header: HeapObject,
    pub data: Arc<Mutex<Vec<Value>>>,
}

/// 堆上的数组切片（视图）
/// 
/// 不复制数据，只引用原数组的一个范围
#[repr(C)]
pub struct HeapArraySlice {
    pub header: HeapObject,
    /// 原数组
    pub source: Arc<Mutex<Vec<Value>>>,
    /// 起始索引（包含）
    pub start: usize,
    /// 结束索引（不包含）
    pub end: usize,
}

/// 堆上的 Map
#[repr(C)]
pub struct HeapMap {
    pub header: HeapObject,
    pub data: Arc<Mutex<HashMap<String, Value>>>,
}

/// 堆上的 Set（集合）
/// 
/// 使用 Vec<Value> 实现，通过 PartialEq 进行元素去重
/// 对于小集合效率足够，大集合考虑使用更高效的数据结构
#[repr(C)]
pub struct HeapSet {
    pub header: HeapObject,
    pub data: Arc<Mutex<Vec<Value>>>,
}

/// 迭代器数据源
#[derive(Debug, Clone)]
pub enum IteratorSource {
    Array(Arc<Mutex<Vec<Value>>>),
    Range(i64, i64, bool),
}

/// 迭代器对象
#[derive(Debug, Clone)]
pub struct Iterator {
    pub source: IteratorSource,
    pub index: usize,
}

impl PartialEq for Iterator {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}

/// 堆上的迭代器
#[repr(C)]
pub struct HeapIterator {
    pub header: HeapObject,
    pub data: Arc<Mutex<Iterator>>,
}

/// Struct 实例
#[derive(Debug, Clone)]
pub struct StructInstance {
    pub type_name: String,
    pub fields: HashMap<String, Value>,
}

impl PartialEq for StructInstance {
    fn eq(&self, other: &Self) -> bool {
        self.type_name == other.type_name && self.fields == other.fields
    }
}

/// 堆上的 Struct
#[repr(C)]
pub struct HeapStruct {
    pub header: HeapObject,
    pub data: Arc<Mutex<StructInstance>>,
}

/// Class 实例
#[derive(Debug, Clone)]
pub struct ClassInstance {
    pub class_name: String,
    pub parent_class: Option<String>,
    pub fields: HashMap<String, Value>,
}

impl PartialEq for ClassInstance {
    fn eq(&self, other: &Self) -> bool {
        self.class_name == other.class_name && self.fields == other.fields
    }
}

/// 堆上的 Class
#[repr(C)]
pub struct HeapClass {
    pub header: HeapObject,
    pub data: Arc<Mutex<ClassInstance>>,
}

/// Enum 变体值
#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariantValue {
    pub enum_name: String,
    pub variant_name: String,
    pub value: Option<Value>,  // 关联值（如 Ok = 200 中的 200）
    pub associated_data: HashMap<String, Value>,  // 关联数据字段
}

/// 堆上的 Enum
#[repr(C)]
pub struct HeapEnum {
    pub header: HeapObject,
    pub data: Box<EnumVariantValue>,
}

/// 堆上的类型引用
#[repr(C)]
pub struct HeapTypeRef {
    pub header: HeapObject,
    pub data: String,
}

// ============================================================================
// 运行时类型信息（反射支持）
// ============================================================================

/// 类型种类
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeKind {
    /// 原始类型（int, float, bool, char, string）
    Primitive,
    /// 数组类型
    Array,
    /// Map 类型
    Map,
    /// Set 类型
    Set,
    /// 函数类型
    Function,
    /// 结构体类型
    Struct,
    /// 类类型
    Class,
    /// 枚举类型
    Enum,
    /// 接口类型
    Interface,
    /// Trait 类型
    Trait,
    /// 可空类型
    Nullable,
    /// 泛型类型
    Generic,
    /// 类型别名
    Alias,
    /// 未知类型
    Unknown,
}

/// 字段信息（反射）
#[derive(Debug, Clone)]
pub struct FieldInfo {
    /// 字段名称
    pub name: String,
    /// 字段类型名称
    pub type_name: String,
    /// 是否是公开的
    pub is_public: bool,
    /// 是否是静态的
    pub is_static: bool,
    /// 是否是常量
    pub is_const: bool,
}

/// 方法信息（反射）
#[derive(Debug, Clone)]
pub struct MethodInfo {
    /// 方法名称
    pub name: String,
    /// 参数类型列表
    pub param_types: Vec<String>,
    /// 返回类型名称
    pub return_type: String,
    /// 是否是公开的
    pub is_public: bool,
    /// 是否是静态的
    pub is_static: bool,
    /// 是否是抽象的
    pub is_abstract: bool,
}

/// 运行时类型信息（反射支持）
#[derive(Debug, Clone)]
pub struct RuntimeTypeInfoData {
    /// 类型名称
    pub name: String,
    /// 类型种类
    pub kind: TypeKind,
    /// 父类型名称（对于 class）
    pub parent: Option<String>,
    /// 实现的接口/trait 列表
    pub implements: Vec<String>,
    /// 字段列表（对于 struct/class）
    pub fields: Vec<FieldInfo>,
    /// 方法列表（对于 struct/class/interface/trait）
    pub methods: Vec<MethodInfo>,
    /// 泛型参数名称（如果是泛型类型）
    pub type_params: Vec<String>,
    /// 元素类型（对于 Array、Set、Nullable 等）
    pub element_type: Option<Box<RuntimeTypeInfoData>>,
    /// 键类型（对于 Map）
    pub key_type: Option<Box<RuntimeTypeInfoData>>,
    /// 值类型（对于 Map）
    pub value_type: Option<Box<RuntimeTypeInfoData>>,
}

impl RuntimeTypeInfoData {
    /// 创建简单的原始类型信息
    pub fn primitive(name: &str) -> Self {
        Self {
            name: name.to_string(),
            kind: TypeKind::Primitive,
            parent: None,
            implements: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            type_params: Vec::new(),
            element_type: None,
            key_type: None,
            value_type: None,
        }
    }
    
    /// 创建数组类型信息
    pub fn array(element_type: RuntimeTypeInfoData) -> Self {
        Self {
            name: format!("{}[]", element_type.name),
            kind: TypeKind::Array,
            parent: None,
            implements: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            type_params: Vec::new(),
            element_type: Some(Box::new(element_type)),
            key_type: None,
            value_type: None,
        }
    }
    
    /// 创建未知类型信息
    pub fn unknown() -> Self {
        Self {
            name: "unknown".to_string(),
            kind: TypeKind::Unknown,
            parent: None,
            implements: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            type_params: Vec::new(),
            element_type: None,
            key_type: None,
            value_type: None,
        }
    }
}

/// 堆上的运行时类型信息
#[repr(C)]
pub struct HeapRuntimeTypeInfo {
    pub header: HeapObject,
    pub data: Box<RuntimeTypeInfoData>,
}

// ============================================================================
// 并发对象定义
// ============================================================================

/// Channel 内部状态
pub struct ChannelState {
    pub sender: Arc<Mutex<Option<crossbeam_channel::Sender<Value>>>>,
    pub receiver: Arc<Mutex<Option<crossbeam_channel::Receiver<Value>>>>,
    pub closed: Arc<AtomicBool>,
}

/// 堆上的 Channel
#[repr(C)]
pub struct HeapChannel {
    pub header: HeapObject,
    pub state: Arc<Mutex<ChannelState>>,
}

/// 堆上的 Mutex（封装一个 Value）
#[repr(C)]
pub struct HeapMutex {
    pub header: HeapObject,
    pub inner: Arc<Mutex<Value>>,
}

/// WaitGroup 内部状态（优化版本）
/// 
/// 使用无锁快速路径 + 自旋等待 + Condvar 等待的分层策略
pub struct WaitGroupState {
    /// 计数器（直接内联，减少间接访问）
    pub counter: AtomicUsize,
    /// 等待者计数（用于优化通知）
    pub waiters: AtomicUsize,
    /// 等待用的互斥锁
    pub mutex: Mutex<()>,
    /// 条件变量
    pub condvar: parking_lot::Condvar,
}

impl WaitGroupState {
    /// 创建新的 WaitGroup 状态
    #[inline]
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
            waiters: AtomicUsize::new(0),
            mutex: Mutex::new(()),
            condvar: parking_lot::Condvar::new(),
        }
    }
    
    /// 增加计数
    #[inline]
    pub fn add(&self, delta: isize) {
        if delta > 0 {
            self.counter.fetch_add(delta as usize, Ordering::SeqCst);
        } else if delta < 0 {
            let abs_delta = (-delta) as usize;
            let old = self.counter.fetch_sub(abs_delta, Ordering::SeqCst);
            // 如果计数器归零且有等待者，通知它们
            if old == abs_delta && self.waiters.load(Ordering::Acquire) > 0 {
                self.condvar.notify_all();
            }
        }
    }
    
    /// 完成一个任务（减少计数）
    #[inline]
    pub fn done(&self) {
        let old = self.counter.fetch_sub(1, Ordering::SeqCst);
        // 如果计数器归零且有等待者，通知它们
        if old == 1 && self.waiters.load(Ordering::Acquire) > 0 {
            self.condvar.notify_all();
        }
    }
    
    /// 等待计数器归零
    #[inline]
    pub fn wait(&self) {
        // 快速路径：无锁检查
        if self.counter.load(Ordering::Acquire) == 0 {
            return;
        }
        
        // 自旋等待：短暂自旋减少上下文切换
        const SPIN_LIMIT: usize = 40;
        for _ in 0..SPIN_LIMIT {
            if self.counter.load(Ordering::Acquire) == 0 {
                return;
            }
            std::hint::spin_loop();
        }
        
        // 慢速路径：注册为等待者并使用 Condvar
        self.waiters.fetch_add(1, Ordering::AcqRel);
        
        let mut guard = self.mutex.lock();
        while self.counter.load(Ordering::Acquire) > 0 {
            self.condvar.wait(&mut guard);
        }
        
        self.waiters.fetch_sub(1, Ordering::AcqRel);
    }
    
    /// 获取当前计数
    #[inline]
    pub fn count(&self) -> usize {
        self.counter.load(Ordering::Acquire)
    }
}

impl Default for WaitGroupState {
    fn default() -> Self {
        Self::new()
    }
}

/// 堆上的 WaitGroup
#[repr(C)]
pub struct HeapWaitGroup {
    pub header: HeapObject,
    pub state: Arc<WaitGroupState>,
}

// ============================================================================
// NaN-Boxed Value 实现
// ============================================================================

/// NaN-Boxed 值（8 字节）
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Value(u64);

impl Value {
    // ========== 位操作（用于栈顶缓存优化） ==========
    
    /// 将 Value 转换为原始 u64 位表示
    /// 
    /// 用于栈顶缓存优化，将 Value 存储在局部变量（CPU 寄存器）中
    #[inline(always)]
    pub const fn to_bits(self) -> u64 {
        self.0
    }
    
    /// 从原始 u64 位表示创建 Value
    /// 
    /// 用于从栈顶缓存恢复 Value
    #[inline(always)]
    pub const fn from_bits(bits: u64) -> Self {
        Value(bits)
    }
    
    // ========== 构造函数 ==========
    
    /// 创建 Null 值
    #[inline(always)]
    pub const fn null() -> Self {
        Value(VAL_NULL)
    }
    
    /// 创建布尔值
    #[inline(always)]
    pub const fn bool(b: bool) -> Self {
        Value(if b { VAL_TRUE } else { VAL_FALSE })
    }
    
    /// 创建整数值
    #[inline(always)]
    pub fn int(n: i64) -> Self {
        // 如果在 i32 范围内，直接内联
        if n >= INT32_MIN && n <= INT32_MAX {
            Value(TAG_INT32 | (n as u32 as u64))
        } else {
            // 否则装箱
            let boxed = Box::new(HeapInt64 {
                header: HeapObject { tag: HeapTag::Int64 },
                value: n,
            });
            let ptr = Box::into_raw(boxed) as u64;
            gc_register_object(ptr, HeapTag::Int64, std::mem::size_of::<HeapInt64>());
            Value(TAG_INT64 | (ptr & PTR_MASK))
        }
    }
    
    /// 创建浮点数值
    #[inline(always)]
    pub fn float(f: f64) -> Self {
        Value(f.to_bits())
    }
    
    /// 创建字符值
    #[inline(always)]
    pub fn char(c: char) -> Self {
        Value(TAG_CHAR | (c as u32 as u64))
    }
    
    /// 创建字符串值（带字符串驻留优化）
    /// 
    /// 短字符串（<=64字节）会被驻留到全局字符串池中，
    /// 相同内容的字符串只存储一份，提高内存效率和比较速度
    #[inline]
    pub fn string(s: String) -> Self {
        // 短字符串进行驻留
        if s.len() <= INTERN_THRESHOLD {
            let pool = get_string_pool();
            
            // 先检查是否已存在
            if let Some(ptr) = pool.get(&s) {
                return Value(TAG_PTR | (*ptr & PTR_MASK));
            }
            
            // 不存在则创建并插入
            let str_len = s.len();
            let boxed = Box::new(HeapString {
                header: HeapObject { tag: HeapTag::String },
                data: s.clone(),
            });
            let ptr = Box::into_raw(boxed) as u64;
            gc_register_object(ptr, HeapTag::String, std::mem::size_of::<HeapString>() + str_len);
            pool.insert(s, ptr);
            Value(TAG_PTR | (ptr & PTR_MASK))
        } else {
            // 长字符串不驻留
            let str_len = s.len();
            let boxed = Box::new(HeapString {
                header: HeapObject { tag: HeapTag::String },
                data: s,
            });
            let ptr = Box::into_raw(boxed) as u64;
            gc_register_object(ptr, HeapTag::String, std::mem::size_of::<HeapString>() + str_len);
            Value(TAG_PTR | (ptr & PTR_MASK))
        }
    }
    
    /// 创建字符串值（不进行驻留）
    /// 
    /// 用于确定不需要驻留的场景（如临时字符串拼接）
    #[inline]
    pub fn string_uninterned(s: String) -> Self {
        let str_len = s.len();
        let boxed = Box::new(HeapString {
            header: HeapObject { tag: HeapTag::String },
            data: s,
        });
        let ptr = Box::into_raw(boxed) as u64;
        gc_register_object(ptr, HeapTag::String, std::mem::size_of::<HeapString>() + str_len);
        Value(TAG_PTR | (ptr & PTR_MASK))
    }
    
    /// 创建函数值
    #[inline]
    pub fn function(f: Arc<Function>) -> Self {
        let boxed = Box::new(HeapFunction {
            header: HeapObject { tag: HeapTag::Function },
            data: f,
        });
        let ptr = Box::into_raw(boxed) as u64;
        gc_register_object(ptr, HeapTag::Function, std::mem::size_of::<HeapFunction>());
        Value(TAG_PTR | (ptr & PTR_MASK))
    }
    
    /// 创建数组值
    #[inline]
    pub fn array(arr: Arc<Mutex<Vec<Value>>>) -> Self {
        let boxed = Box::new(HeapArray {
            header: HeapObject { tag: HeapTag::Array },
            data: arr,
        });
        let ptr = Box::into_raw(boxed) as u64;
        gc_register_object(ptr, HeapTag::Array, std::mem::size_of::<HeapArray>());
        Value(TAG_PTR | (ptr & PTR_MASK))
    }
    
    /// 创建数组切片（视图）
    /// 
    /// 不复制数据，只创建一个指向原数组范围的视图
    #[inline]
    pub fn array_slice(source: Arc<Mutex<Vec<Value>>>, start: usize, end: usize) -> Self {
        let boxed = Box::new(HeapArraySlice {
            header: HeapObject { tag: HeapTag::ArraySlice },
            source,
            start,
            end,
        });
        let ptr = Box::into_raw(boxed) as u64;
        gc_register_object(ptr, HeapTag::ArraySlice, std::mem::size_of::<HeapArraySlice>());
        Value(TAG_PTR | (ptr & PTR_MASK))
    }
    
    /// 创建 Map 值
    #[inline]
    pub fn map(m: Arc<Mutex<HashMap<String, Value>>>) -> Self {
        let boxed = Box::new(HeapMap {
            header: HeapObject { tag: HeapTag::Map },
            data: m,
        });
        let ptr = Box::into_raw(boxed) as u64;
        gc_register_object(ptr, HeapTag::Map, std::mem::size_of::<HeapMap>());
        Value(TAG_PTR | (ptr & PTR_MASK))
    }
    
    /// 创建 Set 值
    #[inline]
    pub fn set(s: Arc<Mutex<Vec<Value>>>) -> Self {
        let boxed = Box::new(HeapSet {
            header: HeapObject { tag: HeapTag::Set },
            data: s,
        });
        let ptr = Box::into_raw(boxed) as u64;
        gc_register_object(ptr, HeapTag::Set, std::mem::size_of::<HeapSet>());
        Value(TAG_PTR | (ptr & PTR_MASK))
    }
    
    /// 创建空 Set
    #[inline]
    pub fn empty_set() -> Self {
        Self::set(Arc::new(Mutex::new(Vec::new())))
    }
    
    /// 创建 Range 值
    #[inline]
    pub fn range(start: i64, end: i64, inclusive: bool) -> Self {
        let boxed = Box::new(HeapRange {
            header: HeapObject { tag: HeapTag::Range },
            start,
            end,
            inclusive,
        });
        let ptr = Box::into_raw(boxed) as u64;
        gc_register_object(ptr, HeapTag::Range, std::mem::size_of::<HeapRange>());
        Value(TAG_PTR | (ptr & PTR_MASK))
    }
    
    /// 创建迭代器值
    #[inline]
    pub fn iterator(iter: Arc<Mutex<Iterator>>) -> Self {
        let boxed = Box::new(HeapIterator {
            header: HeapObject { tag: HeapTag::Iterator },
            data: iter,
        });
        let ptr = Box::into_raw(boxed) as u64;
        gc_register_object(ptr, HeapTag::Iterator, std::mem::size_of::<HeapIterator>());
        Value(TAG_PTR | (ptr & PTR_MASK))
    }
    
    /// 创建 Struct 值
    #[inline]
    pub fn struct_val(s: Arc<Mutex<StructInstance>>) -> Self {
        let boxed = Box::new(HeapStruct {
            header: HeapObject { tag: HeapTag::Struct },
            data: s,
        });
        let ptr = Box::into_raw(boxed) as u64;
        gc_register_object(ptr, HeapTag::Struct, std::mem::size_of::<HeapStruct>());
        Value(TAG_PTR | (ptr & PTR_MASK))
    }
    
    /// 创建 Class 值
    #[inline]
    pub fn class(c: Arc<Mutex<ClassInstance>>) -> Self {
        let boxed = Box::new(HeapClass {
            header: HeapObject { tag: HeapTag::Class },
            data: c,
        });
        let ptr = Box::into_raw(boxed) as u64;
        gc_register_object(ptr, HeapTag::Class, std::mem::size_of::<HeapClass>());
        Value(TAG_PTR | (ptr & PTR_MASK))
    }
    
    /// 创建 Enum 值
    #[inline]
    pub fn enum_val(e: Box<EnumVariantValue>) -> Self {
        let boxed = Box::new(HeapEnum {
            header: HeapObject { tag: HeapTag::Enum },
            data: e,
        });
        let ptr = Box::into_raw(boxed) as u64;
        gc_register_object(ptr, HeapTag::Enum, std::mem::size_of::<HeapEnum>());
        Value(TAG_PTR | (ptr & PTR_MASK))
    }
    
    /// 创建类型引用值
    #[inline]
    pub fn type_ref(name: String) -> Self {
        let name_len = name.len();
        let boxed = Box::new(HeapTypeRef {
            header: HeapObject { tag: HeapTag::TypeRef },
            data: name,
        });
        let ptr = Box::into_raw(boxed) as u64;
        gc_register_object(ptr, HeapTag::TypeRef, std::mem::size_of::<HeapTypeRef>() + name_len);
        Value(TAG_PTR | (ptr & PTR_MASK))
    }
    
    /// 创建运行时类型信息值
    #[inline]
    pub fn runtime_type_info(data: RuntimeTypeInfoData) -> Self {
        let boxed = Box::new(HeapRuntimeTypeInfo {
            header: HeapObject { tag: HeapTag::RuntimeTypeInfo },
            data: Box::new(data),
        });
        let ptr = Box::into_raw(boxed) as u64;
        gc_register_object(ptr, HeapTag::RuntimeTypeInfo, std::mem::size_of::<HeapRuntimeTypeInfo>());
        Value(TAG_PTR | (ptr & PTR_MASK))
    }
    
    /// 创建 Channel 值
    #[inline]
    pub fn channel(state: Arc<Mutex<ChannelState>>) -> Self {
        let boxed = Box::new(HeapChannel {
            header: HeapObject { tag: HeapTag::Channel },
            state,
        });
        let ptr = Box::into_raw(boxed) as u64;
        gc_register_object(ptr, HeapTag::Channel, std::mem::size_of::<HeapChannel>());
        Value(TAG_PTR | (ptr & PTR_MASK))
    }
    
    /// 创建 Mutex 值
    #[inline]
    pub fn mutex(inner: Arc<Mutex<Value>>) -> Self {
        let boxed = Box::new(HeapMutex {
            header: HeapObject { tag: HeapTag::MutexValue },
            inner,
        });
        let ptr = Box::into_raw(boxed) as u64;
        gc_register_object(ptr, HeapTag::MutexValue, std::mem::size_of::<HeapMutex>());
        Value(TAG_PTR | (ptr & PTR_MASK))
    }
    
    /// 创建 WaitGroup 值
    #[inline]
    pub fn waitgroup(state: Arc<WaitGroupState>) -> Self {
        let boxed = Box::new(HeapWaitGroup {
            header: HeapObject { tag: HeapTag::WaitGroup },
            state,
        });
        let ptr = Box::into_raw(boxed) as u64;
        gc_register_object(ptr, HeapTag::WaitGroup, std::mem::size_of::<HeapWaitGroup>());
        Value(TAG_PTR | (ptr & PTR_MASK))
    }
    
    // ========== 类型检查 ==========
    
    /// 是否是 Null
    #[inline(always)]
    pub fn is_null(&self) -> bool {
        self.0 == VAL_NULL
    }
    
    /// 是否是布尔值
    #[inline(always)]
    pub fn is_bool(&self) -> bool {
        self.0 == VAL_TRUE || self.0 == VAL_FALSE
    }
    
    /// 是否是整数
    #[inline(always)]
    pub fn is_int(&self) -> bool {
        (self.0 & (QNAN | TAG_MASK)) == TAG_INT32 || (self.0 & (QNAN | TAG_MASK)) == TAG_INT64
    }
    
    /// 是否是 Int32（内联整数）
    #[inline(always)]
    pub fn is_int32(&self) -> bool {
        (self.0 & (QNAN | TAG_MASK)) == TAG_INT32
    }
    
    /// 是否是浮点数
    #[inline(always)]
    pub fn is_float(&self) -> bool {
        // 不是 NaN-boxed 值就是浮点数
        (self.0 & QNAN) != QNAN
    }
    
    /// 是否是数字
    #[inline(always)]
    pub fn is_number(&self) -> bool {
        self.is_int() || self.is_float()
    }
    
    /// 是否是字符
    #[inline(always)]
    pub fn is_char(&self) -> bool {
        (self.0 & (QNAN | TAG_MASK)) == TAG_CHAR
    }
    
    /// 是否是指针类型
    #[inline(always)]
    fn is_ptr(&self) -> bool {
        (self.0 & (QNAN | TAG_MASK)) == TAG_PTR
    }
    
    /// 获取堆对象标签
    #[inline]
    pub fn heap_tag(&self) -> Option<HeapTag> {
        if self.is_ptr() {
            let ptr = (self.0 & PTR_MASK) as *const HeapObject;
            if !ptr.is_null() {
                unsafe { Some((*ptr).tag) }
            } else {
                None
            }
        } else {
            None
        }
    }
    
    /// 检查是否是 Int64 堆对象
    #[inline]
    fn is_boxed_int64(&self) -> bool {
        (self.0 & (QNAN | TAG_MASK)) == TAG_INT64
    }
    
    /// 检查是否是堆对象
    #[inline]
    pub fn is_heap_object(&self) -> bool {
        self.is_ptr() || self.is_boxed_int64()
    }
    
    /// 获取对象指针（用于 GC）
    #[inline]
    pub fn as_ptr(&self) -> u64 {
        if self.is_ptr() {
            self.0 & PTR_MASK
        } else if self.is_boxed_int64() {
            self.0 & PTR_MASK
        } else {
            0
        }
    }
    
    /// 是否是字符串
    #[inline]
    pub fn is_string(&self) -> bool {
        self.heap_tag() == Some(HeapTag::String)
    }
    
    /// 是否是函数
    #[inline]
    pub fn is_function(&self) -> bool {
        self.heap_tag() == Some(HeapTag::Function)
    }
    
    /// 是否是数组
    #[inline]
    pub fn is_array(&self) -> bool {
        self.heap_tag() == Some(HeapTag::Array)
    }
    
    /// 是否是 Map
    #[inline]
    pub fn is_map(&self) -> bool {
        self.heap_tag() == Some(HeapTag::Map)
    }
    
    /// 是否是 Range
    #[inline]
    pub fn is_range(&self) -> bool {
        self.heap_tag() == Some(HeapTag::Range)
    }
    
    /// 是否是迭代器
    #[inline]
    pub fn is_iterator(&self) -> bool {
        self.heap_tag() == Some(HeapTag::Iterator)
    }
    
    /// 是否是 Struct
    #[inline]
    pub fn is_struct(&self) -> bool {
        self.heap_tag() == Some(HeapTag::Struct)
    }
    
    /// 是否是 Class
    #[inline]
    pub fn is_class(&self) -> bool {
        self.heap_tag() == Some(HeapTag::Class)
    }
    
    /// 是否是 Enum
    #[inline]
    pub fn is_enum(&self) -> bool {
        self.heap_tag() == Some(HeapTag::Enum)
    }
    
    /// 是否是类型引用
    #[inline]
    pub fn is_type_ref(&self) -> bool {
        self.heap_tag() == Some(HeapTag::TypeRef)
    }
    
    /// 是否是运行时类型信息
    #[inline]
    pub fn is_runtime_type_info(&self) -> bool {
        self.heap_tag() == Some(HeapTag::RuntimeTypeInfo)
    }
    
    /// 是否是 Channel
    #[inline]
    pub fn is_channel(&self) -> bool {
        self.heap_tag() == Some(HeapTag::Channel)
    }
    
    /// 是否是 Mutex
    #[inline]
    pub fn is_mutex(&self) -> bool {
        self.heap_tag() == Some(HeapTag::MutexValue)
    }
    
    /// 是否是 WaitGroup
    #[inline]
    pub fn is_waitgroup(&self) -> bool {
        self.heap_tag() == Some(HeapTag::WaitGroup)
    }
    
    // ========== 值提取 ==========
    
    /// 获取布尔值
    #[inline(always)]
    pub fn as_bool(&self) -> Option<bool> {
        if self.0 == VAL_TRUE {
            Some(true)
        } else if self.0 == VAL_FALSE {
            Some(false)
        } else {
            None
        }
    }
    
    /// 获取整数值
    #[inline(always)]
    pub fn as_int(&self) -> Option<i64> {
        if (self.0 & (QNAN | TAG_MASK)) == TAG_INT32 {
            // 从低 32 位提取并符号扩展
            Some((self.0 as u32) as i32 as i64)
        } else if (self.0 & (QNAN | TAG_MASK)) == TAG_INT64 {
            let ptr = (self.0 & PTR_MASK) as *const HeapInt64;
            if !ptr.is_null() {
                unsafe { Some((*ptr).value) }
            } else {
                None
            }
        } else {
            None
        }
    }
    
    /// 获取浮点数值
    #[inline(always)]
    pub fn as_float(&self) -> Option<f64> {
        if self.is_float() {
            Some(f64::from_bits(self.0))
        } else {
            None
        }
    }
    
    /// 尝试转换为 f64
    #[inline]
    pub fn as_f64(&self) -> Option<f64> {
        if let Some(f) = self.as_float() {
            Some(f)
        } else if let Some(i) = self.as_int() {
            Some(i as f64)
        } else {
            None
        }
    }
    
    /// 尝试转换为 i64
    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        if let Some(i) = self.as_int() {
            Some(i)
        } else if let Some(f) = self.as_float() {
            Some(f as i64)
        } else if let Some(c) = self.as_char() {
            Some(c as i64)
        } else {
            None
        }
    }
    
    /// 获取字符值
    #[inline(always)]
    pub fn as_char(&self) -> Option<char> {
        if (self.0 & (QNAN | TAG_MASK)) == TAG_CHAR {
            char::from_u32(self.0 as u32)
                } else {
            None
        }
    }
    
    /// 获取字符串引用
    #[inline]
    pub fn as_string(&self) -> Option<&String> {
        if self.heap_tag() == Some(HeapTag::String) {
            let ptr = (self.0 & PTR_MASK) as *const HeapString;
            unsafe { Some(&(*ptr).data) }
                } else {
            None
        }
    }
    
    /// 获取函数引用
    #[inline]
    pub fn as_function(&self) -> Option<&Arc<Function>> {
        if self.heap_tag() == Some(HeapTag::Function) {
            let ptr = (self.0 & PTR_MASK) as *const HeapFunction;
            unsafe { Some(&(*ptr).data) }
                } else {
            None
        }
    }
    
    /// 获取数组引用
    #[inline]
    pub fn as_array(&self) -> Option<&Arc<Mutex<Vec<Value>>>> {
        if self.heap_tag() == Some(HeapTag::Array) {
            let ptr = (self.0 & PTR_MASK) as *const HeapArray;
            unsafe { Some(&(*ptr).data) }
                } else {
            None
        }
    }
    
    /// 获取数组切片引用
    #[inline]
    pub fn as_array_slice(&self) -> Option<(&Arc<Mutex<Vec<Value>>>, usize, usize)> {
        if self.heap_tag() == Some(HeapTag::ArraySlice) {
            let ptr = (self.0 & PTR_MASK) as *const HeapArraySlice;
            unsafe { Some((&(*ptr).source, (*ptr).start, (*ptr).end)) }
        } else {
            None
        }
    }
    
    /// 检查是否是数组或数组切片
    #[inline]
    pub fn is_array_like(&self) -> bool {
        matches!(self.heap_tag(), Some(HeapTag::Array) | Some(HeapTag::ArraySlice))
    }
    
    /// 获取数组元素（支持数组和切片）
    /// 
    /// 对于切片，索引是相对于切片起始位置的
    pub fn array_get(&self, index: usize) -> Option<Value> {
        if let Some(arr) = self.as_array() {
            let arr = arr.lock();
            arr.get(index).cloned()
        } else if let Some((source, start, end)) = self.as_array_slice() {
            let arr = source.lock();
            let actual_index = start + index;
            if actual_index < end && actual_index < arr.len() {
                arr.get(actual_index).cloned()
            } else {
                None
            }
        } else {
            None
        }
    }
    
    /// 获取数组长度（支持数组和切片）
    pub fn array_len(&self) -> Option<usize> {
        if let Some(arr) = self.as_array() {
            Some(arr.lock().len())
        } else if let Some((_, start, end)) = self.as_array_slice() {
            Some(end - start)
        } else {
            None
        }
    }
    
    /// COW 写入数组元素
    /// 
    /// 如果数组被共享（引用计数 > 1），先复制再写入
    /// 返回是否成功写入
    pub fn array_set_cow(&mut self, index: usize, value: Value) -> bool {
        if let Some(arr) = self.as_array() {
            // 检查是否需要 COW
            if Arc::strong_count(arr) > 1 {
                // 需要复制
                let cloned = {
                    let guard = arr.lock();
                    guard.clone()
                };
                let new_arr = Arc::new(Mutex::new(cloned));
                
                // 写入新数组
                {
                    let mut guard = new_arr.lock();
                    if index < guard.len() {
                        guard[index] = value;
                    } else {
                        return false;
                    }
                }
                
                // 替换为新数组
                *self = Value::array(new_arr);
                true
            } else {
                // 直接写入
                let mut guard = arr.lock();
                if index < guard.len() {
                    guard[index] = value;
                    true
                } else {
                    false
                }
            }
        } else if let Some((source, start, end)) = self.as_array_slice() {
            // 切片写入：总是需要创建新数组（因为切片是只读视图）
            let actual_index = start + index;
            if actual_index >= end {
                return false;
            }
            
            // 将切片转换为独立数组
            let cloned = {
                let guard = source.lock();
                guard[start..end].to_vec()
            };
            let new_arr = Arc::new(Mutex::new(cloned));
            
            // 写入
            {
                let mut guard = new_arr.lock();
                if index < guard.len() {
                    guard[index] = value;
                } else {
                    return false;
                }
            }
            
            // 替换为新数组
            *self = Value::array(new_arr);
            true
        } else {
            false
        }
    }
    
    /// 获取 Map 引用
    #[inline]
    pub fn as_map(&self) -> Option<&Arc<Mutex<HashMap<String, Value>>>> {
        if self.heap_tag() == Some(HeapTag::Map) {
            let ptr = (self.0 & PTR_MASK) as *const HeapMap;
            unsafe { Some(&(*ptr).data) }
        } else {
            None
        }
    }
    
    /// 获取 Set 值
    #[inline]
    pub fn as_set(&self) -> Option<&Arc<Mutex<Vec<Value>>>> {
        if self.heap_tag() == Some(HeapTag::Set) {
            let ptr = (self.0 & PTR_MASK) as *const HeapSet;
            unsafe { Some(&(*ptr).data) }
        } else {
            None
        }
    }
    
    /// 获取 Range 值
    #[inline]
    pub fn as_range(&self) -> Option<(i64, i64, bool)> {
        if self.heap_tag() == Some(HeapTag::Range) {
            let ptr = (self.0 & PTR_MASK) as *const HeapRange;
            unsafe { Some(((*ptr).start, (*ptr).end, (*ptr).inclusive)) }
        } else {
            None
        }
    }
    
    /// 获取迭代器引用
    #[inline]
    pub fn as_iterator(&self) -> Option<&Arc<Mutex<Iterator>>> {
        if self.heap_tag() == Some(HeapTag::Iterator) {
            let ptr = (self.0 & PTR_MASK) as *const HeapIterator;
            unsafe { Some(&(*ptr).data) }
        } else {
            None
        }
    }
    
    /// 获取 Struct 引用
    #[inline]
    pub fn as_struct(&self) -> Option<&Arc<Mutex<StructInstance>>> {
        if self.heap_tag() == Some(HeapTag::Struct) {
            let ptr = (self.0 & PTR_MASK) as *const HeapStruct;
            unsafe { Some(&(*ptr).data) }
                } else {
            None
        }
    }
    
    /// 获取 Class 引用
    #[inline]
    pub fn as_class(&self) -> Option<&Arc<Mutex<ClassInstance>>> {
        if self.heap_tag() == Some(HeapTag::Class) {
            let ptr = (self.0 & PTR_MASK) as *const HeapClass;
            unsafe { Some(&(*ptr).data) }
        } else {
            None
        }
    }
    
    /// 获取 Enum 引用
    #[inline]
    pub fn as_enum(&self) -> Option<&EnumVariantValue> {
        if self.heap_tag() == Some(HeapTag::Enum) {
            let ptr = (self.0 & PTR_MASK) as *const HeapEnum;
            unsafe { Some(&(*ptr).data) }
        } else {
            None
        }
    }
    
    /// 获取类型引用
    #[inline]
    pub fn as_type_ref(&self) -> Option<&String> {
        if self.heap_tag() == Some(HeapTag::TypeRef) {
            let ptr = (self.0 & PTR_MASK) as *const HeapTypeRef;
            unsafe { Some(&(*ptr).data) }
        } else {
            None
        }
    }
    
    /// 获取运行时类型信息引用
    #[inline]
    pub fn as_runtime_type_info(&self) -> Option<&RuntimeTypeInfoData> {
        if self.heap_tag() == Some(HeapTag::RuntimeTypeInfo) {
            let ptr = (self.0 & PTR_MASK) as *const HeapRuntimeTypeInfo;
            unsafe { Some(&(*ptr).data) }
        } else {
            None
        }
    }
    
    /// 获取 Channel 引用
    #[inline]
    pub fn as_channel(&self) -> Option<&Arc<Mutex<ChannelState>>> {
        if self.heap_tag() == Some(HeapTag::Channel) {
            let ptr = (self.0 & PTR_MASK) as *const HeapChannel;
            unsafe { Some(&(*ptr).state) }
        } else {
            None
        }
    }
    
    /// 获取 Mutex 引用
    #[inline]
    pub fn as_mutex(&self) -> Option<&Arc<Mutex<Value>>> {
        if self.heap_tag() == Some(HeapTag::MutexValue) {
            let ptr = (self.0 & PTR_MASK) as *const HeapMutex;
            unsafe { Some(&(*ptr).inner) }
        } else {
            None
        }
    }
    
    /// 获取 WaitGroup 引用
    #[inline]
    pub fn as_waitgroup(&self) -> Option<&Arc<WaitGroupState>> {
        if self.heap_tag() == Some(HeapTag::WaitGroup) {
            let ptr = (self.0 & PTR_MASK) as *const HeapWaitGroup;
            unsafe { Some(&(*ptr).state) }
        } else {
            None
        }
    }
    
    // ========== 工具方法 ==========
    
    /// 判断是否为真值
    #[inline]
    pub fn is_truthy(&self) -> bool {
        if self.0 == VAL_NULL || self.0 == VAL_FALSE {
            return false;
        }
        if self.0 == VAL_TRUE {
            return true;
        }
        if let Some(n) = self.as_int() {
            return n != 0;
        }
        if let Some(f) = self.as_float() {
            return f != 0.0;
        }
        if let Some(c) = self.as_char() {
            return c != '\0';
        }
        if let Some(s) = self.as_string() {
            return !s.is_empty();
        }
        if let Some(arr) = self.as_array() {
            return !arr.lock().is_empty();
        }
        if let Some(m) = self.as_map() {
            return !m.lock().is_empty();
        }
        true
    }
    
    /// 获取类型名称
    #[inline]
    pub fn type_name(&self) -> &'static str {
        if self.is_null() { return "null"; }
        if self.is_bool() { return "bool"; }
        if self.is_int() { return "int"; }
        if self.is_float() { return "float"; }
        if self.is_char() { return "char"; }
        match self.heap_tag() {
            Some(HeapTag::String) => "string",
            Some(HeapTag::Function) => "function",
            Some(HeapTag::Array) => "array",
            Some(HeapTag::ArraySlice) => "array",  // 切片对外也显示为 array
            Some(HeapTag::Map) => "map",
            Some(HeapTag::Set) => "set",
            Some(HeapTag::Range) => "range",
            Some(HeapTag::Iterator) => "iterator",
            Some(HeapTag::Struct) => "struct",
            Some(HeapTag::Class) => "class",
            Some(HeapTag::Enum) => "enum",
            Some(HeapTag::TypeRef) => "type",
            Some(HeapTag::Int64) => "int",
            Some(HeapTag::Channel) => "channel",
            Some(HeapTag::MutexValue) => "mutex",
            Some(HeapTag::WaitGroup) => "waitgroup",
            Some(HeapTag::RuntimeTypeInfo) => "Type",
            None => "unknown",
        }
    }
    
    // ========== 运算方法 ==========

    /// 比较相等
    pub fn eq_value(&self, other: &Self) -> Value {
        Value::bool(self == other)
    }

    /// 比较不等
    pub fn ne_value(&self, other: &Self) -> Value {
        Value::bool(self != other)
    }
    
    /// 逻辑非
    pub fn not(&self) -> Value {
        Value::bool(!self.is_truthy())
    }

    /// 小于
    pub fn lt(&self, other: &Self) -> Result<Value, String> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => return Ok(Value::bool(a < b)),
            _ => {}
        }
        match (self.as_f64(), other.as_f64()) {
            (Some(a), Some(b)) => return Ok(Value::bool(a < b)),
            _ => {}
        }
        match (self.as_string(), other.as_string()) {
            (Some(a), Some(b)) => return Ok(Value::bool(a < b)),
            _ => {}
        }
        Err(format!("Cannot compare {} and {}", self.type_name(), other.type_name()))
    }

    /// 小于等于
    pub fn le(&self, other: &Self) -> Result<Value, String> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => return Ok(Value::bool(a <= b)),
            _ => {}
        }
        match (self.as_f64(), other.as_f64()) {
            (Some(a), Some(b)) => return Ok(Value::bool(a <= b)),
            _ => {}
        }
        match (self.as_string(), other.as_string()) {
            (Some(a), Some(b)) => return Ok(Value::bool(a <= b)),
            _ => {}
        }
        Err(format!("Cannot compare {} and {}", self.type_name(), other.type_name()))
    }

    /// 大于
    pub fn gt(&self, other: &Self) -> Result<Value, String> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => return Ok(Value::bool(a > b)),
            _ => {}
        }
        match (self.as_f64(), other.as_f64()) {
            (Some(a), Some(b)) => return Ok(Value::bool(a > b)),
            _ => {}
        }
        match (self.as_string(), other.as_string()) {
            (Some(a), Some(b)) => return Ok(Value::bool(a > b)),
            _ => {}
        }
        Err(format!("Cannot compare {} and {}", self.type_name(), other.type_name()))
    }

    /// 大于等于
    pub fn ge(&self, other: &Self) -> Result<Value, String> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => return Ok(Value::bool(a >= b)),
            _ => {}
        }
        match (self.as_f64(), other.as_f64()) {
            (Some(a), Some(b)) => return Ok(Value::bool(a >= b)),
            _ => {}
        }
        match (self.as_string(), other.as_string()) {
            (Some(a), Some(b)) => return Ok(Value::bool(a >= b)),
            _ => {}
        }
        Err(format!("Cannot compare {} and {}", self.type_name(), other.type_name()))
    }
    
    /// 幂运算
    pub fn pow(self, rhs: Self) -> Result<Value, String> {
        match (self.as_int(), rhs.as_int()) {
            (Some(a), Some(b)) => {
                if b >= 0 {
                    return Ok(Value::int(a.pow(b as u32)));
                } else {
                    return Ok(Value::float((a as f64).powf(b as f64)));
                }
            }
            _ => {}
        }
        match (self.as_f64(), rhs.as_f64()) {
            (Some(a), Some(b)) => return Ok(Value::float(a.powf(b))),
            _ => {}
        }
        Err(format!("Cannot compute power of {} and {}", self.type_name(), rhs.type_name()))
    }
    
    /// 按位与
    pub fn bit_and(&self, other: &Self) -> Result<Value, String> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => Ok(Value::int(a & b)),
            _ => Err(format!("Cannot bitwise AND {} and {}", self.type_name(), other.type_name())),
        }
    }
    
    /// 按位或
    pub fn bit_or(&self, other: &Self) -> Result<Value, String> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => Ok(Value::int(a | b)),
            _ => Err(format!("Cannot bitwise OR {} and {}", self.type_name(), other.type_name())),
        }
    }
    
    /// 按位异或
    pub fn bit_xor(&self, other: &Self) -> Result<Value, String> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => Ok(Value::int(a ^ b)),
            _ => Err(format!("Cannot bitwise XOR {} and {}", self.type_name(), other.type_name())),
        }
    }
    
    /// 按位取反
    pub fn bit_not(&self) -> Result<Value, String> {
        match self.as_int() {
            Some(n) => Ok(Value::int(!n)),
            None => Err(format!("Cannot bitwise NOT {}", self.type_name())),
        }
    }
    
    /// 左移
    pub fn shl(&self, other: &Self) -> Result<Value, String> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => {
                if b < 0 || b > 63 {
                    Err("Shift amount out of range".to_string())
                } else {
                    Ok(Value::int(a << b))
                }
            }
            _ => Err(format!("Cannot left shift {} by {}", self.type_name(), other.type_name())),
        }
    }
    
    /// 右移
    pub fn shr(&self, other: &Self) -> Result<Value, String> {
        match (self.as_int(), other.as_int()) {
            (Some(a), Some(b)) => {
                if b < 0 || b > 63 {
                    Err("Shift amount out of range".to_string())
                } else {
                    Ok(Value::int(a >> b))
                }
            }
            _ => Err(format!("Cannot right shift {} by {}", self.type_name(), other.type_name())),
        }
    }
}

// ============================================================================
// 运算符实现
// ============================================================================

impl std::ops::Add for Value {
    type Output = Result<Value, String>;
    
    fn add(self, rhs: Self) -> Self::Output {
        // 整数快速路径
        if let (Some(a), Some(b)) = (self.as_int(), rhs.as_int()) {
            return Ok(Value::int(a + b));
        }
        // 浮点数路径
        if let (Some(a), Some(b)) = (self.as_f64(), rhs.as_f64()) {
            return Ok(Value::float(a + b));
        }
        // 字符串连接
        if let (Some(a), Some(b)) = (self.as_string(), rhs.as_string()) {
            return Ok(Value::string(format!("{}{}", a, b)));
        }
        Err(format!("Cannot add {} and {}", self.type_name(), rhs.type_name()))
    }
}

impl std::ops::Sub for Value {
    type Output = Result<Value, String>;
    
    fn sub(self, rhs: Self) -> Self::Output {
        if let (Some(a), Some(b)) = (self.as_int(), rhs.as_int()) {
            return Ok(Value::int(a - b));
        }
        if let (Some(a), Some(b)) = (self.as_f64(), rhs.as_f64()) {
            return Ok(Value::float(a - b));
        }
        Err(format!("Cannot subtract {} from {}", rhs.type_name(), self.type_name()))
    }
}

impl std::ops::Mul for Value {
    type Output = Result<Value, String>;
    
    fn mul(self, rhs: Self) -> Self::Output {
        if let (Some(a), Some(b)) = (self.as_int(), rhs.as_int()) {
            return Ok(Value::int(a * b));
        }
        if let (Some(a), Some(b)) = (self.as_f64(), rhs.as_f64()) {
            return Ok(Value::float(a * b));
        }
        Err(format!("Cannot multiply {} and {}", self.type_name(), rhs.type_name()))
    }
}

impl std::ops::Div for Value {
    type Output = Result<Value, String>;
    
    fn div(self, rhs: Self) -> Self::Output {
        if let (Some(a), Some(b)) = (self.as_int(), rhs.as_int()) {
            if b == 0 {
                return Err("Division by zero".to_string());
            }
            return Ok(Value::int(a / b));
        }
        if let (Some(a), Some(b)) = (self.as_f64(), rhs.as_f64()) {
            if b == 0.0 {
                return Err("Division by zero".to_string());
            }
            return Ok(Value::float(a / b));
        }
        Err(format!("Cannot divide {} by {}", self.type_name(), rhs.type_name()))
    }
}

impl std::ops::Rem for Value {
    type Output = Result<Value, String>;
    
    fn rem(self, rhs: Self) -> Self::Output {
        if let (Some(a), Some(b)) = (self.as_int(), rhs.as_int()) {
            if b == 0 {
                return Err("Division by zero".to_string());
            }
            return Ok(Value::int(a % b));
        }
        if let (Some(a), Some(b)) = (self.as_f64(), rhs.as_f64()) {
            if b == 0.0 {
                return Err("Division by zero".to_string());
            }
            return Ok(Value::float(a % b));
        }
        Err(format!("Cannot compute modulo of {} and {}", self.type_name(), rhs.type_name()))
    }
}

impl std::ops::Neg for Value {
    type Output = Result<Value, String>;
    
    fn neg(self) -> Self::Output {
        if let Some(n) = self.as_int() {
            return Ok(Value::int(-n));
        }
        if let Some(f) = self.as_float() {
            return Ok(Value::float(-f));
        }
        Err(format!("Cannot negate {}", self.type_name()))
    }
}

// ============================================================================
// PartialEq 实现
// ============================================================================

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        // 快速路径：位完全相同
        if self.0 == other.0 {
            return true;
        }
        
        // 整数比较
        if let (Some(a), Some(b)) = (self.as_int(), other.as_int()) {
            return a == b;
        }
        
        // 浮点数比较
        if let (Some(a), Some(b)) = (self.as_float(), other.as_float()) {
            return a == b;
        }
        
        // 混合数字比较
        if let (Some(a), Some(b)) = (self.as_f64(), other.as_f64()) {
            return a == b;
        }
        
        // 字符串比较
        if let (Some(a), Some(b)) = (self.as_string(), other.as_string()) {
            return a == b;
        }
        
        // 数组比较
        if let (Some(a), Some(b)) = (self.as_array(), other.as_array()) {
            return *a.lock() == *b.lock();
        }
        
        // Map 比较
        if let (Some(a), Some(b)) = (self.as_map(), other.as_map()) {
            return *a.lock() == *b.lock();
        }
        
        // Struct 比较
        if let (Some(a), Some(b)) = (self.as_struct(), other.as_struct()) {
            return *a.lock() == *b.lock();
        }
        
        // Class 比较
        if let (Some(a), Some(b)) = (self.as_class(), other.as_class()) {
            return *a.lock() == *b.lock();
        }
        
        // Enum 比较
        if let (Some(a), Some(b)) = (self.as_enum(), other.as_enum()) {
            return a == b;
        }
        
        false
    }
}

// ============================================================================
// Debug 和 Display 实现
// ============================================================================

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_null() {
            write!(f, "Null")
        } else if let Some(b) = self.as_bool() {
            write!(f, "Bool({})", b)
        } else if let Some(n) = self.as_int() {
            write!(f, "Int({})", n)
        } else if let Some(n) = self.as_float() {
            write!(f, "Float({})", n)
        } else if let Some(c) = self.as_char() {
            write!(f, "Char({:?})", c)
        } else if let Some(s) = self.as_string() {
            write!(f, "String({:?})", s)
        } else if self.is_function() {
            write!(f, "Function(...)")
        } else if self.is_array() {
            write!(f, "Array(...)")
        } else if self.is_map() {
            write!(f, "Map(...)")
        } else if let Some((start, end, inc)) = self.as_range() {
            write!(f, "Range({}, {}, {})", start, end, inc)
        } else if self.is_iterator() {
            write!(f, "Iterator(...)")
        } else if self.is_struct() {
            write!(f, "Struct(...)")
        } else if self.is_class() {
            write!(f, "Class(...)")
        } else if let Some(e) = self.as_enum() {
            write!(f, "Enum({:?})", e)
        } else if let Some(t) = self.as_type_ref() {
            write!(f, "TypeRef({})", t)
        } else if self.is_channel() {
            write!(f, "Channel(...)")
        } else if self.is_mutex() {
            write!(f, "Mutex(...)")
        } else if self.is_waitgroup() {
            write!(f, "WaitGroup(...)")
        } else {
            write!(f, "Unknown(0x{:016X})", self.0)
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_null() {
            write!(f, "null")
        } else if let Some(b) = self.as_bool() {
            write!(f, "{}", b)
        } else if let Some(n) = self.as_int() {
            write!(f, "{}", n)
        } else if let Some(n) = self.as_float() {
            if n.fract() == 0.0 {
                write!(f, "{}.0", n)
            } else {
                write!(f, "{}", n)
            }
        } else if let Some(c) = self.as_char() {
            write!(f, "{}", c)
        } else if let Some(s) = self.as_string() {
            write!(f, "{}", s)
        } else if let Some(func) = self.as_function() {
            if let Some(name) = &func.name {
                write!(f, "<fn {}>", name)
            } else {
                write!(f, "<closure>")
            }
        } else if let Some(arr) = self.as_array() {
            let arr = arr.lock();
            write!(f, "[")?;
            for (i, v) in arr.iter().enumerate() {
                if i > 0 { write!(f, ", ")?; }
                write!(f, "{}", v)?;
            }
            write!(f, "]")
        } else if let Some((source, start, end)) = self.as_array_slice() {
            // 数组切片：显示切片范围内的元素
            let arr = source.lock();
            write!(f, "[")?;
            for (i, idx) in (start..end.min(arr.len())).enumerate() {
                if i > 0 { write!(f, ", ")?; }
                write!(f, "{}", arr[idx])?;
            }
            write!(f, "]")
        } else if let Some(m) = self.as_map() {
            let m = m.lock();
            write!(f, "{{")?;
            let mut first = true;
            for (k, v) in m.iter() {
                if !first { write!(f, ", ")?; }
                first = false;
                write!(f, "\"{}\": {}", k, v)?;
            }
            write!(f, "}}")
        } else if let Some(s) = self.as_set() {
            let s = s.lock();
            write!(f, "set{{")?;
            for (i, v) in s.iter().enumerate() {
                if i > 0 { write!(f, ", ")?; }
                write!(f, "{}", v)?;
            }
            write!(f, "}}")
        } else if let Some((start, end, inclusive)) = self.as_range() {
            if inclusive {
                write!(f, "{}..={}", start, end)
            } else {
                write!(f, "{}..{}", start, end)
            }
        } else if self.is_iterator() {
            write!(f, "<iterator>")
        } else if let Some(s) = self.as_struct() {
            let s = s.lock();
            write!(f, "{} {{ ", s.type_name)?;
            let mut first = true;
            for (name, value) in &s.fields {
                if !first { write!(f, ", ")?; }
                first = false;
                write!(f, "{}: {}", name, value)?;
            }
            write!(f, " }}")
        } else if let Some(c) = self.as_class() {
            let c = c.lock();
            write!(f, "{} {{ ", c.class_name)?;
            let mut first = true;
            for (name, value) in &c.fields {
                if !first { write!(f, ", ")?; }
                first = false;
                write!(f, "{}: {}", name, value)?;
            }
            write!(f, " }}")
        } else if let Some(e) = self.as_enum() {
            if e.associated_data.is_empty() {
                write!(f, "{}::{}", e.enum_name, e.variant_name)
            } else {
                write!(f, "{}::{}(", e.enum_name, e.variant_name)?;
                let mut first = true;
                for (name, value) in &e.associated_data {
                    if !first { write!(f, ", ")?; }
                    first = false;
                    write!(f, "{}: {}", name, value)?;
                }
                write!(f, ")")
            }
        } else if let Some(name) = self.as_type_ref() {
            write!(f, "<type {}>", name)
        } else if let Some(ti) = self.as_runtime_type_info() {
            // 运行时类型信息：显示详细的类型信息
            write!(f, "Type(name={}, kind={:?}", ti.name, ti.kind)?;
            if let Some(ref parent) = ti.parent {
                write!(f, ", parent={}", parent)?;
            }
            if !ti.implements.is_empty() {
                write!(f, ", implements=[{}]", ti.implements.join(", "))?;
            }
            if !ti.fields.is_empty() {
                write!(f, ", fields=[")?;
                for (i, field) in ti.fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", field.name, field.type_name)?;
                }
                write!(f, "]")?;
            }
            if !ti.methods.is_empty() {
                write!(f, ", methods=[")?;
                for (i, method) in ti.methods.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}()", method.name)?;
                }
                write!(f, "]")?;
            }
            if let Some(ref elem) = ti.element_type {
                write!(f, ", element_type={}", elem.name)?;
            }
            if let Some(ref key) = ti.key_type {
                write!(f, ", key_type={}", key.name)?;
            }
            if let Some(ref val) = ti.value_type {
                write!(f, ", value_type={}", val.name)?;
            }
            write!(f, ")")
        } else if self.is_channel() {
            write!(f, "<channel>")
        } else if self.is_mutex() {
            write!(f, "<mutex>")
        } else if self.is_waitgroup() {
            write!(f, "<waitgroup>")
        } else {
            write!(f, "<unknown>")
        }
    }
}

impl Default for Value {
    fn default() -> Self {
        Value::null()
    }
}

// ============================================================================
// 辅助宏：简化 Value 构造
// ============================================================================

/// 从旧的 enum 风格创建 Value（兼容层）
#[macro_export]
macro_rules! value {
    (Null) => { Value::null() };
    (Bool($b:expr)) => { Value::bool($b) };
    (Int($n:expr)) => { Value::int($n) };
    (Float($f:expr)) => { Value::float($f) };
    (Char($c:expr)) => { Value::char($c) };
    (String($s:expr)) => { Value::string($s) };
    (Function($f:expr)) => { Value::function($f) };
    (Array($a:expr)) => { Value::array($a) };
    (Map($m:expr)) => { Value::map($m) };
    (Range($s:expr, $e:expr, $i:expr)) => { Value::range($s, $e, $i) };
    (Iterator($it:expr)) => { Value::iterator($it) };
    (Struct($s:expr)) => { Value::struct_val($s) };
    (Class($c:expr)) => { Value::class($c) };
    (Enum($e:expr)) => { Value::enum_val($e) };
    (TypeRef($t:expr)) => { Value::type_ref($t) };
}

// 验证 Value 大小
const _: () = assert!(std::mem::size_of::<Value>() == 8);
