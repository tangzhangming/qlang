//! 字节码定义
//! 
//! 定义虚拟机执行的字节码指令

#![allow(dead_code)]

use crate::vm::Value;
use std::fmt;

/// 操作码
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OpCode {
    /// 从常量池加载值到栈
    /// 操作数: 常量索引 (u16)
    Const = 0,
    
    /// 弹出栈顶
    Pop = 1,
    
    // ============ 算术运算 ============
    /// 加法: pop b, pop a, push a + b
    Add = 10,
    /// 减法: pop b, pop a, push a - b
    Sub = 11,
    /// 乘法: pop b, pop a, push a * b
    Mul = 12,
    /// 除法: pop b, pop a, push a / b
    Div = 13,
    /// 取模: pop b, pop a, push a % b
    Mod = 14,
    /// 幂运算: pop b, pop a, push a ** b
    Pow = 15,
    /// 取负: pop a, push -a
    Neg = 16,
    
    // ============ 比较运算 ============
    /// 等于: pop b, pop a, push a == b
    Eq = 20,
    /// 不等于: pop b, pop a, push a != b
    Ne = 21,
    /// 小于: pop b, pop a, push a < b
    Lt = 22,
    /// 小于等于: pop b, pop a, push a <= b
    Le = 23,
    /// 大于: pop b, pop a, push a > b
    Gt = 24,
    /// 大于等于: pop b, pop a, push a >= b
    Ge = 25,
    
    // ============ 逻辑运算 ============
    /// 逻辑非: pop a, push !a
    Not = 30,
    
    // ============ 位运算 ============
    /// 按位与: pop b, pop a, push a & b
    BitAnd = 31,
    /// 按位或: pop b, pop a, push a | b
    BitOr = 32,
    /// 按位异或: pop b, pop a, push a ^ b
    BitXor = 33,
    /// 按位取反: pop a, push ~a
    BitNot = 34,
    /// 左移: pop b, pop a, push a << b
    Shl = 35,
    /// 右移: pop b, pop a, push a >> b
    Shr = 36,
    
    // ============ 局部变量操作 ============
    /// 获取局部变量: 操作数为槽位索引 (u16)
    GetLocal = 50,
    /// 设置局部变量: 操作数为槽位索引 (u16)
    SetLocal = 51,
    /// 获取 Upvalue（闭包捕获的变量）: 操作数为 upvalue 索引 (u16)
    GetUpvalue = 52,
    /// 设置 Upvalue: 操作数为 upvalue 索引 (u16)
    SetUpvalue = 53,
    /// 关闭 Upvalue（将栈上的值移到堆上）: 操作数为槽位索引 (u16)
    CloseUpvalue = 54,
    
    // ============ 控制流 ============
    /// 无条件跳转: 操作数为偏移量 (i16)
    Jump = 60,
    /// 条件跳转（如果为假则跳转）: 操作数为偏移量 (i16)
    JumpIfFalse = 61,
    /// 条件跳转（如果为真则跳转）: 操作数为偏移量 (i16)
    JumpIfTrue = 62,
    /// 循环跳转（向后跳）: 操作数为偏移量 (u16)
    Loop = 63,
    
    // ============ 内置函数 ============
    /// 打印栈顶值（不换行）
    Print = 70,
    /// 打印栈顶值（换行）
    PrintLn = 71,
    /// 获取类型名称: pop value, push type_name
    TypeOf = 72,
    /// 获取值大小
    SizeOf = 73,
    /// 触发 panic
    Panic = 74,
    /// 将栈顶值转换为字符串: pop value, push string
    ToString = 83,
    /// 获取当前时间戳（毫秒）: push timestamp
    /// [deprecated] 可能在未来版本移除
    Time = 88,
    
    // ============ 数组和范围和Map ============
    /// 创建数组
    /// 操作数: 元素数量 (u16)
    NewArray = 75,
    /// 创建 Map
    /// 操作数: 键值对数量 (u16)
    NewMap = 87,
    /// 获取数组元素
    GetIndex = 76,
    /// 设置数组元素
    SetIndex = 77,
    /// 创建范围 (非包含)
    NewRange = 78,
    /// 创建范围 (包含)
    NewRangeInclusive = 79,
    
    // ============ 迭代器 ============
    /// 初始化迭代器（从数组或范围创建迭代器）
    /// 栈: [..., iterable] -> [..., iterator]
    IterInit = 90,
    /// 获取迭代器下一个值
    /// 栈: [..., iterator] -> [..., iterator, value, has_next]
    IterNext = 91,
    
    // ============ Struct 操作 ============
    /// 创建 struct 实例
    /// 操作数: 字段数量 (u8), 类型名称索引 (u16)
    /// 栈: [..., field_name_1, value_1, ..., field_name_n, value_n] -> [..., struct]
    NewStruct = 92,
    /// 获取 struct 字段
    /// 操作数: 字段名称索引 (u16)
    /// 栈: [..., struct] -> [..., value]
    GetField = 93,
    /// 设置 struct 字段
    /// 操作数: 字段名称索引 (u16)
    /// 栈: [..., struct, value] -> [..., struct]
    SetField = 94,
    /// 调用 struct 方法
    /// 操作数: 方法名称索引 (u16), 参数数量 (u8)
    /// 栈: [..., struct, arg1, ..., argN] -> [..., result]
    InvokeMethod = 95,
    /// 创建 class 实例
    /// 操作数: 类名索引 (u16), 参数数量 (u8)
    /// 栈: [..., arg1, ..., argN] -> [..., instance]
    NewClass = 96,
    /// 获取静态字段
    /// 操作数: 类名索引 (u16), 字段名索引 (u16)
    GetStatic = 97,
    /// 设置静态字段
    /// 操作数: 类名索引 (u16), 字段名索引 (u16)
    SetStatic = 98,
    /// 调用静态方法
    /// 操作数: 类名索引 (u16), 方法名索引 (u16), 参数数量 (u8)
    InvokeStatic = 99,
    /// 调用父类方法
    /// 操作数: 方法名索引 (u16), 参数数量 (u8)
    InvokeSuper = 100,
    /// 复制栈顶值
    Dup = 101,
    /// 如果为 null 则跳转: 操作数为偏移量 (i16)
    /// 栈: [..., value] -> [...]（如果跳转）或 [..., value]（不跳转）
    JumpIfNull = 102,
    /// 安全获取字段（如果对象为 null 则返回 null）
    /// 操作数: 字段名称索引 (u16)
    SafeGetField = 103,
    /// 非空断言获取字段（如果对象为 null 则 panic）
    /// 操作数: 字段名称索引 (u16)
    NonNullGetField = 104,
    /// 安全调用方法（如果对象为 null 则返回 null，不调用）
    /// 操作数: 方法名索引 (u16), 参数数量 (u8)
    SafeInvokeMethod = 105,
    /// 非空断言调用方法（如果对象为 null 则 panic）
    /// 操作数: 方法名索引 (u16), 参数数量 (u8)
    NonNullInvokeMethod = 106,
    
    // ============ 函数调用 ============
    /// 创建闭包
    /// 操作数: 函数索引 (u16)
    Closure = 80,
    /// 调用函数
    /// 操作数: 参数数量 (u8)
    Call = 81,
    /// 从函数返回
    /// 返回栈顶值
    Return = 82,
    
    // ============ 类型操作 ============
    /// 类型转换 (安全): pop value, push converted_value or null
    /// 操作数: 类型名索引 (u16)
    CastSafe = 84,
    /// 类型转换 (强制): pop value, push converted_value or panic
    /// 操作数: 类型名索引 (u16)
    CastForce = 85,
    /// 类型检查: pop value, push bool
    /// 操作数: 类型名索引 (u16)
    TypeCheck = 86,
    
    // ============ 异常处理 ============
    /// 设置异常处理器
    /// 操作数: catch 块偏移量 (i16)
    SetupTry = 110,
    /// 抛出异常
    /// 栈顶值为异常对象
    Throw = 111,
    
    // ============ 专用整数指令 (性能优化) ============
    /// 整数加法 (无类型检查)
    AddInt = 120,
    /// 整数减法 (无类型检查)
    SubInt = 121,
    /// 整数乘法 (无类型检查)
    MulInt = 122,
    /// 整数除法 (无类型检查)
    DivInt = 123,
    /// 整数小于 (无类型检查)
    LtInt = 124,
    /// 整数小于等于 (无类型检查)
    LeInt = 125,
    /// 整数大于 (无类型检查)
    GtInt = 126,
    /// 整数大于等于 (无类型检查)
    GeInt = 127,
    /// 整数等于 (无类型检查)
    EqInt = 128,
    /// 整数不等于 (无类型检查)
    NeInt = 129,
    
    // ============ 融合指令 (性能优化) ============
    /// 加载小整数常量 (-128 到 127)
    /// 操作数: i8
    ConstInt8 = 130,
    /// 获取局部变量并加整数
    /// 操作数: slot (u16), value (i8)
    GetLocalAddInt = 131,
    /// 获取局部变量并减整数
    /// 操作数: slot (u16), value (i8)
    GetLocalSubInt = 132,
    /// 条件跳转并弹出 (如果为假)
    /// 操作数: offset (u16)
    JumpIfFalsePop = 133,
    /// 获取局部整数变量
    /// 操作数: slot (u16)
    GetLocalInt = 134,
    /// 尾调用优化
    /// 操作数: arg_count (u8)
    TailCall = 135,
    
    /// 整数递减 (x - 1)
    DecInt = 136,
    /// 获取局部变量并整数比较小于等于
    /// 操作数: slot (u16), value (i8)
    GetLocalLeInt = 137,
    /// 条件返回（如果栈顶为真，返回第二个栈值）
    ReturnIf = 138,
    
    // ============ 并发指令 (140-160) ============
    /// 启动协程
    /// 操作数: 参数数量 (u8)
    /// 栈: [..., closure, arg1, ..., argN] -> [...]
    GoSpawn = 140,
    
    /// 创建 Channel
    /// 操作数: 容量 (u16)
    /// 栈: [...] -> [..., channel]
    ChannelNew = 141,
    
    /// Channel 发送（阻塞）
    /// 栈: [..., channel, value] -> [...]
    ChannelSend = 142,
    
    /// Channel 接收（阻塞）
    /// 栈: [..., channel] -> [..., value]
    ChannelReceive = 143,
    
    /// Channel 尝试发送（非阻塞）
    /// 栈: [..., channel, value] -> [..., success:bool]
    ChannelTrySend = 144,
    
    /// Channel 尝试接收（非阻塞）
    /// 栈: [..., channel] -> [..., value?, has_value:bool]
    ChannelTryReceive = 145,
    
    /// Channel 关闭
    /// 栈: [..., channel] -> [...]
    ChannelClose = 146,
    
    /// 创建 Mutex
    /// 栈: [..., initial_value] -> [..., mutex]
    MutexNew = 150,
    
    /// Mutex 加锁（返回 guard）
    /// 栈: [..., mutex] -> [..., guard]
    MutexLock = 151,
    
    /// 创建 WaitGroup
    /// 栈: [...] -> [..., wait_group]
    WaitGroupNew = 155,
    
    /// WaitGroup add
    /// 栈: [..., wait_group, delta] -> [...]
    WaitGroupAdd = 156,
    
    /// WaitGroup done
    /// 栈: [..., wait_group] -> [...]
    WaitGroupDone = 157,
    
    /// WaitGroup wait
    /// 栈: [..., wait_group] -> [...]
    WaitGroupWait = 158,
    
    // ============ 控制 ============
    /// 停止执行
    Halt = 255,
}

impl From<u8> for OpCode {
    #[inline(always)]
    fn from(value: u8) -> Self {
        match value {
            0 => OpCode::Const,
            1 => OpCode::Pop,
            10 => OpCode::Add,
            11 => OpCode::Sub,
            12 => OpCode::Mul,
            13 => OpCode::Div,
            14 => OpCode::Mod,
            15 => OpCode::Pow,
            16 => OpCode::Neg,
            20 => OpCode::Eq,
            21 => OpCode::Ne,
            22 => OpCode::Lt,
            23 => OpCode::Le,
            24 => OpCode::Gt,
            25 => OpCode::Ge,
            30 => OpCode::Not,
            31 => OpCode::BitAnd,
            32 => OpCode::BitOr,
            33 => OpCode::BitXor,
            34 => OpCode::BitNot,
            35 => OpCode::Shl,
            36 => OpCode::Shr,
            50 => OpCode::GetLocal,
            51 => OpCode::SetLocal,
            52 => OpCode::GetUpvalue,
            53 => OpCode::SetUpvalue,
            54 => OpCode::CloseUpvalue,
            60 => OpCode::Jump,
            61 => OpCode::JumpIfFalse,
            62 => OpCode::JumpIfTrue,
            63 => OpCode::Loop,
            70 => OpCode::Print,
            71 => OpCode::PrintLn,
            72 => OpCode::TypeOf,
            73 => OpCode::SizeOf,
            74 => OpCode::Panic,
            75 => OpCode::NewArray,
            76 => OpCode::GetIndex,
            77 => OpCode::SetIndex,
            78 => OpCode::NewRange,
            79 => OpCode::NewRangeInclusive,
            90 => OpCode::IterInit,
            91 => OpCode::IterNext,
            92 => OpCode::NewStruct,
            93 => OpCode::GetField,
            94 => OpCode::SetField,
            95 => OpCode::InvokeMethod,
            96 => OpCode::NewClass,
            97 => OpCode::GetStatic,
            98 => OpCode::SetStatic,
            99 => OpCode::InvokeStatic,
            100 => OpCode::InvokeSuper,
            101 => OpCode::Dup,
            102 => OpCode::JumpIfNull,
            103 => OpCode::SafeGetField,
            104 => OpCode::NonNullGetField,
            105 => OpCode::SafeInvokeMethod,
            106 => OpCode::NonNullInvokeMethod,
            80 => OpCode::Closure,
            81 => OpCode::Call,
            82 => OpCode::Return,
            83 => OpCode::ToString,
            84 => OpCode::CastSafe,
            85 => OpCode::CastForce,
            86 => OpCode::TypeCheck,
            87 => OpCode::NewMap,
            88 => OpCode::Time,
            110 => OpCode::SetupTry,
            111 => OpCode::Throw,
            // 专用整数指令
            120 => OpCode::AddInt,
            121 => OpCode::SubInt,
            122 => OpCode::MulInt,
            123 => OpCode::DivInt,
            124 => OpCode::LtInt,
            125 => OpCode::LeInt,
            126 => OpCode::GtInt,
            127 => OpCode::GeInt,
            128 => OpCode::EqInt,
            129 => OpCode::NeInt,
            // 融合指令
            130 => OpCode::ConstInt8,
            131 => OpCode::GetLocalAddInt,
            132 => OpCode::GetLocalSubInt,
            133 => OpCode::JumpIfFalsePop,
            134 => OpCode::GetLocalInt,
            135 => OpCode::TailCall,
            136 => OpCode::DecInt,
            137 => OpCode::GetLocalLeInt,
            138 => OpCode::ReturnIf,
            // 并发指令
            140 => OpCode::GoSpawn,
            141 => OpCode::ChannelNew,
            142 => OpCode::ChannelSend,
            143 => OpCode::ChannelReceive,
            144 => OpCode::ChannelTrySend,
            145 => OpCode::ChannelTryReceive,
            146 => OpCode::ChannelClose,
            150 => OpCode::MutexNew,
            151 => OpCode::MutexLock,
            155 => OpCode::WaitGroupNew,
            156 => OpCode::WaitGroupAdd,
            157 => OpCode::WaitGroupDone,
            158 => OpCode::WaitGroupWait,
            255 => OpCode::Halt,
            _ => panic!("Unknown opcode: {}", value),
        }
    }
}

/// struct/class 方法信息
#[derive(Debug, Clone)]
pub struct MethodInfo {
    /// 方法名称
    pub name: String,
    /// 函数在常量池中的索引
    pub func_index: u16,
}

/// interface 方法签名信息
#[derive(Debug, Clone)]
pub struct InterfaceMethodInfo {
    /// 方法名称
    pub name: String,
    /// 参数数量
    pub arity: usize,
}

/// interface 类型信息
#[derive(Debug, Clone, Default)]
pub struct InterfaceInfo {
    /// 接口名称
    pub name: String,
    /// 方法签名列表
    pub methods: Vec<InterfaceMethodInfo>,
}

/// trait 方法信息
#[derive(Debug, Clone)]
pub struct TraitMethodInfo {
    /// 方法名称
    pub name: String,
    /// 参数数量
    pub arity: usize,
    /// 默认实现的函数索引（如果有）
    pub default_impl: Option<u16>,
}

/// trait 类型信息
#[derive(Debug, Clone, Default)]
pub struct TraitInfo {
    /// trait 名称
    pub name: String,
    /// 方法列表
    pub methods: Vec<TraitMethodInfo>,
}

/// enum 变体信息
#[derive(Debug, Clone)]
pub struct EnumVariantInfo {
    /// 变体名称
    pub name: String,
    /// 关联数据字段名列表
    pub fields: Vec<String>,
    /// 关联值（常量池索引，如 Ok = 200）
    pub value_index: Option<u16>,
}

/// enum 类型信息
#[derive(Debug, Clone, Default)]
pub struct EnumInfo {
    /// enum 名称
    pub name: String,
    /// 变体列表
    pub variants: Vec<EnumVariantInfo>,
}

/// struct/class 类型信息
#[derive(Debug, Clone, Default)]
pub struct TypeInfo {
    /// 类型名称
    pub name: String,
    /// 父类名称（仅 class 使用）
    pub parent: Option<String>,
    /// 方法列表（方法名 -> 函数在常量池中的索引）
    pub methods: std::collections::HashMap<String, u16>,
    /// 静态方法列表（方法名 -> 函数在常量池中的索引）
    pub static_methods: std::collections::HashMap<String, u16>,
    /// 字段定义（字段名 -> 默认值在常量池中的索引，如果有）
    pub fields: Vec<String>,
    /// 静态字段（字段名 -> 值在常量池中的索引）
    pub static_fields: std::collections::HashMap<String, u16>,
    /// 静态常量字段（字段名集合，这些字段不能被修改）
    pub const_fields: std::collections::HashSet<String>,
    /// 是否是 class（false 表示 struct）
    pub is_class: bool,
    /// 是否是抽象类
    pub is_abstract: bool,
    /// 抽象方法列表（方法名列表）
    pub abstract_methods: Vec<String>,
}

/// 字节码块
#[derive(Debug, Clone, Default)]
pub struct Chunk {
    /// 字节码指令
    pub code: Vec<u8>,
    /// 常量池
    pub constants: Vec<Value>,
    /// 行号信息（用于错误报告）
    pub lines: Vec<usize>,
    /// 类型信息表（类型名 -> TypeInfo）
    pub types: std::collections::HashMap<String, TypeInfo>,
    /// 接口信息表（接口名 -> InterfaceInfo）
    pub interfaces: std::collections::HashMap<String, InterfaceInfo>,
    /// trait 信息表（trait 名 -> TraitInfo）
    pub traits: std::collections::HashMap<String, TraitInfo>,
    /// enum 信息表（enum 名 -> EnumInfo）
    pub enums: std::collections::HashMap<String, EnumInfo>,
    /// 命名函数表（函数名 -> 常量池索引）
    pub named_functions: std::collections::HashMap<String, u16>,
}

impl Chunk {
    /// 创建新的字节码块
    pub fn new() -> Self {
        Self::default()
    }

    /// 写入一个字节
    pub fn write(&mut self, byte: u8, line: usize) {
        self.code.push(byte);
        self.lines.push(line);
    }

    /// 写入操作码
    pub fn write_op(&mut self, op: OpCode, line: usize) {
        self.write(op as u8, line);
    }

    /// 添加常量并返回索引
    pub fn add_constant(&mut self, value: Value) -> u16 {
        // 检查是否已存在相同的常量
        for (i, v) in self.constants.iter().enumerate() {
            if v == &value {
                return i as u16;
            }
        }
        
        let index = self.constants.len();
        if index > u16::MAX as usize {
            panic!("Too many constants in one chunk");
        }
        self.constants.push(value);
        index as u16
    }

    /// 写入常量加载指令
    pub fn write_constant(&mut self, value: Value, line: usize) {
        let index = self.add_constant(value);
        self.write_op(OpCode::Const, line);
        // 写入 16 位索引（大端序）
        self.write((index >> 8) as u8, line);
        self.write((index & 0xFF) as u8, line);
    }
    
    /// 写入局部变量获取指令
    pub fn write_get_local(&mut self, slot: usize, line: usize) {
        self.write_op(OpCode::GetLocal, line);
        self.write((slot >> 8) as u8, line);
        self.write((slot & 0xFF) as u8, line);
    }
    
    /// 写入局部变量设置指令
    pub fn write_set_local(&mut self, slot: usize, line: usize) {
        self.write_op(OpCode::SetLocal, line);
        self.write((slot >> 8) as u8, line);
        self.write((slot & 0xFF) as u8, line);
    }
    
    /// 写入跳转指令，返回跳转地址用于后续回填
    pub fn write_jump(&mut self, op: OpCode, line: usize) -> usize {
        self.write_op(op, line);
        self.write(0xFF, line);
        self.write(0xFF, line);
        self.code.len() - 2 // 返回偏移量的位置
    }
    
    /// 回填跳转偏移量
    pub fn patch_jump(&mut self, offset: usize) {
        let jump = self.code.len() - offset - 2;
        
        if jump > u16::MAX as usize {
            panic!("Jump too large");
        }
        
        self.code[offset] = ((jump >> 8) & 0xFF) as u8;
        self.code[offset + 1] = (jump & 0xFF) as u8;
    }
    
    /// 写入循环指令（向后跳转）
    pub fn write_loop(&mut self, loop_start: usize, line: usize) {
        self.write_op(OpCode::Loop, line);
        
        let offset = self.code.len() - loop_start + 2;
        if offset > u16::MAX as usize {
            panic!("Loop body too large");
        }
        
        self.write(((offset >> 8) & 0xFF) as u8, line);
        self.write((offset & 0xFF) as u8, line);
    }
    
    /// 写入函数调用指令
    pub fn write_call(&mut self, arg_count: u8, line: usize) {
        self.write_op(OpCode::Call, line);
        self.write(arg_count, line);
    }
    
    /// 写入尾调用指令
    pub fn write_tail_call(&mut self, arg_count: u8, line: usize) {
        self.write_op(OpCode::TailCall, line);
        self.write(arg_count, line);
    }
    
    /// 写入 16 位无符号整数（大端序）
    pub fn write_u16(&mut self, value: u16, line: usize) {
        self.write((value >> 8) as u8, line);
        self.write((value & 0xFF) as u8, line);
    }
    
    /// 写入小整数常量 (-128 到 127)
    pub fn write_const_int8(&mut self, value: i8, line: usize) {
        self.write_op(OpCode::ConstInt8, line);
        self.write(value as u8, line);
    }
    
    /// 写入获取局部整数变量
    pub fn write_get_local_int(&mut self, slot: u16, line: usize) {
        self.write_op(OpCode::GetLocalInt, line);
        self.write((slot >> 8) as u8, line);
        self.write((slot & 0xFF) as u8, line);
    }
    
    /// 写入获取局部变量并加整数
    pub fn write_get_local_add_int(&mut self, slot: u16, value: i8, line: usize) {
        self.write_op(OpCode::GetLocalAddInt, line);
        self.write((slot >> 8) as u8, line);
        self.write((slot & 0xFF) as u8, line);
        self.write(value as u8, line);
    }
    
    /// 写入获取局部变量并减整数
    pub fn write_get_local_sub_int(&mut self, slot: u16, value: i8, line: usize) {
        self.write_op(OpCode::GetLocalSubInt, line);
        self.write((slot >> 8) as u8, line);
        self.write((slot & 0xFF) as u8, line);
        self.write(value as u8, line);
    }
    
    /// 写入条件跳转并弹出
    pub fn write_jump_if_false_pop(&mut self, line: usize) -> usize {
        self.write_op(OpCode::JumpIfFalsePop, line);
        self.write(0xFF, line);
        self.write(0xFF, line);
        self.code.len() - 2
    }
    
    /// 获取当前代码位置
    pub fn current_offset(&self) -> usize {
        self.code.len()
    }

    /// 获取指定位置的行号
    pub fn get_line(&self, offset: usize) -> usize {
        if offset < self.lines.len() {
            self.lines[offset]
        } else {
            0
        }
    }
    
    /// 注册类型（struct）
    pub fn register_type(&mut self, name: String) {
        if !self.types.contains_key(&name) {
            self.types.insert(name.clone(), TypeInfo {
                name,
                parent: None,
                methods: std::collections::HashMap::new(),
                static_methods: std::collections::HashMap::new(),
                fields: Vec::new(),
                static_fields: std::collections::HashMap::new(),
                const_fields: std::collections::HashSet::new(),
                is_class: false,
                is_abstract: false,
                abstract_methods: Vec::new(),
            });
        }
    }
    
    /// 注册 class 类型
    pub fn register_class(&mut self, name: String, parent: Option<String>) {
        self.register_class_with_abstract(name, parent, false);
    }
    
    /// 注册 class 类型（带抽象标记）
    pub fn register_class_with_abstract(&mut self, name: String, parent: Option<String>, is_abstract: bool) {
        if !self.types.contains_key(&name) {
            self.types.insert(name.clone(), TypeInfo {
                name,
                parent,
                methods: std::collections::HashMap::new(),
                static_methods: std::collections::HashMap::new(),
                fields: Vec::new(),
                static_fields: std::collections::HashMap::new(),
                const_fields: std::collections::HashSet::new(),
                is_class: true,
                is_abstract,
                abstract_methods: Vec::new(),
            });
        }
    }
    
    /// 注册抽象方法
    pub fn register_abstract_method(&mut self, type_name: &str, method_name: String) {
        if let Some(type_info) = self.types.get_mut(type_name) {
            type_info.abstract_methods.push(method_name);
        }
    }
    
    /// 注册方法到类型
    pub fn register_method(&mut self, type_name: &str, method_name: String, func_index: u16) {
        if let Some(type_info) = self.types.get_mut(type_name) {
            type_info.methods.insert(method_name, func_index);
        }
    }
    
    /// 注册 interface
    pub fn register_interface(&mut self, name: String, methods: Vec<InterfaceMethodInfo>) {
        if !self.interfaces.contains_key(&name) {
            self.interfaces.insert(name.clone(), InterfaceInfo { name, methods });
        }
    }
    
    /// 获取 interface 信息
    pub fn get_interface(&self, name: &str) -> Option<&InterfaceInfo> {
        self.interfaces.get(name)
    }
    
    /// 注册 trait
    pub fn register_trait(&mut self, name: String, methods: Vec<TraitMethodInfo>) {
        if !self.traits.contains_key(&name) {
            self.traits.insert(name.clone(), TraitInfo { name, methods });
        }
    }
    
    /// 获取 trait 信息
    pub fn get_trait(&self, name: &str) -> Option<&TraitInfo> {
        self.traits.get(name)
    }
    
    /// 注册 enum
    pub fn register_enum(&mut self, name: String, variants: Vec<EnumVariantInfo>) {
        if !self.enums.contains_key(&name) {
            self.enums.insert(name.clone(), EnumInfo { name, variants });
        }
    }
    
    /// 获取 enum 信息
    pub fn get_enum(&self, name: &str) -> Option<&EnumInfo> {
        self.enums.get(name)
    }
    
    /// 注册静态方法到类型
    pub fn register_static_method(&mut self, type_name: &str, method_name: String, func_index: u16) {
        if let Some(type_info) = self.types.get_mut(type_name) {
            type_info.static_methods.insert(method_name, func_index);
        }
    }
    
    /// 注册字段到类型
    pub fn register_field(&mut self, type_name: &str, field_name: String) {
        if let Some(type_info) = self.types.get_mut(type_name) {
            if !type_info.fields.contains(&field_name) {
                type_info.fields.push(field_name);
            }
        }
    }
    
    /// 注册静态字段到类型
    pub fn register_static_field(&mut self, type_name: &str, field_name: String, value_index: u16) {
        if let Some(type_info) = self.types.get_mut(type_name) {
            type_info.static_fields.insert(field_name, value_index);
        }
    }
    
    /// 注册静态常量字段
    pub fn register_static_const(&mut self, type_name: &str, field_name: String, value_index: u16) {
        if let Some(type_info) = self.types.get_mut(type_name) {
            type_info.static_fields.insert(field_name.clone(), value_index);
            type_info.const_fields.insert(field_name);
        }
    }
    
    /// 检查静态字段是否是常量
    pub fn is_static_const(&self, type_name: &str, field_name: &str) -> bool {
        if let Some(type_info) = self.types.get(type_name) {
            type_info.const_fields.contains(field_name)
        } else {
            false
        }
    }
    
    /// 获取类型的方法函数索引（包含继承链查找）
    pub fn get_method(&self, type_name: &str, method_name: &str) -> Option<u16> {
        let mut current_type = type_name;
        loop {
            if let Some(type_info) = self.types.get(current_type) {
                // 先在当前类中查找
                if let Some(idx) = type_info.methods.get(method_name) {
                    return Some(*idx);
                }
                // 如果当前类没有，尝试在父类中查找
                if let Some(ref parent) = type_info.parent {
                    current_type = parent;
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
    }
    
    /// 获取类型的静态方法函数索引
    pub fn get_static_method(&self, type_name: &str, method_name: &str) -> Option<u16> {
        self.types.get(type_name)
            .and_then(|t| t.static_methods.get(method_name))
            .copied()
    }
    
    /// 获取类型信息
    pub fn get_type(&self, type_name: &str) -> Option<&TypeInfo> {
        self.types.get(type_name)
    }
    
    /// 注册命名函数
    pub fn register_named_function(&mut self, name: String, func_index: u16) {
        self.named_functions.insert(name, func_index);
    }
    
    /// 获取命名函数的常量池索引
    pub fn get_named_function(&self, name: &str) -> Option<u16> {
        self.named_functions.get(name).copied()
    }
}

impl fmt::Display for Chunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== Chunk ===")?;
        writeln!(f, "Constants:")?;
        for (i, c) in self.constants.iter().enumerate() {
            writeln!(f, "  {}: {}", i, c)?;
        }
        writeln!(f, "Code:")?;
        
        let mut offset = 0;
        while offset < self.code.len() {
            offset = self.disassemble_instruction(f, offset)?;
        }
        
        Ok(())
    }
}

impl Chunk {
    /// 反汇编单条指令
    fn disassemble_instruction(&self, f: &mut fmt::Formatter<'_>, offset: usize) -> Result<usize, fmt::Error> {
        write!(f, "{:04} ", offset)?;
        
        // 显示行号
        if offset > 0 && self.lines[offset] == self.lines[offset - 1] {
            write!(f, "   | ")?;
        } else {
            write!(f, "{:4} ", self.lines[offset])?;
        }
        
        let instruction = OpCode::from(self.code[offset]);
        
        match instruction {
            OpCode::Const => {
                let index = ((self.code[offset + 1] as u16) << 8) | (self.code[offset + 2] as u16);
                let value = &self.constants[index as usize];
                writeln!(f, "CONST {:5} ({})", index, value)?;
                Ok(offset + 3)
            }
            _ => {
                writeln!(f, "{:?}", instruction)?;
                Ok(offset + 1)
            }
        }
    }
}
