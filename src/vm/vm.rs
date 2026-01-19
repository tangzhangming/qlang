//! 虚拟机实现
//! 
//! 执行字节码指令

use crate::compiler::{Chunk, OpCode};
use crate::i18n::{Locale, format_message, messages};
use super::value::{Value, Iterator, IteratorSource, StructInstance, Function};
use crate::stdlib::StdlibRegistry;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use parking_lot::Mutex;
use std::sync::OnceLock;

/// 栈大小（预分配容量，避免运行时扩容）
const STACK_SIZE: usize = 1024;

/// 最大调用深度
const MAX_FRAMES: usize = 64;

/// 全局标准库注册表（延迟初始化）
static STDLIB_REGISTRY: OnceLock<StdlibRegistry> = OnceLock::new();

/// 获取标准库注册表
fn get_stdlib_registry() -> &'static StdlibRegistry {
    STDLIB_REGISTRY.get_or_init(|| StdlibRegistry::new())
}

/// 栈帧信息（用于栈追踪）
#[derive(Debug, Clone)]
pub struct StackFrame {
    /// 函数名（闭包为 "<closure>"，顶层为 "<main>"）
    pub function_name: String,
    /// 源码文件名（如果有）
    pub file_name: Option<String>,
    /// 行号
    pub line: usize,
    /// 列号（如果有）
    pub column: Option<usize>,
}

impl std::fmt::Display for StackFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let file = self.file_name.as_deref().unwrap_or("<unknown>");
        if let Some(col) = self.column {
            write!(f, "  at {} ({}:{}:{})", self.function_name, file, self.line, col)
        } else {
            write!(f, "  at {} ({}:{})", self.function_name, file, self.line)
        }
    }
}

/// 运行时错误
#[derive(Debug, Clone)]
pub struct RuntimeError {
    /// 错误消息
    pub message: String,
    /// 发生错误的行号
    pub line: usize,
    /// 栈追踪
    pub stack_trace: Vec<StackFrame>,
}

impl RuntimeError {
    /// 创建新的运行时错误
    pub fn new(message: String, line: usize) -> Self {
        Self { 
            message, 
            line,
            stack_trace: Vec::new(),
        }
    }
    
    /// 创建带栈追踪的运行时错误
    pub fn with_trace(message: String, line: usize, stack_trace: Vec<StackFrame>) -> Self {
        Self { message, line, stack_trace }
    }
    
    /// 格式化完整的错误信息（包括栈追踪）
    pub fn format_full(&self) -> String {
        let mut result = format!("RuntimeError: {} (line {})", self.message, self.line);
        if !self.stack_trace.is_empty() {
            result.push_str("\nStack trace:");
            for frame in &self.stack_trace {
                result.push('\n');
                result.push_str(&frame.to_string());
            }
        }
        result
    }
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format_full())
    }
}

/// 调用帧（优化：使用 Copy 而非 Clone）
#[derive(Debug, Clone, Copy)]
struct CallFrame {
    /// 返回地址（调用后继续执行的位置）
    return_ip: u32,
    /// 栈基址（参数和局部变量的起始位置）
    base_slot: u16,
    /// 是否是方法调用（影响返回时的栈截断位置）
    is_method_call: bool,
    // 注意：移除了 func 字段，因为大多数情况下不需要
}

/// 异常处理器
#[derive(Debug)]
struct ExceptionHandler {
    /// catch 块的地址
    catch_ip: usize,
    /// 设置处理器时的栈深度
    stack_depth: usize,
    /// 设置处理器时的调用帧深度
    frame_depth: usize,
}

/// 虚拟机
pub struct VM {
    /// 字节码块
    chunk: Arc<Chunk>,
    /// 指令指针
    ip: usize,
    /// 值栈
    stack: Vec<Value>,
    /// 调用栈
    frames: Vec<CallFrame>,
    /// 异常处理器栈
    exception_handlers: Vec<ExceptionHandler>,
    /// 当前语言
    locale: Locale,
    /// 当前栈基址（缓存，避免每次访问 frames.last()）
    current_base: usize,
    /// 静态字段缓存 (类名::字段名 -> 值)
    static_fields: std::collections::HashMap<String, Value>,
    /// VTable 注册表（用于虚方法派发）
    vtable_registry: super::vtable::VTableRegistry,
    /// 抢占标志（用于协程调度）
    /// 当调度器需要抢占当前协程时设置为 true
    preempt_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// 内联缓存（方法调用优化）
    /// 缓存 (类型名, 方法名) -> 函数索引
    inline_cache: std::collections::HashMap<(String, String), u16>,
}

impl VM {
    /// 创建新的虚拟机
    pub fn new(chunk: Arc<Chunk>, locale: Locale) -> Self {
        Self {
            chunk,
            ip: 0,
            stack: Vec::with_capacity(STACK_SIZE),
            frames: Vec::with_capacity(MAX_FRAMES),
            exception_handlers: Vec::new(),
            locale,
            current_base: 0,
            static_fields: std::collections::HashMap::new(),
            vtable_registry: super::vtable::VTableRegistry::new(),
            preempt_flag: None,
            inline_cache: std::collections::HashMap::with_capacity(64),
        }
    }
    
    /// 创建带抢占支持的虚拟机
    pub fn with_preempt(chunk: Arc<Chunk>, locale: Locale, preempt_flag: Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self {
            chunk,
            ip: 0,
            stack: Vec::with_capacity(STACK_SIZE),
            frames: Vec::with_capacity(MAX_FRAMES),
            exception_handlers: Vec::new(),
            locale,
            current_base: 0,
            static_fields: std::collections::HashMap::new(),
            vtable_registry: super::vtable::VTableRegistry::new(),
            preempt_flag: Some(preempt_flag),
            inline_cache: std::collections::HashMap::with_capacity(64),
        }
    }
    
    /// 设置抢占标志
    pub fn set_preempt_flag(&mut self, flag: Arc<std::sync::atomic::AtomicBool>) {
        self.preempt_flag = Some(flag);
    }
    
    /// 检查是否需要抢占
    #[inline]
    pub fn should_preempt(&self) -> bool {
        self.preempt_flag
            .as_ref()
            .map(|f| f.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(false)
    }
    
    /// 请求抢占
    pub fn request_preempt(&self) {
        if let Some(flag) = &self.preempt_flag {
            flag.store(true, std::sync::atomic::Ordering::Release);
        }
    }
    
    /// 清除抢占标志
    pub fn clear_preempt(&self) {
        if let Some(flag) = &self.preempt_flag {
            flag.store(false, std::sync::atomic::Ordering::Release);
        }
    }
    
    /// 查找方法（带内联缓存）
    /// 
    /// 缓存 (类型名, 方法名) -> 函数索引 的映射，避免重复查找
    #[inline]
    pub fn lookup_method_cached(&mut self, type_name: &str, method_name: &str) -> Option<u16> {
        let cache_key = (type_name.to_string(), method_name.to_string());
        
        // 先查缓存
        if let Some(&func_index) = self.inline_cache.get(&cache_key) {
            return Some(func_index);
        }
        
        // 缓存未命中，查找类型信息
        if let Some(type_info) = self.chunk.get_type(type_name) {
            if let Some(&method_index) = type_info.methods.get(method_name) {
                // 缓存结果
                self.inline_cache.insert(cache_key, method_index);
                return Some(method_index);
            }
        }
        
        None
    }
    
    /// 清除内联缓存
    pub fn clear_inline_cache(&mut self) {
        self.inline_cache.clear();
    }
    
    // ========== GC 根集扫描支持 ==========
    
    /// 扫描 VM 栈上的所有根引用
    /// 
    /// 用于垃圾回收器标记阶段，遍历所有可能包含引用的值
    pub fn scan_gc_roots<F>(&self, mut callback: F) 
    where
        F: FnMut(&Value),
    {
        // 扫描值栈
        for value in &self.stack {
            callback(value);
        }
        
        // 扫描静态字段
        for value in self.static_fields.values() {
            callback(value);
        }
    }
    
    /// 获取栈上所有活跃引用的迭代器
    pub fn stack_roots(&self) -> impl std::iter::Iterator<Item = &Value> {
        self.stack.iter()
    }
    
    /// 获取当前栈深度
    pub fn stack_depth(&self) -> usize {
        self.stack.len()
    }
    
    /// 获取当前调用帧数量
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
    
    /// 创建同步版本的虚拟机（别名，保持兼容）
    pub fn new_sync(chunk: Arc<Chunk>, locale: Locale) -> Self {
        Self::new(chunk, locale)
    }
    
    /// 运行协程（函数返回时自动退出）
    pub fn run_coroutine(&mut self) -> Result<(), RuntimeError> {
        self.run_coroutine_internal()
    }
    
    /// 内部协程运行
    fn run_coroutine_internal(&mut self) -> Result<(), RuntimeError> {
        // 运行直到顶层帧返回
        loop {
            let op = self.read_byte();
            
            // 检查是否是返回指令
            if op == 82 { // OpCode::Return
                let return_value = self.pop_fast();
                
                if self.frames.is_empty() {
                    // 没有调用帧了，协程结束
                    return Ok(());
                }
                
                let frame = self.frames.pop().unwrap();
                
                // 检查是否是协程的顶层帧（return_ip == u32::MAX）
                if frame.return_ip == u32::MAX {
                    // 协程结束
                    return Ok(());
                }
                
                let truncate_to = if frame.is_method_call {
                    frame.base_slot as usize
                } else {
                    (frame.base_slot as usize).saturating_sub(1)
                };
                self.stack.truncate(truncate_to);
                self.push_fast(return_value);
                self.ip = frame.return_ip as usize;
                self.current_base = self.frames.last().map(|f| f.base_slot as usize).unwrap_or(0);
                continue;
            }
            
            // 检查是否是 Halt 指令
            if op == 255 { // OpCode::Halt
                return Ok(());
            }
            
            // 回退并使用主循环处理
            self.ip -= 1;
            
            // 执行主循环的一次迭代
            if let Err(e) = self.execute_one_instruction_sync() {
                return Err(e);
            }
        }
    }
    
    /// 执行一条指令（用于协程，同步版本）
    fn execute_one_instruction_sync(&mut self) -> Result<(), RuntimeError> {
        // 保存当前 IP，用于检测是否处理了指令
        let saved_ip = self.ip;
        
        // 调用 run()，但只执行一条指令就返回
        // 这是一个简化的实现，实际上我们复用主循环
        let op = self.read_byte();
        let opcode = OpCode::from(op);
        
        // 执行简化版的指令
        match opcode {
            OpCode::Const => {
                let index = self.read_u16() as usize;
                let value = unsafe { self.chunk.constants.get_unchecked(index).clone() };
                self.push_fast(value);
            }
            OpCode::Pop => {
                self.pop()?;
            }
            OpCode::Add => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                    self.push_fast(Value::int(x + y));
                } else {
                    let result = (a + b).map_err(|e| self.runtime_error(&e))?;
                    self.push_fast(result);
                }
            }
            OpCode::GetLocal => {
                let slot = self.read_u16() as usize;
                let actual_slot = self.current_base + slot;
                let value = self.stack[actual_slot].clone();
                self.push_fast(value);
            }
            OpCode::SetLocal => {
                let slot = self.read_u16() as usize;
                let value = self.peek()?.clone();
                let actual_slot = self.current_base + slot;
                self.stack[actual_slot] = value;
            }
            OpCode::GetUpvalue => {
                // 获取 upvalue（当前简化实现，实际需要 upvalue 运行时支持）
                let _index = self.read_u16() as usize;
                // TODO: 实现完整的 upvalue 支持
                self.push_fast(Value::null());
            }
            OpCode::SetUpvalue => {
                let _index = self.read_u16() as usize;
                // TODO: 实现完整的 upvalue 支持
            }
            OpCode::CloseUpvalue => {
                let _slot = self.read_u16() as usize;
                // TODO: 实现完整的 upvalue 关闭
            }
            OpCode::PrintLn => {
                let value = self.pop_fast();
                println!("{}", value);
                self.push_fast(Value::null());
            }
            OpCode::Print => {
                let value = self.pop_fast();
                print!("{}", value);
                self.push_fast(Value::null());
            }
            OpCode::Call => {
                let arg_count = self.read_byte() as usize;
                let callee_idx = self.stack.len() - arg_count - 1;
                let callee = self.stack[callee_idx].clone();
                
                if let Some(func) = callee.as_function() {
                    if self.frames.len() >= MAX_FRAMES {
                        return Err(self.runtime_error("Stack overflow"));
                    }
                    let base_slot = callee_idx + 1;
                    self.frames.push(CallFrame {
                        return_ip: self.ip as u32,
                        base_slot: base_slot as u16,
                        is_method_call: false,
                    });
                    self.current_base = base_slot;
                    self.ip = func.chunk_index;
                } else {
                    return Err(self.runtime_error(&format!("Cannot call {}", callee.type_name())));
                }
            }
            _ => {
                // 其他指令：回退并报告不支持
                // 实际上应该支持更多指令，这里简化处理
                return Err(self.runtime_error(&format!("Unsupported instruction {:?} in coroutine", opcode)));
            }
        }
        
        Ok(())
    }

    /// 运行字节码
    /// 
    /// 使用直接 u8 匹配优化热路径指令，避免 OpCode::from() 转换开销
    pub fn run(&mut self) -> Result<(), RuntimeError> {
        // 热路径 opcode 常量（避免每次转换）
        const OP_CONST_INT8: u8 = 130;
        const OP_GET_LOCAL: u8 = 50;
        const OP_GET_LOCAL_INT: u8 = 134;
        const OP_ADD_INT: u8 = 120;
        const OP_SUB_INT: u8 = 121;
        const OP_LE_INT: u8 = 125;
        const OP_LT_INT: u8 = 124;
        const OP_GET_LOCAL_ADD_INT: u8 = 131;
        const OP_GET_LOCAL_SUB_INT: u8 = 132;
        const OP_GET_LOCAL_LE_INT: u8 = 137;
        const OP_JUMP_IF_FALSE: u8 = 61;
        const OP_JUMP_IF_FALSE_POP: u8 = 133;
        const OP_CALL: u8 = 81;
        const OP_RETURN: u8 = 82;
        const OP_CONST: u8 = 0;
        const OP_HALT: u8 = 255;
        // 超级指令常量
        const OP_ADD_LOCALS: u8 = 200;
        const OP_SUB_LOCALS: u8 = 201;
        const OP_JUMP_IF_LOCAL_LE_CONST: u8 = 202;
        const OP_JUMP_IF_LOCAL_LT_CONST: u8 = 203;
        const OP_RETURN_LOCAL: u8 = 205;
        const OP_RETURN_INT: u8 = 206;
        const OP_LOAD_LOCALS2: u8 = 207;
        
        // 抢占式调度：安全点检查仅在跳转/调用指令处进行（向后跳转表示循环）
        // 这样避免在热路径上增加开销
        
        loop {
            let op = self.read_byte();
            
            // 热路径：直接 u8 匹配，避免 OpCode::from() 开销
            match op {
                OP_CONST_INT8 => {
                    let value = self.read_byte() as i8 as i64;
                    self.push_fast(Value::int(value));
                    continue;
                }
                OP_GET_LOCAL => {
                    let slot = self.read_u16() as usize;
                    let actual_slot = self.current_base + slot;
                    let value = unsafe { self.stack.get_unchecked(actual_slot).clone() };
                    self.push_fast(value);
                    continue;
                }
                OP_GET_LOCAL_INT => {
                    let slot = self.read_u16() as usize;
                    let actual_slot = self.current_base + slot;
                    let value = unsafe { self.stack.get_unchecked(actual_slot).clone() };
                    self.push_fast(value);
                    continue;
                }
                OP_ADD_INT => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    let x = unsafe { a.as_int().unwrap_unchecked() };
                    let y = unsafe { b.as_int().unwrap_unchecked() };
                    self.push_fast(Value::int(x + y));
                    continue;
                }
                OP_SUB_INT => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    let x = unsafe { a.as_int().unwrap_unchecked() };
                    let y = unsafe { b.as_int().unwrap_unchecked() };
                    self.push_fast(Value::int(x - y));
                    continue;
                }
                OP_LE_INT => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    let x = unsafe { a.as_int().unwrap_unchecked() };
                    let y = unsafe { b.as_int().unwrap_unchecked() };
                    self.push_fast(Value::bool(x <= y));
                    continue;
                }
                OP_LT_INT => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    let x = unsafe { a.as_int().unwrap_unchecked() };
                    let y = unsafe { b.as_int().unwrap_unchecked() };
                    self.push_fast(Value::bool(x < y));
                    continue;
                }
                OP_GET_LOCAL_ADD_INT => {
                    let slot = self.read_u16() as usize;
                    let add_value = self.read_byte() as i8 as i64;
                    let actual_slot = self.current_base + slot;
                    let base_value = unsafe { self.stack.get_unchecked(actual_slot) };
                    let n = unsafe { base_value.as_int().unwrap_unchecked() };
                    self.push_fast(Value::int(n + add_value));
                    continue;
                }
                OP_GET_LOCAL_SUB_INT => {
                    let slot = self.read_u16() as usize;
                    let sub_value = self.read_byte() as i8 as i64;
                    let actual_slot = self.current_base + slot;
                    let base_value = unsafe { self.stack.get_unchecked(actual_slot) };
                    let n = unsafe { base_value.as_int().unwrap_unchecked() };
                    self.push_fast(Value::int(n - sub_value));
                    continue;
                }
                OP_GET_LOCAL_LE_INT => {
                    let slot = self.read_u16() as usize;
                    let cmp_value = self.read_byte() as i8 as i64;
                    let actual_slot = self.current_base + slot;
                    let local = unsafe { self.stack.get_unchecked(actual_slot) };
                    let n = unsafe { local.as_int().unwrap_unchecked() };
                    self.push_fast(Value::bool(n <= cmp_value));
                    continue;
                }
                OP_JUMP_IF_FALSE => {
                    let offset = self.read_u16() as i16;
                    let condition = self.peek()?;
                    if !condition.is_truthy() {
                        self.ip = (self.ip as isize + offset as isize) as usize;
                    }
                    continue;
                }
                OP_JUMP_IF_FALSE_POP => {
                    let offset = self.read_u16() as usize;
                    let condition = self.pop_fast();
                    if !condition.is_truthy() {
                        self.ip += offset;
                    }
                    continue;
                }
                OP_CONST => {
                    let index = self.read_u16() as usize;
                    let value = unsafe { self.chunk.constants.get_unchecked(index).clone() };
                    self.push_fast(value);
                    continue;
                }
                OP_CALL => {
                    let arg_count = self.read_byte() as usize;
                    
                    // 安全检查：确保栈上有足够的元素
                    if self.stack.len() < arg_count + 1 {
                        return Err(self.runtime_error("Stack underflow in function call"));
                    }
                    
                    let callee_idx = self.stack.len() - arg_count - 1;
                    let callee = self.stack[callee_idx].clone();
                    
                    if let Some(func) = callee.as_function() {
                        // 快速路径：简单函数调用（无默认参数、无可变参数）
                        if !func.has_variadic && func.defaults.is_empty() && arg_count == func.arity {
                            // 使用 unsafe 优化帧操作
                            let frames_len = self.frames.len();
                            if frames_len >= MAX_FRAMES {
                                return Err(self.runtime_error("Stack overflow"));
                            }
                            let base_slot = callee_idx + 1;
                            // unsafe 压入调用帧，避免容量检查
                            unsafe {
                                let frame = CallFrame {
                                    return_ip: self.ip as u32,
                                    base_slot: base_slot as u16,
                                    is_method_call: false,
                                };
                                std::ptr::write(self.frames.as_mut_ptr().add(frames_len), frame);
                                self.frames.set_len(frames_len + 1);
                            }
                            self.current_base = base_slot;
                            self.ip = func.chunk_index;
                            continue;
                        }
                        // 慢速路径也在这里处理，不要 fall through
                        // 检查调用深度
                        if self.frames.len() >= MAX_FRAMES {
                            return Err(self.runtime_error("Stack overflow: too many nested function calls"));
                        }
                        
                        let fixed_params = if func.has_variadic { func.arity - 1 } else { func.arity };
                        
                        // 检查必需参数数量
                        if arg_count < func.required_params {
                            let msg = format!(
                                "Expected at least {} arguments but got {}",
                                func.required_params, arg_count
                            );
                            return Err(self.runtime_error(&msg));
                        }
                        
                        // 如果没有可变参数，检查参数上限
                        if !func.has_variadic && arg_count > func.arity {
                            let msg = format!(
                                "Expected at most {} arguments but got {}",
                                func.arity, arg_count
                            );
                            return Err(self.runtime_error(&msg));
                        }
                        
                        // 处理默认参数：补充缺失的参数
                        if arg_count < fixed_params && !func.defaults.is_empty() {
                            let defaults_start = func.required_params;
                            for i in arg_count..fixed_params {
                                let default_idx = i - defaults_start;
                                if default_idx < func.defaults.len() {
                                    self.push_fast(func.defaults[default_idx].clone());
                                }
                            }
                        }
                        
                        // 处理可变参数：将多余的参数打包成数组
                        if func.has_variadic {
                            let variadic_count = if arg_count > fixed_params {
                                arg_count - fixed_params
                            } else {
                                0
                            };
                            
                            // 收集可变参数
                            let mut variadic_args = Vec::with_capacity(variadic_count);
                            for _ in 0..variadic_count {
                                variadic_args.push(self.pop_fast());
                            }
                            variadic_args.reverse();
                            
                            // 创建数组并压栈
                            self.push_fast(Value::array(std::sync::Arc::new(parking_lot::Mutex::new(variadic_args))));
                        }
                        
                        // 创建调用帧
                        let base_slot = callee_idx + 1;
                        self.frames.push(CallFrame {
                            return_ip: self.ip as u32,
                            base_slot: base_slot as u16,
                            is_method_call: false,
                        });
                        
                        self.current_base = base_slot;
                        self.ip = func.chunk_index;
                        continue;
                    } else {
                        return Err(self.runtime_error(&format!("Cannot call {}", callee.type_name())));
                    }
                }
                OP_RETURN => {
                    let return_value = self.pop_fast();
                    
                    let frames_len = self.frames.len();
                    if frames_len == 0 {
                        self.push_fast(return_value);
                        return Ok(());
                    }
                    
                    // unsafe 弹出调用帧
                    let frame = unsafe {
                        let new_len = frames_len - 1;
                        self.frames.set_len(new_len);
                        std::ptr::read(self.frames.as_ptr().add(new_len))
                    };
                    
                    // 简单函数调用：直接计算截断位置
                    let truncate_to = if frame.is_method_call {
                        frame.base_slot as usize
                    } else {
                        (frame.base_slot as usize).saturating_sub(1)
                    };
                    
                    // unsafe 截断栈
                    unsafe { self.stack.set_len(truncate_to); }
                    self.push_fast(return_value);
                    
                    self.ip = frame.return_ip as usize;
                    // 优化：直接读取新的 base，避免 last().map() 开销
                    let new_len = self.frames.len();
                    self.current_base = if new_len > 0 {
                        unsafe { self.frames.get_unchecked(new_len - 1).base_slot as usize }
                    } else {
                        0
                    };
                    continue;
                }
                OP_HALT => {
                    return Ok(());
                }
                // ====== 超级指令（热路径） ======
                OP_ADD_LOCALS => {
                    // 两个局部变量相加
                    let slot1 = self.read_byte() as usize;
                    let slot2 = self.read_byte() as usize;
                    let actual1 = self.current_base + slot1;
                    let actual2 = self.current_base + slot2;
                    let a = unsafe { self.stack.get_unchecked(actual1) };
                    let b = unsafe { self.stack.get_unchecked(actual2) };
                    let x = unsafe { a.as_int().unwrap_unchecked() };
                    let y = unsafe { b.as_int().unwrap_unchecked() };
                    self.push_fast(Value::int(x + y));
                    continue;
                }
                OP_SUB_LOCALS => {
                    // 两个局部变量相减
                    let slot1 = self.read_byte() as usize;
                    let slot2 = self.read_byte() as usize;
                    let actual1 = self.current_base + slot1;
                    let actual2 = self.current_base + slot2;
                    let a = unsafe { self.stack.get_unchecked(actual1) };
                    let b = unsafe { self.stack.get_unchecked(actual2) };
                    let x = unsafe { a.as_int().unwrap_unchecked() };
                    let y = unsafe { b.as_int().unwrap_unchecked() };
                    self.push_fast(Value::int(x - y));
                    continue;
                }
                OP_JUMP_IF_LOCAL_LE_CONST => {
                    // 局部变量 <= 常量 ? 跳转
                    let slot = self.read_byte() as usize;
                    let const_val = self.read_byte() as i8 as i64;
                    let offset = self.read_i16();
                    let actual = self.current_base + slot;
                    let local = unsafe { self.stack.get_unchecked(actual) };
                    let n = unsafe { local.as_int().unwrap_unchecked() };
                    if n <= const_val {
                        self.ip = (self.ip as isize + offset as isize) as usize;
                    }
                    continue;
                }
                OP_JUMP_IF_LOCAL_LT_CONST => {
                    // 局部变量 < 常量 ? 跳转
                    let slot = self.read_byte() as usize;
                    let const_val = self.read_byte() as i8 as i64;
                    let offset = self.read_i16();
                    let actual = self.current_base + slot;
                    let local = unsafe { self.stack.get_unchecked(actual) };
                    let n = unsafe { local.as_int().unwrap_unchecked() };
                    if n < const_val {
                        self.ip = (self.ip as isize + offset as isize) as usize;
                    }
                    continue;
                }
                OP_RETURN_LOCAL => {
                    // 返回局部变量
                    let slot = self.read_byte() as usize;
                    let actual = self.current_base + slot;
                    let return_value = unsafe { self.stack.get_unchecked(actual).clone() };
                    
                    if self.frames.is_empty() {
                        self.push_fast(return_value);
                        return Ok(());
                    }
                    
                    let frame = unsafe { self.frames.pop().unwrap_unchecked() };
                    let truncate_to = if frame.is_method_call {
                        frame.base_slot as usize
                    } else {
                        (frame.base_slot as usize).saturating_sub(1)
                    };
                    self.stack.truncate(truncate_to);
                    self.push_fast(return_value);
                    self.ip = frame.return_ip as usize;
                    self.current_base = self.frames.last().map(|f| f.base_slot as usize).unwrap_or(0);
                    continue;
                }
                OP_RETURN_INT => {
                    // 返回小整数常量
                    let value = self.read_byte() as i8 as i64;
                    let return_value = Value::int(value);
                    
                    if self.frames.is_empty() {
                        self.push_fast(return_value);
                        return Ok(());
                    }
                    
                    let frame = unsafe { self.frames.pop().unwrap_unchecked() };
                    let truncate_to = if frame.is_method_call {
                        frame.base_slot as usize
                    } else {
                        (frame.base_slot as usize).saturating_sub(1)
                    };
                    self.stack.truncate(truncate_to);
                    self.push_fast(return_value);
                    self.ip = frame.return_ip as usize;
                    self.current_base = self.frames.last().map(|f| f.base_slot as usize).unwrap_or(0);
                    continue;
                }
                OP_LOAD_LOCALS2 => {
                    // 一次加载两个局部变量
                    let slot1 = self.read_byte() as usize;
                    let slot2 = self.read_byte() as usize;
                    let actual1 = self.current_base + slot1;
                    let actual2 = self.current_base + slot2;
                    let v1 = unsafe { self.stack.get_unchecked(actual1).clone() };
                    let v2 = unsafe { self.stack.get_unchecked(actual2).clone() };
                    self.push_fast(v1);
                    self.push_fast(v2);
                    continue;
                }
                _ => {}
            }
            
            // 冷路径：使用 OpCode::from() 处理其他指令
            let opcode = OpCode::from(op);
            
            match opcode {
                OpCode::Const => {
                    let index = self.read_u16() as usize;
                    // SAFETY: 编译器保证索引在常量池范围内
                    let value = unsafe { self.chunk.constants.get_unchecked(index).clone() };
                    self.push_fast(value);
                }
                
                OpCode::Pop => {
                    self.pop()?;
                }
                
                OpCode::Add => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    // 整数快速路径
                    if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                        self.push_fast(Value::int(x + y));
                    } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                        self.push_fast(Value::float(x + y));
                    } else if let (Some(x), Some(y)) = (a.as_int(), b.as_float()) {
                        self.push_fast(Value::float(x as f64 + y));
                    } else if let (Some(x), Some(y)) = (a.as_float(), b.as_int()) {
                        self.push_fast(Value::float(x + y as f64));
                    } else if let (Some(s1), Some(s2)) = (a.as_string(), b.as_string()) {
                        self.push_fast(Value::string(format!("{}{}", s1, s2)));
                    } else if a.is_class() || a.is_struct() {
                        // 只对 Class/Struct 类型检查运算符重载
                        if let Some(result) = self.try_operator_overload(&a, &b, "add")? {
                            self.push_fast(result);
                        } else {
                    let result = (a + b).map_err(|e| self.runtime_error(&e))?;
                            self.push_fast(result);
                        }
                    } else {
                        let result = (a + b).map_err(|e| self.runtime_error(&e))?;
                        self.push_fast(result);
                    }
                }
                
                OpCode::Sub => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    // 整数快速路径
                    if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                        self.push_fast(Value::int(x - y));
                    } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                        self.push_fast(Value::float(x - y));
                    } else if let (Some(x), Some(y)) = (a.as_int(), b.as_float()) {
                        self.push_fast(Value::float(x as f64 - y));
                    } else if let (Some(x), Some(y)) = (a.as_float(), b.as_int()) {
                        self.push_fast(Value::float(x - y as f64));
                    } else if a.is_class() || a.is_struct() {
                        if let Some(result) = self.try_operator_overload(&a, &b, "sub")? {
                            self.push_fast(result);
                        } else {
                    let result = (a - b).map_err(|e| self.runtime_error(&e))?;
                            self.push_fast(result);
                        }
                    } else {
                        let result = (a - b).map_err(|e| self.runtime_error(&e))?;
                        self.push_fast(result);
                    }
                }
                
                OpCode::Mul => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    // 整数快速路径
                    if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                        self.push(Value::int(x * y));
                    } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                        self.push(Value::float(x * y));
                    } else if let (Some(x), Some(y)) = (a.as_int(), b.as_float()) {
                        self.push(Value::float(x as f64 * y));
                    } else if let (Some(x), Some(y)) = (a.as_float(), b.as_int()) {
                        self.push(Value::float(x * y as f64));
                    } else if a.is_class() || a.is_struct() {
                        if let Some(result) = self.try_operator_overload(&a, &b, "mul")? {
                            self.push(result);
                        } else {
                    let result = (a * b).map_err(|e| self.runtime_error(&e))?;
                    self.push(result);
                        }
                    } else {
                        let result = (a * b).map_err(|e| self.runtime_error(&e))?;
                        self.push(result);
                    }
                }
                
                OpCode::Div => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    // 整数快速路径
                    if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                        if y == 0 {
                            return Err(self.runtime_error("Division by zero"));
                        }
                        self.push(Value::int(x / y));
                    } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                        self.push(Value::float(x / y));
                    } else if let (Some(x), Some(y)) = (a.as_int(), b.as_float()) {
                        self.push(Value::float(x as f64 / y));
                    } else if let (Some(x), Some(y)) = (a.as_float(), b.as_int()) {
                        self.push(Value::float(x / y as f64));
                    } else if a.is_class() || a.is_struct() {
                        if let Some(result) = self.try_operator_overload(&a, &b, "div")? {
                            self.push(result);
                        } else {
                    let result = (a / b).map_err(|e| self.runtime_error(&e))?;
                    self.push(result);
                        }
                    } else {
                        let result = (a / b).map_err(|e| self.runtime_error(&e))?;
                        self.push(result);
                    }
                }
                
                OpCode::Mod => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    // 整数快速路径
                    if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                        if y == 0 {
                            return Err(self.runtime_error("Modulo by zero"));
                        }
                        self.push(Value::int(x % y));
                    } else {
                    let result = (a % b).map_err(|e| self.runtime_error(&e))?;
                    self.push(result);
                    }
                }
                
                OpCode::Pow => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    // 整数快速路径
                    if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                        if y >= 0 {
                            self.push(Value::int(x.pow(y as u32)));
                        } else {
                            self.push(Value::float((x as f64).powf(y as f64)));
                        }
                    } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                        self.push(Value::float(x.powf(y)));
                    } else if let (Some(x), Some(y)) = (a.as_int(), b.as_float()) {
                        self.push(Value::float((x as f64).powf(y)));
                    } else if let (Some(x), Some(y)) = (a.as_float(), b.as_int()) {
                        self.push(Value::float(x.powf(y as f64)));
                    } else {
                    let result = a.pow(b).map_err(|e| self.runtime_error(&e))?;
                    self.push(result);
                    }
                }
                
                OpCode::Neg => {
                    let a = self.pop()?;
                    // 整数快速路径
                    if let Some(x) = a.as_int() {
                        self.push(Value::int(-x));
                    } else if let Some(x) = a.as_float() {
                        self.push(Value::float(-x));
                    } else {
                    let result = (-a).map_err(|e| self.runtime_error(&e))?;
                    self.push(result);
                    }
                }
                
                OpCode::Eq => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    // 整数快速路径
                    if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                        self.push_fast(Value::bool(x == y));
                    } else if let (Some(x), Some(y)) = (a.as_bool(), b.as_bool()) {
                        self.push_fast(Value::bool(x == y));
                    } else {
                        self.push_fast(a.eq_value(&b));
                    }
                }
                
                OpCode::Ne => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    // 整数快速路径
                    if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                        self.push_fast(Value::bool(x != y));
                    } else if let (Some(x), Some(y)) = (a.as_bool(), b.as_bool()) {
                        self.push_fast(Value::bool(x != y));
                    } else {
                        self.push_fast(a.ne_value(&b));
                    }
                }
                
                OpCode::Lt => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    // 整数快速路径
                    if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                        self.push_fast(Value::bool(x < y));
                    } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                        self.push_fast(Value::bool(x < y));
                    } else {
                    let result = a.lt(&b).map_err(|e| self.runtime_error(&e))?;
                        self.push_fast(result);
                    }
                }
                
                OpCode::Le => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    // 整数快速路径
                    if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                        self.push_fast(Value::bool(x <= y));
                    } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                        self.push_fast(Value::bool(x <= y));
                    } else {
                    let result = a.le(&b).map_err(|e| self.runtime_error(&e))?;
                        self.push_fast(result);
                    }
                }
                
                OpCode::Gt => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    // 整数快速路径
                    if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                        self.push_fast(Value::bool(x > y));
                    } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                        self.push_fast(Value::bool(x > y));
                    } else {
                    let result = a.gt(&b).map_err(|e| self.runtime_error(&e))?;
                        self.push_fast(result);
                    }
                }
                
                OpCode::Ge => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    // 整数快速路径
                    if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                        self.push_fast(Value::bool(x >= y));
                    } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                        self.push_fast(Value::bool(x >= y));
                    } else {
                    let result = a.ge(&b).map_err(|e| self.runtime_error(&e))?;
                        self.push_fast(result);
                    }
                }
                
                OpCode::Not => {
                    let a = self.pop()?;
                    self.push(a.not());
                }
                
                OpCode::BitAnd => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    let result = a.bit_and(&b).map_err(|e| self.runtime_error(&e))?;
                    self.push(result);
                }
                
                OpCode::BitOr => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    let result = a.bit_or(&b).map_err(|e| self.runtime_error(&e))?;
                    self.push(result);
                }
                
                OpCode::BitXor => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    let result = a.bit_xor(&b).map_err(|e| self.runtime_error(&e))?;
                    self.push(result);
                }
                
                OpCode::BitNot => {
                    let a = self.pop()?;
                    let result = a.bit_not().map_err(|e| self.runtime_error(&e))?;
                    self.push(result);
                }
                
                OpCode::Shl => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    let result = a.shl(&b).map_err(|e| self.runtime_error(&e))?;
                    self.push(result);
                }
                
                OpCode::Shr => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    let result = a.shr(&b).map_err(|e| self.runtime_error(&e))?;
                    self.push(result);
                }
                
                OpCode::Print => {
                    let value = self.pop()?;
                    print!("{}", value);
                }
                
                OpCode::PrintLn => {
                    let value = self.pop()?;
                    println!("{}", value);
                }
                
                OpCode::TypeOf => {
                    let value = self.pop()?;
                    let type_name = value.type_name();
                    self.push(Value::string(type_name.to_string()));
                }
                
                OpCode::TypeInfo => {
                    use super::value::{RuntimeTypeInfoData, TypeKind, FieldInfo, MethodInfo};
                    
                    let value = self.pop()?;
                    
                    // 根据值的类型创建对应的类型信息
                    let type_info = if value.is_null() {
                        RuntimeTypeInfoData::primitive("null")
                    } else if value.is_bool() {
                        RuntimeTypeInfoData::primitive("bool")
                    } else if value.is_int() {
                        RuntimeTypeInfoData::primitive("int")
                    } else if value.is_float() {
                        RuntimeTypeInfoData::primitive("float")
                    } else if value.is_char() {
                        RuntimeTypeInfoData::primitive("char")
                    } else if value.as_string().is_some() {
                        RuntimeTypeInfoData::primitive("string")
                    } else if value.is_function() {
                        let mut info = RuntimeTypeInfoData::unknown();
                        info.name = "function".to_string();
                        info.kind = TypeKind::Function;
                        
                        // 如果能获取函数信息，添加更多细节
                        if let Some(func) = value.as_function() {
                            if let Some(name) = &func.name {
                                info.name = name.clone();
                            }
                            // 记录参数数量作为一个"方法"
                            info.methods.push(MethodInfo {
                                name: "(call)".to_string(),
                                param_types: vec!["any".to_string(); func.arity],
                                return_type: "any".to_string(),
                                is_public: true,
                                is_static: false,
                                is_abstract: false,
                            });
                        }
                        info
                    } else if value.as_array().is_some() {
                        let element_info = RuntimeTypeInfoData::primitive("any");
                        RuntimeTypeInfoData::array(element_info)
                    } else if value.as_map().is_some() {
                        let mut info = RuntimeTypeInfoData::unknown();
                        info.name = "map".to_string();
                        info.kind = TypeKind::Map;
                        info.key_type = Some(Box::new(RuntimeTypeInfoData::primitive("any")));
                        info.value_type = Some(Box::new(RuntimeTypeInfoData::primitive("any")));
                        info
                    } else if value.as_set().is_some() {
                        let mut info = RuntimeTypeInfoData::unknown();
                        info.name = "set".to_string();
                        info.kind = TypeKind::Set;
                        info.element_type = Some(Box::new(RuntimeTypeInfoData::primitive("any")));
                        info
                    } else if let Some(s) = value.as_struct() {
                        let s = s.lock();
                        let mut info = RuntimeTypeInfoData::unknown();
                        info.name = s.type_name.clone();
                        info.kind = TypeKind::Struct;
                        info.fields = s.fields.keys().map(|k| FieldInfo {
                            name: k.clone(),
                            type_name: "any".to_string(),
                            is_public: true,
                            is_static: false,
                            is_const: false,
                        }).collect();
                        info
                    } else if let Some(c) = value.as_class() {
                        let c = c.lock();
                        let mut info = RuntimeTypeInfoData::unknown();
                        info.name = c.class_name.clone();
                        info.kind = TypeKind::Class;
                        info.parent = c.parent_class.clone();
                        info.fields = c.fields.keys().map(|k| FieldInfo {
                            name: k.clone(),
                            type_name: "any".to_string(),
                            is_public: true,
                            is_static: false,
                            is_const: false,
                        }).collect();
                        info
                    } else if let Some(e) = value.as_enum() {
                        let mut info = RuntimeTypeInfoData::unknown();
                        info.name = format!("{}::{}", e.enum_name, e.variant_name);
                        info.kind = TypeKind::Enum;
                        info.fields = e.associated_data.keys().map(|k| FieldInfo {
                            name: k.clone(),
                            type_name: "any".to_string(),
                            is_public: true,
                            is_static: false,
                            is_const: false,
                        }).collect();
                        info
                    } else if value.is_channel() {
                        let mut info = RuntimeTypeInfoData::unknown();
                        info.name = "channel".to_string();
                        info.kind = TypeKind::Primitive; // 通道作为原始类型处理
                        info
                    } else if value.is_mutex() {
                        let mut info = RuntimeTypeInfoData::unknown();
                        info.name = "mutex".to_string();
                        info.kind = TypeKind::Primitive;
                        info
                    } else if value.is_waitgroup() {
                        let mut info = RuntimeTypeInfoData::unknown();
                        info.name = "waitgroup".to_string();
                        info.kind = TypeKind::Primitive;
                        info
                    } else if let Some(t) = value.as_type_ref() {
                        let mut info = RuntimeTypeInfoData::unknown();
                        info.name = t.clone();
                        info.kind = TypeKind::Alias;
                        info
                    } else if let Some(ti) = value.as_runtime_type_info() {
                        // 如果已经是类型信息，直接克隆返回
                        ti.clone()
                    } else {
                        RuntimeTypeInfoData::unknown()
                    };
                    
                    self.push(Value::runtime_type_info(type_info));
                }
                
                OpCode::SizeOf => {
                    let value = self.pop()?;
                    let size = if value.is_null() {
                        0
                    } else if value.is_bool() {
                        1
                    } else if value.is_int() {
                        8
                    } else if value.is_float() {
                        8
                    } else if value.is_char() {
                        4
                    } else if let Some(s) = value.as_string() {
                        s.len() as i64
                    } else if value.is_function() {
                        0 // 函数大小不适用
                    } else if let Some(arr) = value.as_array() {
                        arr.lock().len() as i64
                    } else if let Some(m) = value.as_map() {
                        m.lock().len() as i64
                    } else if value.is_range() {
                        24 // 两个i64 + bool
                    } else if value.is_iterator() {
                        0 // 迭代器大小不适用
                    } else if let Some(s) = value.as_struct() {
                        s.lock().fields.len() as i64 // 字段数量
                    } else if let Some(c) = value.as_class() {
                        c.lock().fields.len() as i64 // 字段数量
                    } else if let Some(e) = value.as_enum() {
                        e.associated_data.len() as i64 // 关联数据字段数量
                    } else {
                        0 // 类型引用本身没有大小
                    };
                    self.push(Value::int(size));
                }
                
                OpCode::Time => {
                    // 获取当前时间戳（毫秒）
                    // [deprecated] 可能在未来版本移除
                    use std::time::{SystemTime, UNIX_EPOCH};
                    let timestamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    self.push(Value::int(timestamp));
                }
                
                OpCode::Panic => {
                    let value = self.pop()?;
                    return Err(self.runtime_error(&format!("Panic: {}", value)));
                }
                
                OpCode::ToString => {
                    let value = self.pop()?;
                    let string_value = if let Some(s) = value.as_string() {
                        s.clone()
                    } else if let Some(n) = value.as_int() {
                        n.to_string()
                    } else if let Some(f) = value.as_float() {
                        f.to_string()
                    } else if let Some(b) = value.as_bool() {
                        b.to_string()
                    } else if let Some(c) = value.as_char() {
                        c.to_string()
                    } else if value.is_null() {
                        "null".to_string()
                    } else if let Some(arr) = value.as_array() {
                        format!("{:?}", arr.lock())
                    } else if let Some(m) = value.as_map() {
                        format!("{:?}", m.lock())
                    } else if let Some(s) = value.as_struct() {
                        format!("struct {}{{...}}", s.lock().type_name)
                    } else if let Some(c) = value.as_class() {
                        format!("class {}{{...}}", c.lock().class_name)
                    } else if let Some(e) = value.as_enum() {
                        format!("{}::{}", e.enum_name, e.variant_name)
                    } else {
                        format!("{}", value)
                    };
                    self.push(Value::string(string_value));
                }
                
                OpCode::CastSafe => {
                    let type_name_index = self.read_u16() as usize;
                    let type_name = if let Some(s) = self.chunk.constants[type_name_index].as_string() {
                        s.clone()
                    } else {
                        return Err(self.runtime_error("Invalid type name"));
                    };
                    
                    let value = self.pop()?;
                    let result = self.try_cast_value(value, &type_name);
                    self.push(result);
                }
                
                OpCode::CastForce => {
                    let type_name_index = self.read_u16() as usize;
                    let type_name = if let Some(s) = self.chunk.constants[type_name_index].as_string() {
                        s.clone()
                    } else {
                        return Err(self.runtime_error("Invalid type name"));
                    };
                    
                    let value = self.pop()?;
                    let result = self.try_cast_value(value.clone(), &type_name);
                    if result.is_null() {
                        return Err(self.runtime_error(&format!(
                            "Cannot cast {} to {}",
                            value.type_name(),
                            type_name
                        )));
                    } else {
                        self.push(result);
                    }
                }
                
                OpCode::TypeCheck => {
                    let type_name_index = self.read_u16() as usize;
                    let type_name = if let Some(s) = self.chunk.constants[type_name_index].as_string() {
                        s.clone()
                    } else {
                        return Err(self.runtime_error("Invalid type name"));
                    };
                    
                    let value = self.pop()?;
                    let is_type = self.check_value_type(&value, &type_name);
                    self.push(Value::bool(is_type));
                }
                
                OpCode::NewArray => {
                    let count = self.read_u16() as usize;
                    let mut elements = Vec::with_capacity(count);
                    // 从栈上弹出元素（逆序，因为先压入的在栈底）
                    for _ in 0..count {
                        elements.push(self.pop()?);
                    }
                    elements.reverse();
                    self.push(Value::array(Arc::new(Mutex::new(elements))));
                }
                
                OpCode::NewMap => {
                    let count = self.read_u16() as usize;
                    let mut map = std::collections::HashMap::with_capacity(count);
                    // 从栈上弹出键值对（逆序）
                    let mut pairs = Vec::with_capacity(count);
                    for _ in 0..count {
                        let value = self.pop()?;
                        let key = self.pop()?;
                        pairs.push((key, value));
                    }
                    // 按正确顺序插入
                    for (key, value) in pairs.into_iter().rev() {
                        let key_str = if let Some(s) = key.as_string() {
                            s.clone()
                        } else {
                            return Err(self.runtime_error(&format!(
                                "Map key must be string, got {}",
                                key.type_name()
                            )));
                        };
                        map.insert(key_str, value);
                    }
                    self.push(Value::map(Arc::new(Mutex::new(map))));
                }
                
                OpCode::NewSet => {
                    // 创建新 Set
                    let count = self.read_u16() as usize;
                    let mut elements = Vec::with_capacity(count);
                    // 从栈上弹出元素
                    for _ in 0..count {
                        let value = self.pop()?;
                        // 检查是否已存在（Set 去重）
                        if !elements.iter().any(|v| v == &value) {
                            elements.push(value);
                        }
                    }
                    elements.reverse(); // 恢复正确顺序
                    self.push(Value::set(Arc::new(Mutex::new(elements))));
                }
                
                OpCode::SetAdd => {
                    // Set 添加元素
                    let value = self.pop()?;
                    let set_val = self.pop()?;
                    if let Some(set) = set_val.as_set() {
                        let mut set = set.lock();
                        // 检查是否已存在
                        if !set.iter().any(|v| v == &value) {
                            set.push(value);
                        }
                        drop(set);
                        self.push(set_val);
                    } else {
                        return Err(self.runtime_error(&format!(
                            "Cannot call add on non-set type: {}",
                            set_val.type_name()
                        )));
                    }
                }
                
                OpCode::SetContains => {
                    // Set 包含检查
                    let value = self.pop()?;
                    let set_val = self.pop()?;
                    if let Some(set) = set_val.as_set() {
                        let set = set.lock();
                        let contains = set.iter().any(|v| v == &value);
                        self.push(Value::bool(contains));
                    } else {
                        return Err(self.runtime_error(&format!(
                            "Cannot call contains on non-set type: {}",
                            set_val.type_name()
                        )));
                    }
                }
                
                OpCode::SetRemove => {
                    // Set 移除元素
                    let value = self.pop()?;
                    let set_val = self.pop()?;
                    if let Some(set) = set_val.as_set() {
                        let mut set = set.lock();
                        let pos = set.iter().position(|v| v == &value);
                        let removed = if let Some(idx) = pos {
                            set.remove(idx);
                            true
                        } else {
                            false
                        };
                        self.push(Value::bool(removed));
                    } else {
                        return Err(self.runtime_error(&format!(
                            "Cannot call remove on non-set type: {}",
                            set_val.type_name()
                        )));
                    }
                }
                
                OpCode::SetSize => {
                    // Set 大小
                    let set_val = self.pop()?;
                    if let Some(set) = set_val.as_set() {
                        let set = set.lock();
                        self.push(Value::int(set.len() as i64));
                    } else {
                        return Err(self.runtime_error(&format!(
                            "Cannot get size of non-set type: {}",
                            set_val.type_name()
                        )));
                    }
                }
                
                OpCode::ArraySlice => {
                    // 创建数组切片
                    let end = self.pop()?.as_int().unwrap_or(0) as usize;
                    let start = self.pop()?.as_int().unwrap_or(0) as usize;
                    let array_val = self.pop()?;
                    
                    if let Some(arr) = array_val.as_array() {
                        let arr_len = arr.lock().len();
                        // 边界检查
                        let actual_start = start.min(arr_len);
                        let actual_end = end.min(arr_len);
                        if actual_start <= actual_end {
                            self.push(Value::array_slice(arr.clone(), actual_start, actual_end));
                        } else {
                            // 无效范围，返回空数组
                            self.push(Value::array(Arc::new(Mutex::new(Vec::new()))));
                        }
                    } else if let Some((source, slice_start, slice_end)) = array_val.as_array_slice() {
                        // 切片的切片
                        let actual_start = (slice_start + start).min(slice_end);
                        let actual_end = (slice_start + end).min(slice_end);
                        if actual_start <= actual_end {
                            self.push(Value::array_slice(source.clone(), actual_start, actual_end));
                        } else {
                            self.push(Value::array(Arc::new(Mutex::new(Vec::new()))));
                        }
                    } else {
                        return Err(self.runtime_error(&format!(
                            "Cannot slice non-array type: {}",
                            array_val.type_name()
                        )));
                    }
                }
                
                OpCode::GetIndex => {
                    let index = self.pop()?;
                    let object = self.pop()?;
                    
                    if let (Some(arr), Some(i)) = (object.as_array(), index.as_int()) {
                            let arr = arr.lock();
                        let idx = if i < 0 {
                            (arr.len() as i64 + i) as usize
                            } else {
                            i as usize
                            };
                            if idx >= arr.len() {
                                return Err(self.runtime_error(&format!(
                                    "Index {} out of bounds for array of length {}", i, arr.len()
                                )));
                            }
                            self.push(arr[idx].clone());
                    } else if let (Some(s), Some(i)) = (object.as_string(), index.as_int()) {
                        let idx = if i < 0 {
                            (s.len() as i64 + i) as usize
                            } else {
                            i as usize
                            };
                            if let Some(c) = s.chars().nth(idx) {
                            self.push(Value::char(c));
                            } else {
                                return Err(self.runtime_error(&format!(
                                    "Index {} out of bounds for string of length {}", i, s.len()
                                )));
                            }
                    } else if let (Some(m), Some(key)) = (object.as_map(), index.as_string()) {
                        let m = m.lock();
                        if let Some(v) = m.get(key) {
                            self.push(v.clone());
                        } else {
                            self.push(Value::null());
                        }
                    } else {
                            return Err(self.runtime_error(&format!(
                                "Cannot index {} with {}", object.type_name(), index.type_name()
                            )));
                    }
                }
                
                OpCode::SetIndex => {
                    let value = self.pop()?;
                    let index = self.pop()?;
                    let object = self.pop()?;
                    
                    if let (Some(arr), Some(i)) = (object.as_array(), index.as_int()) {
                            let mut arr = arr.lock();
                        let idx = if i < 0 {
                            (arr.len() as i64 + i) as usize
                            } else {
                            i as usize
                            };
                            if idx >= arr.len() {
                                return Err(self.runtime_error(&format!(
                                    "Index {} out of bounds for array of length {}", i, arr.len()
                                )));
                            }
                        arr[idx] = value;
                            self.push(value);
                    } else if let (Some(m), Some(key)) = (object.as_map(), index.as_string()) {
                        let mut m = m.lock();
                        m.insert(key.clone(), value);
                        self.push(value);
                    } else {
                            return Err(self.runtime_error(&format!(
                                "Cannot set index on {}", object.type_name()
                            )));
                    }
                }
                
                OpCode::NewRange => {
                    let end = self.pop()?;
                    let start = self.pop()?;
                    
                    if let (Some(s), Some(e)) = (start.as_int(), end.as_int()) {
                        self.push(Value::range(s, e, false));
                    } else {
                            return Err(self.runtime_error(&format!(
                                "Range requires integer bounds, got {} and {}",
                                start.type_name(), end.type_name()
                            )));
                    }
                }
                
                OpCode::NewRangeInclusive => {
                    let end = self.pop()?;
                    let start = self.pop()?;
                    
                    if let (Some(s), Some(e)) = (start.as_int(), end.as_int()) {
                        self.push(Value::range(s, e, true));
                    } else {
                            return Err(self.runtime_error(&format!(
                                "Range requires integer bounds, got {} and {}",
                                start.type_name(), end.type_name()
                            )));
                    }
                }
                
                OpCode::IterInit => {
                    let iterable = self.pop()?;
                    
                    let iter = if let Some(arr) = iterable.as_array() {
                            Iterator {
                            source: IteratorSource::Array(arr.clone()),
                                index: 0,
                            }
                    } else if let Some((start, end, inclusive)) = iterable.as_range() {
                            Iterator {
                                source: IteratorSource::Range(start, end, inclusive),
                                index: 0,
                            }
                    } else {
                            return Err(self.runtime_error(&format!(
                                "Cannot iterate over {}",
                                iterable.type_name()
                            )));
                    };
                    self.push(Value::iterator(Arc::new(Mutex::new(iter))));
                }
                
                OpCode::IterNext => {
                    // 获取迭代器但不弹出
                    let iter_val = self.peek()?.clone();
                    
                    if let Some(iter_rc) = iter_val.as_iterator() {
                        // 先获取当前索引和源的克隆
                        let (index, source_clone) = {
                            let iter = iter_rc.lock();
                            (iter.index, iter.source.clone())
                        };
                        
                        let (value, has_more) = match source_clone {
                            IteratorSource::Array(arr) => {
                                let arr = arr.lock();
                                if index < arr.len() {
                                    let value = arr[index].clone();
                                    (value, true)
                                } else {
                                    (Value::null(), false)
                                }
                            }
                            IteratorSource::Range(start, end, inclusive) => {
                                let current = start + index as i64;
                                let has_more = if inclusive {
                                    current <= end
                                } else {
                                    current < end
                                };
                                
                                if has_more {
                                    (Value::int(current), true)
                                } else {
                                    (Value::null(), false)
                                }
                            }
                        };
                        
                        // 如果有下一个元素，更新索引
                        if has_more {
                            iter_rc.lock().index += 1;
                        }
                        
                        self.push(value);
                        self.push(Value::bool(has_more));
                    } else {
                        return Err(self.runtime_error("IterNext requires an iterator"));
                    }
                }
                
                OpCode::GetLocal => {
                    let slot = self.read_u16() as usize;
                    // 使用缓存的栈基址，无边界检查
                    let actual_slot = self.current_base + slot;
                    // SAFETY: 编译器保证 slot 在有效范围内
                    let value = unsafe { self.stack.get_unchecked(actual_slot).clone() };
                    self.push_fast(value);
                }
                
                OpCode::SetLocal => {
                    let slot = self.read_u16() as usize;
                    let value = self.peek()?.clone();
                    // 使用缓存的栈基址
                    let actual_slot = self.current_base + slot;
                    self.stack[actual_slot] = value;
                }
                
                OpCode::GetUpvalue => {
                    let _index = self.read_u16() as usize;
                    // TODO: 实现完整的 upvalue 支持
                    self.push_fast(Value::null());
                }
                
                OpCode::SetUpvalue => {
                    let _index = self.read_u16() as usize;
                    // TODO: 实现完整的 upvalue 支持
                }
                
                OpCode::CloseUpvalue => {
                    let _slot = self.read_u16() as usize;
                    // TODO: 实现完整的 upvalue 关闭
                }
                
                OpCode::Jump => {
                    let offset = self.read_u16() as usize;
                    self.ip += offset;
                }
                
                OpCode::JumpIfFalse => {
                    let offset = self.read_u16() as usize;
                    // SAFETY: peek 在非空栈上调用
                    let top = unsafe { self.stack.last().unwrap_unchecked() };
                    if !top.is_truthy() {
                        self.ip += offset;
                    }
                }
                
                OpCode::JumpIfTrue => {
                    let offset = self.read_u16() as usize;
                    // SAFETY: peek 在非空栈上调用
                    let top = unsafe { self.stack.last().unwrap_unchecked() };
                    if top.is_truthy() {
                        self.ip += offset;
                    }
                }
                
                OpCode::Loop => {
                    // 安全点：向后跳转（循环）是抢占检查点
                    if self.should_preempt() {
                        // 可以在这里让出 CPU，但对于单线程 VM 我们只是清除标志
                        self.clear_preempt();
                    }
                    let offset = self.read_u16() as usize;
                    self.ip -= offset;
                }
                
                OpCode::Closure => {
                    // Closure 指令已被替换为直接加载函数常量
                    // 这个分支不应该被执行
                    let _func_index = self.read_u16();
                    let msg = "Unexpected Closure opcode";
                    return Err(self.runtime_error(msg));
                }
                
                OpCode::Call => {
                    let arg_count = self.read_byte() as usize;
                    
                    // 获取被调用的函数（在参数下方）
                    let callee_idx = self.stack.len() - arg_count - 1;
                    let callee = self.stack[callee_idx].clone();
                    
                    if let Some(func) = callee.as_function() {
                        // 快速路径：简单函数调用（参数数量匹配，无默认值，无可变参数）
                        if !func.has_variadic && func.defaults.is_empty() && arg_count == func.arity {
                            // 检查调用深度
                            if self.frames.len() >= MAX_FRAMES {
                                let msg = "Stack overflow: too many nested function calls";
                                return Err(self.runtime_error(msg));
                            }
                            
                            // 创建调用帧
                            let base_slot = callee_idx + 1;
                            self.frames.push(CallFrame {
                                return_ip: self.ip as u32,
                                base_slot: base_slot as u16,
                                is_method_call: false,
                            });
                            
                            // 更新缓存的栈基址
                            self.current_base = base_slot;
                            
                            // 跳转到函数体
                            self.ip = func.chunk_index;
                        } else {
                            // 慢速路径：处理默认参数和可变参数
                        let fixed_params = if func.has_variadic { func.arity - 1 } else { func.arity };
                        
                        // 检查必需参数数量
                        if arg_count < func.required_params {
                            let msg = format!(
                                "Expected at least {} arguments but got {}",
                                func.required_params, arg_count
                            );
                            return Err(self.runtime_error(&msg));
                        }
                        
                        // 如果没有可变参数，检查参数上限
                        if !func.has_variadic && arg_count > func.arity {
                            let msg = format!(
                                "Expected at most {} arguments but got {}",
                                func.arity, arg_count
                            );
                            return Err(self.runtime_error(&msg));
                        }
                        
                            // 处理可变参数
                        if func.has_variadic {
                            let variadic_count = if arg_count > fixed_params {
                                arg_count - fixed_params
                            } else {
                                0
                            };
                            
                            let mut variadic_args = Vec::with_capacity(variadic_count);
                            for _ in 0..variadic_count {
                                variadic_args.push(self.pop()?);
                            }
                                variadic_args.reverse();
                            
                                self.push(Value::array(Arc::new(Mutex::new(variadic_args))));
                        }
                        
                            // 填充缺失的默认参数
                        let current_fixed_args = if func.has_variadic {
                            std::cmp::min(arg_count, fixed_params)
                        } else {
                            arg_count
                        };
                        let missing_count = fixed_params - current_fixed_args;
                        if missing_count > 0 {
                            let defaults_start = func.defaults.len() - missing_count;
                            if func.has_variadic {
                                let variadic_array = self.pop()?;
                                for i in 0..missing_count {
                                    self.push(func.defaults[defaults_start + i].clone());
                                }
                                self.push(variadic_array);
                            } else {
                                for i in 0..missing_count {
                                    self.push(func.defaults[defaults_start + i].clone());
                                }
                            }
                        }
                        
                        // 检查调用深度
                        if self.frames.len() >= MAX_FRAMES {
                            let msg = "Stack overflow: too many nested function calls";
                            return Err(self.runtime_error(msg));
                        }
                        
                        // 创建调用帧
                            let base_slot = callee_idx + 1;
                            self.frames.push(CallFrame {
                                return_ip: self.ip as u32,
                                base_slot: base_slot as u16,
                                is_method_call: false,
                            });
                            
                            // 更新缓存的栈基址
                            self.current_base = base_slot;
                        
                        // 跳转到函数体
                        self.ip = func.chunk_index;
                        }
                    } else {
                        let msg = format!("Cannot call {}", callee.type_name());
                        return Err(self.runtime_error(&msg));
                    }
                }
                
                OpCode::Return => {
                    // 获取返回值
                    let return_value = self.pop_fast();
                    
                    // 如果没有调用帧，说明是顶层返回
                    if self.frames.is_empty() {
                        // 压回返回值（供外部使用）
                        self.push_fast(return_value);
                        return Ok(());
                    }
                    
                    // 弹出调用帧
                    let frame = unsafe { self.frames.pop().unwrap_unchecked() };
                    
                    // 清理栈：移除函数值/receiver 和所有局部变量
                    // 对于方法调用：base_slot 指向 receiver，截断到 base_slot
                    // 对于函数调用：base_slot 指向第一个参数，截断到 base_slot - 1（移除函数值）
                    let truncate_to = if frame.is_method_call {
                        frame.base_slot as usize
                    } else {
                        (frame.base_slot as usize).saturating_sub(1)
                    };
                    self.stack.truncate(truncate_to);
                    
                    // 压入返回值
                    self.push_fast(return_value);
                    
                    // 恢复指令指针
                    self.ip = frame.return_ip as usize;
                    
                    // 更新缓存的栈基址
                    self.current_base = self.frames.last().map(|f| f.base_slot as usize).unwrap_or(0);
                }
                
                OpCode::NewStruct => {
                    let field_count = self.read_byte() as usize;
                    let type_name_index = self.read_u16() as usize;
                    
                    // 从常量池获取类型名称
                    let type_name = if let Some(s) = self.chunk.constants[type_name_index].as_string() {
                        s.clone()
                    } else {
                        return Err(self.runtime_error("Invalid struct type name"));
                    };
                    
                    // 从栈中弹出字段（逆序，因为先压入的在栈底）
                    let mut fields = std::collections::HashMap::new();
                    for _ in 0..field_count {
                        let value = self.pop()?;
                        let field_name_val = self.pop()?;
                        let field_name = if let Some(s) = field_name_val.as_string() {
                            s.clone()
                        } else {
                            return Err(self.runtime_error("Invalid field name"));
                        };
                        fields.insert(field_name, value);
                    }
                    
                    // 创建 struct 实例
                    let instance = StructInstance { type_name, fields };
                    self.push(Value::struct_val(Arc::new(Mutex::new(instance))));
                }
                
                OpCode::GetField => {
                    let field_name_index = self.read_u16() as usize;
                    let field_name = if let Some(s) = self.chunk.constants[field_name_index].as_string() {
                        s.clone()
                    } else {
                        return Err(self.runtime_error("Invalid field name"));
                    };
                    
                    let obj_val = self.pop()?;
                    if let Some(s) = obj_val.as_struct() {
                        let s = s.lock();
                        if let Some(value) = s.fields.get(&field_name) {
                            self.push(value.clone());
                        } else {
                            return Err(self.runtime_error(&format!(
                                "Struct '{}' has no field '{}'",
                                s.type_name, field_name
                            )));
                        }
                    } else if let Some(c) = obj_val.as_class() {
                        let c = c.lock();
                        if let Some(value) = c.fields.get(&field_name) {
                            self.push(value.clone());
                        } else {
                            return Err(self.runtime_error(&format!(
                                "Class '{}' has no field '{}'",
                                c.class_name, field_name
                            )));
                        }
                    } else if let Some(e) = obj_val.as_enum() {
                        // 枚举字段访问
                        if field_name == "value" {
                            // .value 属性：返回枚举关联值
                            if let Some(ref val) = e.value {
                                self.push(val.clone());
                            } else {
                                self.push(Value::null());
                            }
                        } else if field_name == "name" {
                            // .name 属性：返回变体名
                            self.push(Value::string(e.variant_name.clone()));
                        } else if let Some(value) = e.associated_data.get(&field_name) {
                            // 关联数据字段
                            self.push(value.clone());
                        } else {
                            return Err(self.runtime_error(&format!(
                                "Enum '{}::{}' has no field '{}'",
                                e.enum_name, e.variant_name, field_name
                            )));
                        }
                    } else {
                        return Err(self.runtime_error(&format!(
                            "Cannot access field '{}' on {}",
                            field_name,
                            obj_val.type_name()
                        )));
                    }
                }
                
                OpCode::SetField => {
                    let field_name_index = self.read_u16() as usize;
                    let field_name = if let Some(s) = self.chunk.constants[field_name_index].as_string() {
                        s.clone()
                    } else {
                        return Err(self.runtime_error("Invalid field name"));
                    };
                    
                    let value = self.pop()?;
                    let obj_val = self.peek()?.clone(); // 保留对象在栈上
                    if let Some(s) = obj_val.as_struct() {
                            let mut s = s.lock();
                            if s.fields.contains_key(&field_name) {
                                s.fields.insert(field_name, value);
                            } else {
                                return Err(self.runtime_error(&format!(
                                    "Struct '{}' has no field '{}'",
                                    s.type_name, field_name
                                )));
                            }
                    } else if let Some(c) = obj_val.as_class() {
                        let mut c = c.lock();
                        // 对于 class，允许设置已定义的字段或新字段
                        c.fields.insert(field_name, value);
                    } else {
                        return Err(self.runtime_error(&format!(
                            "Cannot set field '{}' on {}",
                            field_name,
                            obj_val.type_name()
                        )));
                    }
                }
                
                OpCode::JumpIfNull => {
                    let offset = self.read_u16() as usize;
                    let value = self.peek()?;
                    if value.is_null() {
                        // 只跳转，不弹出（由后续指令处理）
                        self.ip += offset;
                    }
                }
                
                OpCode::SafeGetField => {
                    let field_name_index = self.read_u16() as usize;
                    let field_name = if let Some(s) = self.chunk.constants[field_name_index].as_string() {
                        s.clone()
                    } else {
                        return Err(self.runtime_error("Invalid field name"));
                    };
                    
                    let obj_val = self.pop()?;
                    // 如果对象为 null，直接返回 null
                    if obj_val.is_null() {
                        self.push(Value::null());
                    } else if let Some(s) = obj_val.as_struct() {
                        let s = s.lock();
                        if let Some(value) = s.fields.get(&field_name) {
                            self.push(value.clone());
                        } else {
                            self.push(Value::null());
                        }
                    } else if let Some(c) = obj_val.as_class() {
                        let c = c.lock();
                        if let Some(value) = c.fields.get(&field_name) {
                            self.push(value.clone());
                        } else {
                            self.push(Value::null());
                        }
                    } else {
                        self.push(Value::null());
                    }
                }
                
                OpCode::NonNullGetField => {
                    let field_name_index = self.read_u16() as usize;
                    let field_name = if let Some(s) = self.chunk.constants[field_name_index].as_string() {
                        s.clone()
                    } else {
                        return Err(self.runtime_error("Invalid field name"));
                    };
                    
                    let obj_val = self.pop()?;
                    // 如果对象为 null，触发 panic
                    if obj_val.is_null() {
                        return Err(self.runtime_error("Non-null assertion failed: value is null"));
                    }
                    if let Some(s) = obj_val.as_struct() {
                        let s = s.lock();
                        if let Some(value) = s.fields.get(&field_name) {
                            self.push(value.clone());
                        } else {
                            return Err(self.runtime_error(&format!(
                                "Struct '{}' has no field '{}'",
                                s.type_name, field_name
                            )));
                        }
                    } else if let Some(c) = obj_val.as_class() {
                        let c = c.lock();
                        if let Some(value) = c.fields.get(&field_name) {
                            self.push(value.clone());
                        } else {
                            return Err(self.runtime_error(&format!(
                                "Class '{}' has no field '{}'",
                                c.class_name, field_name
                            )));
                        }
                    } else {
                        return Err(self.runtime_error(&format!(
                            "Cannot access field '{}' on {}",
                            field_name,
                            obj_val.type_name()
                        )));
                    }
                }
                
                OpCode::SafeInvokeMethod => {
                    let method_name_index = self.read_u16() as usize;
                    let arg_count = self.read_byte() as usize;
                    
                    // 获取方法名
                    let method_name = if let Some(s) = self.chunk.constants[method_name_index].as_string() {
                        s.clone()
                    } else {
                        return Err(self.runtime_error("Invalid method name"));
                    };
                    
                    // 获取 receiver（在参数下方）
                    let receiver_idx = self.stack.len() - arg_count - 1;
                    let receiver = self.stack[receiver_idx].clone();
                    
                    // 如果 receiver 为 null，弹出参数并返回 null
                    if receiver.is_null() {
                        // 弹出所有参数和 receiver
                        self.stack.truncate(receiver_idx);
                        // 压入 null
                        self.push(Value::null());
                        continue;
                    }
                    
                    // 检查是否是标准库类实例
                    if let Some(class_instance) = receiver.as_class() {
                        let instance_guard = class_instance.lock();
                        let class_name = instance_guard.class_name.clone();
                        drop(instance_guard);
                        
                        let registry = get_stdlib_registry();
                        if let Some((_, _)) = registry.find_class_module(&class_name) {
                            // 是标准库类实例，从栈中获取参数
                            let args_start = receiver_idx + 1;
                            let args = self.stack[args_start..].to_vec();
                            // 弹出参数和receiver
                            self.stack.truncate(receiver_idx);
                            
                            // 调用标准库方法
                            match registry.call_class_method(&receiver, &method_name, &args) {
                                Ok(result) => {
                                    self.push(result);
                                    continue;
                                }
                                Err(e) => {
                                    return Err(self.runtime_error(&e));
                                }
                            }
                        }
                    }
                    
                    // 否则执行类/结构体方法调用
                    if let Some(instance) = receiver.as_class() {
                        let instance = instance.lock();
                        let class_name = instance.class_name.clone();
                        drop(instance);
                        
                        if let Some(type_info) = self.chunk.get_type(&class_name).cloned() {
                            if let Some(&method_index) = type_info.methods.get(&method_name) {
                                let method_index = method_index as usize;
                                if let Some(func) = self.chunk.constants[method_index].as_function() {
                                    let func = func.clone();
                                    
                                    // 将 receiver 移到参数前面作为 this
                                    let this_slot = receiver_idx;
                                    
                                    // 检查参数数量
                                    if arg_count < func.required_params.saturating_sub(1) {
                                        let msg = format!(
                                            "Method '{}' expected at least {} arguments but got {}",
                                            method_name, func.required_params.saturating_sub(1), arg_count
                                        );
                                        return Err(self.runtime_error(&msg));
                                    }
                                    
                                    // 填充默认参数
                                    let missing = func.arity.saturating_sub(arg_count + 1);
                                    if missing > 0 && !func.defaults.is_empty() {
                                        let start = func.defaults.len().saturating_sub(missing);
                                        for i in 0..missing {
                                            if start + i < func.defaults.len() {
                                                self.push(func.defaults[start + i].clone());
                                            }
                                        }
                                    }
                                    
                                    if self.frames.len() >= MAX_FRAMES {
                                        return Err(self.runtime_error("Stack overflow"));
                                    }
                                    
                                    let frame = CallFrame {
                                        return_ip: self.ip as u32,
                                        base_slot: this_slot as u16,
                                        is_method_call: true,
                                    };
                                    self.frames.push(frame);
                                    self.current_base = this_slot;
                                    self.ip = func.chunk_index;
                                    continue;
                                }
                            }
                        }
                    }
                    
                    if let Some(instance) = receiver.as_struct() {
                        let instance = instance.lock();
                        let type_name = instance.type_name.clone();
                        drop(instance);
                        
                        if let Some(type_info) = self.chunk.get_type(&type_name).cloned() {
                            if let Some(&method_index) = type_info.methods.get(&method_name) {
                                let method_index = method_index as usize;
                                if let Some(func) = self.chunk.constants[method_index].as_function() {
                                    let func = func.clone();
                                    let this_slot = receiver_idx;
                                    
                                    if arg_count < func.required_params.saturating_sub(1) {
                                        let msg = format!(
                                            "Method '{}' expected at least {} arguments but got {}",
                                            method_name, func.required_params.saturating_sub(1), arg_count
                                        );
                                        return Err(self.runtime_error(&msg));
                                    }
                                    
                                    let missing = func.arity.saturating_sub(arg_count + 1);
                                    if missing > 0 && !func.defaults.is_empty() {
                                        let start = func.defaults.len().saturating_sub(missing);
                                        for i in 0..missing {
                                            if start + i < func.defaults.len() {
                                                self.push(func.defaults[start + i].clone());
                                            }
                                        }
                                    }
                                    
                                    if self.frames.len() >= MAX_FRAMES {
                                        return Err(self.runtime_error("Stack overflow"));
                                    }
                                    
                                    let frame = CallFrame {
                                        return_ip: self.ip as u32,
                                        base_slot: this_slot as u16,
                                        is_method_call: true,
                                    };
                                    self.frames.push(frame);
                                    self.current_base = this_slot;
                                    self.ip = func.chunk_index;
                                    continue;
                                }
                            }
                        }
                    }
                    
                    return Err(self.runtime_error(&format!(
                        "Cannot safely call method '{}' on {}",
                        method_name, receiver.type_name()
                    )));
                }
                
                OpCode::NonNullInvokeMethod => {
                    let method_name_index = self.read_u16() as usize;
                    let arg_count = self.read_byte() as usize;
                    
                    // 获取方法名
                    let method_name = if let Some(s) = self.chunk.constants[method_name_index].as_string() {
                        s.clone()
                    } else {
                        return Err(self.runtime_error("Invalid method name"));
                    };
                    
                    // 获取 receiver（在参数下方）
                    let receiver_idx = self.stack.len() - arg_count - 1;
                    let receiver = self.stack[receiver_idx].clone();
                    
                    // 如果 receiver 为 null，抛出异常
                    if receiver.is_null() {
                        return Err(self.runtime_error(&format!(
                            "Non-null assertion failed: cannot call method '{}' on null",
                            method_name
                        )));
                    }
                    
                    // 检查是否是标准库类实例
                    if let Some(class_instance) = receiver.as_class() {
                        let instance_guard = class_instance.lock();
                        let class_name = instance_guard.class_name.clone();
                        drop(instance_guard);
                        
                        let registry = get_stdlib_registry();
                        if let Some((_, _)) = registry.find_class_module(&class_name) {
                            // 是标准库类实例，从栈中获取参数
                            let args_start = receiver_idx + 1;
                            let args = self.stack[args_start..].to_vec();
                            // 弹出参数和receiver
                            self.stack.truncate(receiver_idx);
                            
                            // 调用标准库方法
                            match registry.call_class_method(&receiver, &method_name, &args) {
                                Ok(result) => {
                                    self.push(result);
                                    continue;
                                }
                                Err(e) => {
                                    return Err(self.runtime_error(&e));
                                }
                            }
                        }
                    }
                    
                    // 否则执行类/结构体方法调用
                    if let Some(instance) = receiver.as_class() {
                        let instance = instance.lock();
                        let class_name = instance.class_name.clone();
                        drop(instance);
                        
                        if let Some(type_info) = self.chunk.get_type(&class_name).cloned() {
                            if let Some(&method_index) = type_info.methods.get(&method_name) {
                                let method_index = method_index as usize;
                                if let Some(func) = self.chunk.constants[method_index].as_function() {
                                    let func = func.clone();
                                    let this_slot = receiver_idx;
                                    
                                    if arg_count < func.required_params.saturating_sub(1) {
                                        let msg = format!(
                                            "Method '{}' expected at least {} arguments but got {}",
                                            method_name, func.required_params.saturating_sub(1), arg_count
                                        );
                                        return Err(self.runtime_error(&msg));
                                    }
                                    
                                    let missing = func.arity.saturating_sub(arg_count + 1);
                                    if missing > 0 && !func.defaults.is_empty() {
                                        let start = func.defaults.len().saturating_sub(missing);
                                        for i in 0..missing {
                                            if start + i < func.defaults.len() {
                                                self.push(func.defaults[start + i].clone());
                                            }
                                        }
                                    }
                                    
                                    if self.frames.len() >= MAX_FRAMES {
                                        return Err(self.runtime_error("Stack overflow"));
                                    }
                                    
                                    let frame = CallFrame {
                                        return_ip: self.ip as u32,
                                        base_slot: this_slot as u16,
                                        is_method_call: true,
                                    };
                                    self.frames.push(frame);
                                    self.current_base = this_slot;
                                    self.ip = func.chunk_index;
                                    continue;
                                }
                            }
                        }
                    }
                    
                    if let Some(instance) = receiver.as_struct() {
                        let instance = instance.lock();
                        let type_name = instance.type_name.clone();
                        drop(instance);
                        
                        if let Some(type_info) = self.chunk.get_type(&type_name).cloned() {
                            if let Some(&method_index) = type_info.methods.get(&method_name) {
                                let method_index = method_index as usize;
                                if let Some(func) = self.chunk.constants[method_index].as_function() {
                                    let func = func.clone();
                                    let this_slot = receiver_idx;
                                    
                                    if arg_count < func.required_params.saturating_sub(1) {
                                        let msg = format!(
                                            "Method '{}' expected at least {} arguments but got {}",
                                            method_name, func.required_params.saturating_sub(1), arg_count
                                        );
                                        return Err(self.runtime_error(&msg));
                                    }
                                    
                                    let missing = func.arity.saturating_sub(arg_count + 1);
                                    if missing > 0 && !func.defaults.is_empty() {
                                        let start = func.defaults.len().saturating_sub(missing);
                                        for i in 0..missing {
                                            if start + i < func.defaults.len() {
                                                self.push(func.defaults[start + i].clone());
                                            }
                                        }
                                    }
                                    
                                    if self.frames.len() >= MAX_FRAMES {
                                        return Err(self.runtime_error("Stack overflow"));
                                    }
                                    
                                    let frame = CallFrame {
                                        return_ip: self.ip as u32,
                                        base_slot: this_slot as u16,
                                        is_method_call: true,
                                    };
                                    self.frames.push(frame);
                                    self.current_base = this_slot;
                                    self.ip = func.chunk_index;
                                    continue;
                                }
                            }
                        }
                    }
                    
                    return Err(self.runtime_error(&format!(
                        "Cannot call method '{}' on {}",
                        method_name, receiver.type_name()
                    )));
                }
                
                OpCode::InvokeMethod => {
                    let method_name_index = self.read_u16() as usize;
                    let arg_count = self.read_byte() as usize;
                    
                    // 获取方法名
                    let method_name = if let Some(s) = self.chunk.constants[method_name_index].as_string() {
                        s.clone()
                    } else {
                        return Err(self.runtime_error("Invalid method name"));
                    };
                    
                    // 获取 receiver（在参数下方）
                    let receiver_idx = self.stack.len() - arg_count - 1;
                    let receiver = self.stack[receiver_idx].clone();
                    
                    // 检查是否是标准库类实例
                    if let Some(class_instance) = receiver.as_class() {
                        let instance_guard = class_instance.lock();
                        let class_name = instance_guard.class_name.clone();
                        drop(instance_guard);
                        
                        let registry = get_stdlib_registry();
                        if let Some((_, _)) = registry.find_class_module(&class_name) {
                            // 是标准库类实例，从栈中获取参数
                            let args_start = receiver_idx + 1;
                            let args = self.stack[args_start..].to_vec();
                            // 弹出参数和receiver
                            self.stack.truncate(receiver_idx);
                            
                            // 调用标准库方法
                            match registry.call_class_method(&receiver, &method_name, &args) {
                                Ok(result) => {
                                    self.push(result);
                                    continue;
                                }
                                Err(e) => {
                                    return Err(self.runtime_error(&e));
                                }
                            }
                        }
                    }
                    
                    // 检查是否是类型引用（静态方法调用）
                    if let Some(class_name) = receiver.as_type_ref() {
                        let class_name = class_name.clone();
                        // 查找静态方法
                        let func_index = match self.chunk.get_static_method(&class_name, &method_name) {
                            Some(idx) => idx as usize,
                            None => return Err(self.runtime_error(&format!(
                                "Class '{}' has no static method '{}'",
                                class_name, method_name
                            ))),
                        };
                        
                        let func = if let Some(f) = self.chunk.constants[func_index].as_function() {
                            f.clone()
                        } else {
                            return Err(self.runtime_error("Static method is not a function"));
                        };
                        
                        // 移除类型引用，静态方法不需要 this
                        self.stack.remove(receiver_idx);
                        let base = self.stack.len() - arg_count;
                        
                        // 填充默认参数
                        let missing = func.arity.saturating_sub(arg_count);
                        if missing > 0 && !func.defaults.is_empty() {
                            let start = func.defaults.len().saturating_sub(missing);
                            for i in 0..missing {
                                if start + i < func.defaults.len() {
                                    self.push(func.defaults[start + i].clone());
                                }
                            }
                        }
                        
                        if self.frames.len() >= MAX_FRAMES {
                            return Err(self.runtime_error("Stack overflow"));
                        }
                        
                        let frame = CallFrame {
                            return_ip: self.ip as u32,
                            base_slot: base as u16,
                            is_method_call: false, // 静态方法没有 this，类似普通函数调用
                        };
                        self.frames.push(frame);
                        self.ip = func.chunk_index;
                        continue;
                    }
                    
                    // 检查是否是数组方法调用
                    if let Some(arr) = receiver.as_array() {
                        match method_name.as_str() {
                            "push" => {
                                // arr.push(value) - 向数组末尾添加元素
                                if arg_count != 1 {
                                    return Err(self.runtime_error("push() expects 1 argument"));
                                }
                                let value = self.stack[receiver_idx + 1].clone();
                                arr.lock().push(value);
                                // 移除参数和 receiver，返回 null
                                self.stack.truncate(receiver_idx);
                                self.push(Value::null());
                                continue;
                            }
                            "pop" => {
                                // arr.pop() - 移除并返回数组末尾元素
                                if arg_count != 0 {
                                    return Err(self.runtime_error("pop() expects 0 arguments"));
                                }
                                let popped = arr.lock().pop().unwrap_or(Value::null());
                                // 移除 receiver，返回弹出的值
                                self.stack.truncate(receiver_idx);
                                self.push(popped);
                                continue;
                            }
                            "len" => {
                                // arr.len() - 返回数组长度
                                if arg_count != 0 {
                                    return Err(self.runtime_error("len() expects 0 arguments"));
                                }
                                let len = arr.lock().len() as i64;
                                self.stack.truncate(receiver_idx);
                                self.push(Value::int(len));
                                continue;
                            }
                            "first" => {
                                // arr.first() - 返回第一个元素
                                if arg_count != 0 {
                                    return Err(self.runtime_error("first() expects 0 arguments"));
                                }
                                let first = arr.lock().first().cloned().unwrap_or(Value::null());
                                self.stack.truncate(receiver_idx);
                                self.push(first);
                                continue;
                            }
                            "last" => {
                                // arr.last() - 返回最后一个元素
                                if arg_count != 0 {
                                    return Err(self.runtime_error("last() expects 0 arguments"));
                                }
                                let last = arr.lock().last().cloned().unwrap_or(Value::null());
                                self.stack.truncate(receiver_idx);
                                self.push(last);
                                continue;
                            }
                            "contains" => {
                                // arr.contains(value) - 检查数组是否包含某值
                                if arg_count != 1 {
                                    return Err(self.runtime_error("contains() expects 1 argument"));
                                }
                                let value = self.stack[receiver_idx + 1].clone();
                                let contains = arr.lock().contains(&value);
                                self.stack.truncate(receiver_idx);
                                self.push(Value::bool(contains));
                                continue;
                            }
                            "reverse" => {
                                // arr.reverse() - 反转数组
                                if arg_count != 0 {
                                    return Err(self.runtime_error("reverse() expects 0 arguments"));
                                }
                                arr.lock().reverse();
                                self.stack.truncate(receiver_idx);
                                self.push(Value::null());
                                continue;
                            }
                            "clear" => {
                                // arr.clear() - 清空数组
                                if arg_count != 0 {
                                    return Err(self.runtime_error("clear() expects 0 arguments"));
                                }
                                arr.lock().clear();
                                self.stack.truncate(receiver_idx);
                                self.push(Value::null());
                                continue;
                            }
                            "indexOf" => {
                                // arr.indexOf(value) - 查找元素索引
                                if arg_count != 1 {
                                    return Err(self.runtime_error("indexOf() expects 1 argument"));
                                }
                                let value = self.stack[receiver_idx + 1].clone();
                                let idx = arr.lock().iter().position(|x| x == &value)
                                    .map(|i| i as i64)
                                    .unwrap_or(-1);
                                self.stack.truncate(receiver_idx);
                                self.push(Value::int(idx));
                                continue;
                            }
                            "lastIndexOf" => {
                                // arr.lastIndexOf(value) - 查找最后一个元素索引
                                if arg_count != 1 {
                                    return Err(self.runtime_error("lastIndexOf() expects 1 argument"));
                                }
                                let value = self.stack[receiver_idx + 1].clone();
                                let idx = arr.lock().iter().rposition(|x| x == &value)
                                    .map(|i| i as i64)
                                    .unwrap_or(-1);
                                self.stack.truncate(receiver_idx);
                                self.push(Value::int(idx));
                                continue;
                            }
                            "join" => {
                                // arr.join(separator) - 连接为字符串
                                if arg_count != 1 {
                                    return Err(self.runtime_error("join() expects 1 argument"));
                                }
                                let sep = if let Some(s) = self.stack[receiver_idx + 1].as_string() {
                                    s.clone()
                                } else {
                                    return Err(self.runtime_error("join() expects a string argument"));
                                };
                                let result: String = arr.lock().iter()
                                    .map(|v| format!("{}", v))
                                    .collect::<Vec<_>>()
                                    .join(&sep);
                                self.stack.truncate(receiver_idx);
                                self.push(Value::string(result));
                                continue;
                            }
                            "slice" => {
                                // arr.slice(start, end?) - 截取数组
                                if arg_count < 1 || arg_count > 2 {
                                    return Err(self.runtime_error("slice() expects 1 or 2 arguments"));
                                }
                                let start = if let Some(i) = self.stack[receiver_idx + 1].as_int() {
                                    i as usize
                                } else {
                                    return Err(self.runtime_error("slice() first argument must be integer"));
                                };
                                let arr_len = arr.lock().len();
                                let end = if arg_count == 2 {
                                    if let Some(i) = self.stack[receiver_idx + 2].as_int() {
                                        (i as usize).min(arr_len)
                                    } else {
                                        return Err(self.runtime_error("slice() second argument must be integer"));
                                    }
                                } else {
                                    arr_len
                                };
                                let result: Vec<Value> = arr.lock()[start.min(arr_len)..end].to_vec();
                                self.stack.truncate(receiver_idx);
                                self.push(Value::array(Arc::new(Mutex::new(result))));
                                continue;
                            }
                            "concat" => {
                                // arr.concat(other) - 连接两个数组
                                if arg_count != 1 {
                                    return Err(self.runtime_error("concat() expects 1 argument"));
                                }
                                let other = if let Some(a) = self.stack[receiver_idx + 1].as_array() {
                                    a.lock().clone()
                                } else {
                                    return Err(self.runtime_error("concat() expects an array argument"));
                                };
                                let mut result = arr.lock().clone();
                                result.extend(other);
                                self.stack.truncate(receiver_idx);
                                self.push(Value::array(Arc::new(Mutex::new(result))));
                                continue;
                            }
                            "copy" => {
                                // arr.copy() - 复制数组
                                if arg_count != 0 {
                                    return Err(self.runtime_error("copy() expects 0 arguments"));
                                }
                                let result = arr.lock().clone();
                                self.stack.truncate(receiver_idx);
                                self.push(Value::array(Arc::new(Mutex::new(result))));
                                continue;
                            }
                            "isEmpty" => {
                                // arr.isEmpty() - 检查是否为空
                                if arg_count != 0 {
                                    return Err(self.runtime_error("isEmpty() expects 0 arguments"));
                                }
                                let result = arr.lock().is_empty();
                                self.stack.truncate(receiver_idx);
                                self.push(Value::bool(result));
                                continue;
                            }
                            "collect" => {
                                // arr.collect(fn) - 映射转换 (注: map 是关键字，用 collect 代替)
                                if arg_count != 1 {
                                    return Err(self.runtime_error("map() expects 1 argument"));
                                }
                                let callback = self.stack[receiver_idx + 1].clone();
                                let func = callback.as_function().ok_or_else(|| {
                                    self.runtime_error("map() expects a function argument")
                                })?.clone();
                                let elements = arr.lock().clone();
                                self.stack.truncate(receiver_idx);
                                let mut results = Vec::with_capacity(elements.len());
                                for (i, elem) in elements.into_iter().enumerate() {
                                    let result = self.call_closure(&func, &[elem, Value::int(i as i64)])?;
                                    results.push(result);
                                }
                                self.push(Value::array(Arc::new(Mutex::new(results))));
                                continue;
                            }
                            "filter" | "where" => {
                                // arr.filter(fn) / arr.where(fn) - 过滤筛选
                                if arg_count != 1 {
                                    return Err(self.runtime_error("filter() expects 1 argument"));
                                }
                                let callback = self.stack[receiver_idx + 1].clone();
                                let func = callback.as_function().ok_or_else(|| {
                                    self.runtime_error("filter() expects a function argument")
                                })?.clone();
                                let elements = arr.lock().clone();
                                self.stack.truncate(receiver_idx);
                                let mut results = Vec::new();
                                for (i, elem) in elements.into_iter().enumerate() {
                                    let keep = self.call_closure(&func, &[elem.clone(), Value::int(i as i64)])?;
                                    if keep.is_truthy() {
                                        results.push(elem);
                                    }
                                }
                                self.push(Value::array(Arc::new(Mutex::new(results))));
                                continue;
                            }
                            "reduce" | "fold" => {
                                // arr.reduce(fn, init) / arr.fold(fn, init) - 归约聚合
                                if arg_count != 2 {
                                    return Err(self.runtime_error("reduce() expects 2 arguments"));
                                }
                                let callback = self.stack[receiver_idx + 1].clone();
                                let func = callback.as_function().ok_or_else(|| {
                                    self.runtime_error("reduce() first argument must be a function")
                                })?.clone();
                                let mut acc = self.stack[receiver_idx + 2].clone();
                                let elements = arr.lock().clone();
                                self.stack.truncate(receiver_idx);
                                for (i, elem) in elements.into_iter().enumerate() {
                                    acc = self.call_closure(&func, &[acc, elem, Value::int(i as i64)])?;
                                }
                                self.push(acc);
                                continue;
                            }
                            "forEach" | "each" => {
                                // arr.forEach(fn) / arr.each(fn) - 遍历执行
                                if arg_count != 1 {
                                    return Err(self.runtime_error("forEach() expects 1 argument"));
                                }
                                let callback = self.stack[receiver_idx + 1].clone();
                                let func = callback.as_function().ok_or_else(|| {
                                    self.runtime_error("forEach() expects a function argument")
                                })?.clone();
                                let elements = arr.lock().clone();
                                self.stack.truncate(receiver_idx);
                                for (i, elem) in elements.into_iter().enumerate() {
                                    self.call_closure(&func, &[elem, Value::int(i as i64)])?;
                                }
                                self.push(Value::null());
                                continue;
                            }
                            "find" => {
                                // arr.find(fn) - 查找第一个满足条件的元素
                                if arg_count != 1 {
                                    return Err(self.runtime_error("find() expects 1 argument"));
                                }
                                let callback = self.stack[receiver_idx + 1].clone();
                                let func = callback.as_function().ok_or_else(|| {
                                    self.runtime_error("find() expects a function argument")
                                })?.clone();
                                let elements = arr.lock().clone();
                                self.stack.truncate(receiver_idx);
                                let mut found = Value::null();
                                for (i, elem) in elements.into_iter().enumerate() {
                                    let matches = self.call_closure(&func, &[elem.clone(), Value::int(i as i64)])?;
                                    if matches.is_truthy() {
                                        found = elem;
                                        break;
                                    }
                                }
                                self.push(found);
                                continue;
                            }
                            "findIndex" => {
                                // arr.findIndex(fn) - 查找第一个满足条件的元素索引
                                if arg_count != 1 {
                                    return Err(self.runtime_error("findIndex() expects 1 argument"));
                                }
                                let callback = self.stack[receiver_idx + 1].clone();
                                let func = callback.as_function().ok_or_else(|| {
                                    self.runtime_error("findIndex() expects a function argument")
                                })?.clone();
                                let elements = arr.lock().clone();
                                self.stack.truncate(receiver_idx);
                                let mut found_idx: i64 = -1;
                                for (i, elem) in elements.into_iter().enumerate() {
                                    let matches = self.call_closure(&func, &[elem, Value::int(i as i64)])?;
                                    if matches.is_truthy() {
                                        found_idx = i as i64;
                                        break;
                                    }
                                }
                                self.push(Value::int(found_idx));
                                continue;
                            }
                            "every" | "all" => {
                                // arr.every(fn) / arr.all(fn) - 检查所有元素是否都满足条件
                                if arg_count != 1 {
                                    return Err(self.runtime_error("every() expects 1 argument"));
                                }
                                let callback = self.stack[receiver_idx + 1].clone();
                                let func = callback.as_function().ok_or_else(|| {
                                    self.runtime_error("every() expects a function argument")
                                })?.clone();
                                let elements = arr.lock().clone();
                                self.stack.truncate(receiver_idx);
                                let mut all_pass = true;
                                for (i, elem) in elements.into_iter().enumerate() {
                                    let passes = self.call_closure(&func, &[elem, Value::int(i as i64)])?;
                                    if !passes.is_truthy() {
                                        all_pass = false;
                                        break;
                                    }
                                }
                                self.push(Value::bool(all_pass));
                                continue;
                            }
                            "some" | "any" => {
                                // arr.some(fn) / arr.any(fn) - 检查是否有任一元素满足条件
                                if arg_count != 1 {
                                    return Err(self.runtime_error("some() expects 1 argument"));
                                }
                                let callback = self.stack[receiver_idx + 1].clone();
                                let func = callback.as_function().ok_or_else(|| {
                                    self.runtime_error("some() expects a function argument")
                                })?.clone();
                                let elements = arr.lock().clone();
                                self.stack.truncate(receiver_idx);
                                let mut any_pass = false;
                                for (i, elem) in elements.into_iter().enumerate() {
                                    let passes = self.call_closure(&func, &[elem, Value::int(i as i64)])?;
                                    if passes.is_truthy() {
                                        any_pass = true;
                                        break;
                                    }
                                }
                                self.push(Value::bool(any_pass));
                                continue;
                            }
                            "sort" => {
                                // arr.sort(fn?) - 排序（可选比较函数）
                                if arg_count > 1 {
                                    return Err(self.runtime_error("sort() expects 0 or 1 argument"));
                                }
                                let mut elements = arr.lock().clone();
                                if arg_count == 1 {
                                    let callback = self.stack[receiver_idx + 1].clone();
                                    let func = callback.as_function().ok_or_else(|| {
                                        self.runtime_error("sort() argument must be a function")
                                    })?.clone();
                                    self.stack.truncate(receiver_idx);
                                    // 使用冒泡排序来支持自定义比较函数
                                    let len = elements.len();
                                    for i in 0..len {
                                        for j in 0..len - 1 - i {
                                            let cmp = self.call_closure(&func, &[elements[j].clone(), elements[j + 1].clone()])?;
                                            if let Some(n) = cmp.as_int() {
                                                if n > 0 {
                                                    elements.swap(j, j + 1);
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    self.stack.truncate(receiver_idx);
                                    // 默认排序：数字按大小，字符串按字典序
                                    elements.sort_by(|a, b| {
                                        if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                                            x.cmp(&y)
                                        } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                                            x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
                                        } else if let (Some(x), Some(y)) = (a.as_string(), b.as_string()) {
                                            x.cmp(y)
                                        } else {
                                            std::cmp::Ordering::Equal
                                        }
                                    });
                                }
                                *arr.lock() = elements;
                                self.push(Value::null());
                                continue;
                            }
                            _ => {
                                return Err(self.runtime_error(&format!(
                                    "Array has no method '{}'",
                                    method_name
                                )));
                            }
                        }
                    }
                    
                    // 检查是否是字符串方法调用
                    if let Some(s) = receiver.as_string() {
                        let s = s.clone();
                        match method_name.as_str() {
                            "len" => {
                                // str.len() - 返回字符串长度
                                if arg_count != 0 {
                                    return Err(self.runtime_error("len() expects 0 arguments"));
                                }
                                let len = s.chars().count() as i64;
                                self.stack.truncate(receiver_idx);
                                self.push(Value::int(len));
                                continue;
                            }
                            "split" => {
                                // str.split(delimiter) - 按分隔符分割字符串
                                if arg_count != 1 {
                                    return Err(self.runtime_error("split() expects 1 argument"));
                                }
                                let delimiter = if let Some(d) = self.stack[receiver_idx + 1].as_string() {
                                    d.clone()
                                } else {
                                    return Err(self.runtime_error("split() expects a string argument"));
                                };
                                let parts: Vec<Value> = s.split(&delimiter)
                                    .map(|part| Value::string(part.to_string()))
                                    .collect();
                                self.stack.truncate(receiver_idx);
                                self.push(Value::array(Arc::new(Mutex::new(parts))));
                                continue;
                            }
                            "trim" => {
                                // str.trim() - 去除首尾空白
                                if arg_count != 0 {
                                    return Err(self.runtime_error("trim() expects 0 arguments"));
                                }
                                let trimmed = s.trim().to_string();
                                self.stack.truncate(receiver_idx);
                                self.push(Value::string(trimmed));
                                continue;
                            }
                            "trimStart" => {
                                // str.trimStart() - 去除开头空白
                                if arg_count != 0 {
                                    return Err(self.runtime_error("trimStart() expects 0 arguments"));
                                }
                                let trimmed = s.trim_start().to_string();
                                self.stack.truncate(receiver_idx);
                                self.push(Value::string(trimmed));
                                continue;
                            }
                            "trimEnd" => {
                                // str.trimEnd() - 去除结尾空白
                                if arg_count != 0 {
                                    return Err(self.runtime_error("trimEnd() expects 0 arguments"));
                                }
                                let trimmed = s.trim_end().to_string();
                                self.stack.truncate(receiver_idx);
                                self.push(Value::string(trimmed));
                                continue;
                            }
                            "replace" => {
                                // str.replace(old, new) - 替换所有匹配项
                                if arg_count != 2 {
                                    return Err(self.runtime_error("replace() expects 2 arguments"));
                                }
                                let old_str = if let Some(o) = self.stack[receiver_idx + 1].as_string() {
                                    o.clone()
                                } else {
                                    return Err(self.runtime_error("replace() first argument must be string"));
                                };
                                let new_str = if let Some(n) = self.stack[receiver_idx + 2].as_string() {
                                    n.clone()
                                } else {
                                    return Err(self.runtime_error("replace() second argument must be string"));
                                };
                                let result = s.replace(&old_str, &new_str);
                                self.stack.truncate(receiver_idx);
                                self.push(Value::string(result));
                                continue;
                            }
                            "replaceFirst" => {
                                // str.replaceFirst(old, new) - 替换第一个匹配项
                                if arg_count != 2 {
                                    return Err(self.runtime_error("replaceFirst() expects 2 arguments"));
                                }
                                let old_str = if let Some(o) = self.stack[receiver_idx + 1].as_string() {
                                    o.clone()
                                } else {
                                    return Err(self.runtime_error("replaceFirst() first argument must be string"));
                                };
                                let new_str = if let Some(n) = self.stack[receiver_idx + 2].as_string() {
                                    n.clone()
                                } else {
                                    return Err(self.runtime_error("replaceFirst() second argument must be string"));
                                };
                                let result = s.replacen(&old_str, &new_str, 1);
                                self.stack.truncate(receiver_idx);
                                self.push(Value::string(result));
                                continue;
                            }
                            "contains" => {
                                // str.contains(substr) - 检查是否包含子串
                                if arg_count != 1 {
                                    return Err(self.runtime_error("contains() expects 1 argument"));
                                }
                                let substr = if let Some(sub) = self.stack[receiver_idx + 1].as_string() {
                                    sub.clone()
                                } else {
                                    return Err(self.runtime_error("contains() expects a string argument"));
                                };
                                let result = s.contains(&substr);
                                self.stack.truncate(receiver_idx);
                                self.push(Value::bool(result));
                                continue;
                            }
                            "startsWith" => {
                                // str.startsWith(prefix) - 检查是否以前缀开头
                                if arg_count != 1 {
                                    return Err(self.runtime_error("startsWith() expects 1 argument"));
                                }
                                let prefix = if let Some(p) = self.stack[receiver_idx + 1].as_string() {
                                    p.clone()
                                } else {
                                    return Err(self.runtime_error("startsWith() expects a string argument"));
                                };
                                let result = s.starts_with(&prefix);
                                self.stack.truncate(receiver_idx);
                                self.push(Value::bool(result));
                                continue;
                            }
                            "endsWith" => {
                                // str.endsWith(suffix) - 检查是否以后缀结尾
                                if arg_count != 1 {
                                    return Err(self.runtime_error("endsWith() expects 1 argument"));
                                }
                                let suffix = if let Some(sf) = self.stack[receiver_idx + 1].as_string() {
                                    sf.clone()
                                } else {
                                    return Err(self.runtime_error("endsWith() expects a string argument"));
                                };
                                let result = s.ends_with(&suffix);
                                self.stack.truncate(receiver_idx);
                                self.push(Value::bool(result));
                                continue;
                            }
                            "toUpper" => {
                                // str.toUpper() - 转大写
                                if arg_count != 0 {
                                    return Err(self.runtime_error("toUpper() expects 0 arguments"));
                                }
                                let result = s.to_uppercase();
                                self.stack.truncate(receiver_idx);
                                self.push(Value::string(result));
                                continue;
                            }
                            "toLower" => {
                                // str.toLower() - 转小写
                                if arg_count != 0 {
                                    return Err(self.runtime_error("toLower() expects 0 arguments"));
                                }
                                let result = s.to_lowercase();
                                self.stack.truncate(receiver_idx);
                                self.push(Value::string(result));
                                continue;
                            }
                            "charAt" => {
                                // str.charAt(index) - 获取指定位置字符
                                if arg_count != 1 {
                                    return Err(self.runtime_error("charAt() expects 1 argument"));
                                }
                                let index = if let Some(i) = self.stack[receiver_idx + 1].as_int() {
                                    i as usize
                                } else {
                                    return Err(self.runtime_error("charAt() expects an integer argument"));
                                };
                                let result = s.chars().nth(index)
                                    .map(|c| Value::string(c.to_string()))
                                    .unwrap_or(Value::null());
                                self.stack.truncate(receiver_idx);
                                self.push(result);
                                continue;
                            }
                            "indexOf" => {
                                // str.indexOf(substr) - 查找子串位置
                                if arg_count != 1 {
                                    return Err(self.runtime_error("indexOf() expects 1 argument"));
                                }
                                let substr = if let Some(sub) = self.stack[receiver_idx + 1].as_string() {
                                    sub.clone()
                                } else {
                                    return Err(self.runtime_error("indexOf() expects a string argument"));
                                };
                                let result = s.find(&substr)
                                    .map(|i| Value::int(i as i64))
                                    .unwrap_or(Value::int(-1));
                                self.stack.truncate(receiver_idx);
                                self.push(result);
                                continue;
                            }
                            "lastIndexOf" => {
                                // str.lastIndexOf(substr) - 查找最后一个子串位置
                                if arg_count != 1 {
                                    return Err(self.runtime_error("lastIndexOf() expects 1 argument"));
                                }
                                let substr = if let Some(sub) = self.stack[receiver_idx + 1].as_string() {
                                    sub.clone()
                                } else {
                                    return Err(self.runtime_error("lastIndexOf() expects a string argument"));
                                };
                                let result = s.rfind(&substr)
                                    .map(|i| Value::int(i as i64))
                                    .unwrap_or(Value::int(-1));
                                self.stack.truncate(receiver_idx);
                                self.push(result);
                                continue;
                            }
                            "substring" => {
                                // str.substring(start, end?) - 截取子串
                                if arg_count < 1 || arg_count > 2 {
                                    return Err(self.runtime_error("substring() expects 1 or 2 arguments"));
                                }
                                let start = if let Some(i) = self.stack[receiver_idx + 1].as_int() {
                                    i as usize
                                } else {
                                    return Err(self.runtime_error("substring() first argument must be integer"));
                                };
                                let end = if arg_count == 2 {
                                    if let Some(i) = self.stack[receiver_idx + 2].as_int() {
                                        i as usize
                                    } else {
                                        return Err(self.runtime_error("substring() second argument must be integer"));
                                    }
                                } else {
                                    s.chars().count()
                                };
                                let result: String = s.chars().skip(start).take(end - start).collect();
                                self.stack.truncate(receiver_idx);
                                self.push(Value::string(result));
                                continue;
                            }
                            "repeat" => {
                                // str.repeat(n) - 重复字符串
                                if arg_count != 1 {
                                    return Err(self.runtime_error("repeat() expects 1 argument"));
                                }
                                let n = if let Some(i) = self.stack[receiver_idx + 1].as_int() {
                                    i as usize
                                } else {
                                    return Err(self.runtime_error("repeat() expects an integer argument"));
                                };
                                let result = s.repeat(n);
                                self.stack.truncate(receiver_idx);
                                self.push(Value::string(result));
                                continue;
                            }
                            "isEmpty" => {
                                // str.isEmpty() - 检查是否为空
                                if arg_count != 0 {
                                    return Err(self.runtime_error("isEmpty() expects 0 arguments"));
                                }
                                let result = s.is_empty();
                                self.stack.truncate(receiver_idx);
                                self.push(Value::bool(result));
                                continue;
                            }
                            "reverse" => {
                                // str.reverse() - 反转字符串
                                if arg_count != 0 {
                                    return Err(self.runtime_error("reverse() expects 0 arguments"));
                                }
                                let result: String = s.chars().rev().collect();
                                self.stack.truncate(receiver_idx);
                                self.push(Value::string(result));
                                continue;
                            }
                            _ => {
                                return Err(self.runtime_error(&format!(
                                    "String has no method '{}'",
                                    method_name
                                )));
                            }
                        }
                    }
                    
                    // 检查是否是 Map 方法调用
                    if let Some(map) = receiver.as_map() {
                        match method_name.as_str() {
                            "len" => {
                                // map.len() - 返回键值对数量
                                if arg_count != 0 {
                                    return Err(self.runtime_error("len() expects 0 arguments"));
                                }
                                let len = map.lock().len() as i64;
                                self.stack.truncate(receiver_idx);
                                self.push(Value::int(len));
                                continue;
                            }
                            "keys" => {
                                // map.keys() - 返回所有键的数组
                                if arg_count != 0 {
                                    return Err(self.runtime_error("keys() expects 0 arguments"));
                                }
                                let keys: Vec<Value> = map.lock().keys()
                                    .map(|k| Value::string(k.clone()))
                                    .collect();
                                self.stack.truncate(receiver_idx);
                                self.push(Value::array(Arc::new(Mutex::new(keys))));
                                continue;
                            }
                            "values" => {
                                // map.values() - 返回所有值的数组
                                if arg_count != 0 {
                                    return Err(self.runtime_error("values() expects 0 arguments"));
                                }
                                let values: Vec<Value> = map.lock().values().cloned().collect();
                                self.stack.truncate(receiver_idx);
                                self.push(Value::array(Arc::new(Mutex::new(values))));
                                continue;
                            }
                            "has" => {
                                // map.has(key) - 检查键是否存在
                                if arg_count != 1 {
                                    return Err(self.runtime_error("has() expects 1 argument"));
                                }
                                let key = if let Some(s) = self.stack[receiver_idx + 1].as_string() {
                                    s.clone()
                                } else {
                                    return Err(self.runtime_error("Map key must be a string"));
                                };
                                let result = map.lock().contains_key(&key);
                                self.stack.truncate(receiver_idx);
                                self.push(Value::bool(result));
                                continue;
                            }
                            "get" => {
                                // map.get(key, default?) - 获取值，可选默认值
                                if arg_count < 1 || arg_count > 2 {
                                    return Err(self.runtime_error("get() expects 1 or 2 arguments"));
                                }
                                let key = if let Some(s) = self.stack[receiver_idx + 1].as_string() {
                                    s.clone()
                                } else {
                                    return Err(self.runtime_error("Map key must be a string"));
                                };
                                let default = if arg_count == 2 {
                                    self.stack[receiver_idx + 2]
                                } else {
                                    Value::null()
                                };
                                let result = map.lock().get(&key).cloned().unwrap_or(default);
                                self.stack.truncate(receiver_idx);
                                self.push(result);
                                continue;
                            }
                            "set" => {
                                // map.set(key, value) - 设置键值对
                                if arg_count != 2 {
                                    return Err(self.runtime_error("set() expects 2 arguments"));
                                }
                                let key = if let Some(s) = self.stack[receiver_idx + 1].as_string() {
                                    s.clone()
                                } else {
                                    return Err(self.runtime_error("Map key must be a string"));
                                };
                                let value = self.stack[receiver_idx + 2];
                                map.lock().insert(key, value);
                                self.stack.truncate(receiver_idx);
                                self.push(Value::null());
                                continue;
                            }
                            "remove" => {
                                // map.remove(key) - 删除键值对，返回被删除的值
                                if arg_count != 1 {
                                    return Err(self.runtime_error("remove() expects 1 argument"));
                                }
                                let key = if let Some(s) = self.stack[receiver_idx + 1].as_string() {
                                    s.clone()
                                } else {
                                    return Err(self.runtime_error("Map key must be a string"));
                                };
                                let removed = map.lock().remove(&key).unwrap_or(Value::null());
                                self.stack.truncate(receiver_idx);
                                self.push(removed);
                                continue;
                            }
                            "clear" => {
                                // map.clear() - 清空所有键值对
                                if arg_count != 0 {
                                    return Err(self.runtime_error("clear() expects 0 arguments"));
                                }
                                map.lock().clear();
                                self.stack.truncate(receiver_idx);
                                self.push(Value::null());
                                continue;
                            }
                            "isEmpty" => {
                                // map.isEmpty() - 检查是否为空
                                if arg_count != 0 {
                                    return Err(self.runtime_error("isEmpty() expects 0 arguments"));
                                }
                                let result = map.lock().is_empty();
                                self.stack.truncate(receiver_idx);
                                self.push(Value::bool(result));
                                continue;
                            }
                            _ => {
                                return Err(self.runtime_error(&format!(
                                    "Map has no method '{}'",
                                    method_name
                                )));
                            }
                        }
                    }
                    
                    // 检查是否是 Range 方法调用
                    if let Some((start, end, inclusive)) = receiver.as_range() {
                        
                        match method_name.as_str() {
                            "start" => {
                                // range.start() - 返回起始值
                                if arg_count != 0 {
                                    return Err(self.runtime_error("start() expects 0 arguments"));
                                }
                                self.stack.truncate(receiver_idx);
                                self.push(Value::int(start));
                                continue;
                            }
                            "end" => {
                                // range.end() - 返回结束值
                                if arg_count != 0 {
                                    return Err(self.runtime_error("end() expects 0 arguments"));
                                }
                                self.stack.truncate(receiver_idx);
                                self.push(Value::int(end));
                                continue;
                            }
                            "len" => {
                                // range.len() - 返回范围长度
                                if arg_count != 0 {
                                    return Err(self.runtime_error("len() expects 0 arguments"));
                                }
                                let len = if inclusive {
                                    (end - start + 1).max(0)
                                } else {
                                    (end - start).max(0)
                                };
                                self.stack.truncate(receiver_idx);
                                self.push(Value::int(len));
                                continue;
                            }
                            "contains" => {
                                // range.contains(value) - 检查是否包含值
                                if arg_count != 1 {
                                    return Err(self.runtime_error("contains() expects 1 argument"));
                                }
                                let value = if let Some(v) = self.stack[receiver_idx + 1].as_int() {
                                    v
                                } else {
                                    return Err(self.runtime_error("contains() expects an integer argument"));
                                };
                                let result = if inclusive {
                                    value >= start && value <= end
                                } else {
                                    value >= start && value < end
                                };
                                self.stack.truncate(receiver_idx);
                                self.push(Value::bool(result));
                                continue;
                            }
                            "toArray" => {
                                // range.toArray() - 转换为数组
                                if arg_count != 0 {
                                    return Err(self.runtime_error("toArray() expects 0 arguments"));
                                }
                                let arr: Vec<Value> = if inclusive {
                                    (start..=end).map(Value::int).collect()
                                } else {
                                    (start..end).map(Value::int).collect()
                                };
                                self.stack.truncate(receiver_idx);
                                self.push(Value::array(Arc::new(Mutex::new(arr))));
                                continue;
                            }
                            "isInclusive" => {
                                // range.isInclusive() - 是否是包含范围
                                if arg_count != 0 {
                                    return Err(self.runtime_error("isInclusive() expects 0 arguments"));
                                }
                                self.stack.truncate(receiver_idx);
                                self.push(Value::bool(inclusive));
                                continue;
                            }
                            "isEmpty" => {
                                // range.isEmpty() - 是否为空范围
                                if arg_count != 0 {
                                    return Err(self.runtime_error("isEmpty() expects 0 arguments"));
                                }
                                let empty = if inclusive {
                                    start > end
                                } else {
                                    start >= end
                                };
                                self.stack.truncate(receiver_idx);
                                self.push(Value::bool(empty));
                                continue;
                            }
                            "step" => {
                                // range.step(n) - 返回带步长的迭代数组
                                if arg_count != 1 {
                                    return Err(self.runtime_error("step() expects 1 argument"));
                                }
                                let step_val = if let Some(s) = self.stack[receiver_idx + 1].as_int() {
                                    s
                                } else {
                                    return Err(self.runtime_error("step() expects an integer argument"));
                                };
                                if step_val <= 0 {
                                    return Err(self.runtime_error("step() argument must be positive"));
                                }
                                let mut arr = Vec::new();
                                let mut i = start;
                                let limit = if inclusive { end + 1 } else { end };
                                while i < limit {
                                    arr.push(Value::int(i));
                                    i += step_val;
                                }
                                self.stack.truncate(receiver_idx);
                                self.push(Value::array(Arc::new(Mutex::new(arr))));
                                continue;
                            }
                            _ => {
                                return Err(self.runtime_error(&format!(
                                    "Range has no method '{}'",
                                    method_name
                                )));
                            }
                        }
                    }
                    
                    // 获取类型名和方法
                    let type_name = if let Some(s) = receiver.as_struct() {
                        s.lock().type_name.clone()
                    } else if let Some(c) = receiver.as_class() {
                        c.lock().class_name.clone()
                    } else {
                        return Err(self.runtime_error(&format!(
                            "Cannot call method '{}' on {}",
                            method_name,
                            receiver.type_name()
                        )));
                    };
                    
                    // 查找方法
                    let func_index = match self.chunk.get_method(&type_name, &method_name) {
                        Some(idx) => idx as usize,
                        None => return Err(self.runtime_error(&format!(
                            "Type '{}' has no method '{}'",
                            type_name, method_name
                        ))),
                    };
                    
                    // 获取函数对象
                    let func = if let Some(f) = self.chunk.constants[func_index].as_function() {
                        f.clone()
                    } else {
                        return Err(self.runtime_error("Method is not a function"));
                    };
                    
                    // 检查参数数量（+1 是因为 this 作为第一个参数）
                    let _expected_args = func.arity;
                    let actual_args = arg_count + 1; // receiver + args
                    
                    if actual_args < func.required_params {
                        let msg = format!(
                            "Method '{}' expected at least {} arguments but got {}",
                            method_name, func.required_params - 1, arg_count
                        );
                        return Err(self.runtime_error(&msg));
                    }
                    
                    if !func.has_variadic && actual_args > func.arity {
                        let msg = format!(
                            "Method '{}' expected at most {} arguments but got {}",
                            method_name, func.arity - 1, arg_count
                        );
                        return Err(self.runtime_error(&msg));
                    }
                    
                    // 处理默认参数
                    let fixed_params = if func.has_variadic { func.arity - 1 } else { func.arity };
                    let missing_count = fixed_params.saturating_sub(actual_args);
                    if missing_count > 0 && !func.defaults.is_empty() {
                        let defaults_start = func.defaults.len().saturating_sub(missing_count);
                        for i in 0..missing_count {
                            if defaults_start + i < func.defaults.len() {
                                self.push(func.defaults[defaults_start + i].clone());
                            }
                        }
                    }
                    
                    // 检查调用深度
                    if self.frames.len() >= MAX_FRAMES {
                        let msg = "Stack overflow: too many nested function calls";
                        return Err(self.runtime_error(msg));
                    }
                    
                    // receiver 已经在栈上正确位置（在参数下方）
                    // 创建调用帧：base_slot 指向 receiver 位置
                    let frame = CallFrame {
                        return_ip: self.ip as u32,
                        base_slot: receiver_idx as u16, // receiver 作为第一个局部变量 (this)
                        is_method_call: true, // 实例方法调用
                    };
                    self.frames.push(frame);
                    self.current_base = receiver_idx; // 设置当前栈基址
                    
                    // 跳转到方法体
                    self.ip = func.chunk_index;
                }
                
                OpCode::NewClass => {
                    let class_name_index = self.read_u16() as usize;
                    let arg_count = self.read_byte() as usize;
                    
                    // 获取类名
                    let class_name = if let Some(s) = self.chunk.constants[class_name_index].as_string() {
                        s.clone()
                    } else {
                        return Err(self.runtime_error("Invalid class name"));
                    };
                    
                    // 检查是否是标准库类
                    let registry = get_stdlib_registry();
                    if let Some((_, _)) = registry.find_class_module(&class_name) {
                        // 是标准库类，从栈中获取参数
                        let args_start = self.stack.len() - arg_count;
                        let args = self.stack[args_start..].to_vec();
                        // 弹出参数
                        self.stack.truncate(args_start);
                        
                        // 调用标准库构造函数
                        match registry.create_class_instance(&class_name, &args) {
                            Ok(instance) => {
                                self.push(instance);
                                continue;
                            }
                            Err(e) => {
                                return Err(self.runtime_error(&e));
                            }
                        }
                    }
                    
                    // 不是标准库类，按普通类处理
                    // 获取类型信息
                    let type_info = match self.chunk.get_type(&class_name) {
                        Some(t) => t.clone(),
                        None => return Err(self.runtime_error(&format!(
                            "Undefined class: {}", class_name
                        ))),
                    };
                    
                    // 检查是否是抽象类
                    if type_info.is_abstract {
                        return Err(self.runtime_error(&format!(
                            "Cannot instantiate abstract class '{}'", class_name
                        )));
                    }
                    
                    // 创建实例，初始化字段为 null
                    let mut fields = std::collections::HashMap::new();
                    for field_name in &type_info.fields {
                        fields.insert(field_name.clone(), Value::null());
                    }
                    
                    let instance = super::value::ClassInstance {
                        class_name: class_name.clone(),
                        parent_class: type_info.parent.clone(),
                        fields,
                    };
                    let instance_value = Value::class(Arc::new(Mutex::new(instance)));
                    
                    // 查找 init 构造函数
                    if let Some(init_index) = type_info.methods.get("init") {
                        let init_func = if let Some(f) = self.chunk.constants[*init_index as usize].as_function() {
                            f.clone()
                        } else {
                            return Err(self.runtime_error("init is not a function"));
                        };
                        
                        // 检查参数数量
                        let expected_args = init_func.arity - 1; // -1 因为 this 是隐式参数
                        if arg_count < init_func.required_params - 1 {
                            let msg = format!(
                                "Constructor expected at least {} arguments but got {}",
                                init_func.required_params - 1, arg_count
                            );
                            return Err(self.runtime_error(&msg));
                        }
                        
                        if !init_func.has_variadic && arg_count > expected_args {
                            let msg = format!(
                                "Constructor expected at most {} arguments but got {}",
                                expected_args, arg_count
                            );
                            return Err(self.runtime_error(&msg));
                        }
                        
                        // 将实例插入到参数下方（作为 this）
                        // 当前栈: [..., arg1, arg2, ..., argN]
                        // 目标栈: [..., instance, arg1, arg2, ..., argN]
                        let insert_pos = self.stack.len() - arg_count;
                        self.stack.insert(insert_pos, instance_value.clone());
                        
                        // 处理默认参数
                        let fixed_params = if init_func.has_variadic { init_func.arity - 1 } else { init_func.arity };
                        let actual_args = arg_count + 1; // +1 for this
                        let missing_count = fixed_params.saturating_sub(actual_args);
                        if missing_count > 0 && !init_func.defaults.is_empty() {
                            let defaults_start = init_func.defaults.len().saturating_sub(missing_count);
                            for i in 0..missing_count {
                                if defaults_start + i < init_func.defaults.len() {
                                    self.push(init_func.defaults[defaults_start + i].clone());
                                }
                            }
                        }
                        
                        // 检查调用深度
                        if self.frames.len() >= MAX_FRAMES {
                            return Err(self.runtime_error("Stack overflow: too many nested calls"));
                        }
                        
                        // 创建调用帧
                        let frame = CallFrame {
                            return_ip: self.ip as u32,
                            base_slot: insert_pos as u16,
                            is_method_call: true, // init 方法调用
                        };
                        self.frames.push(frame);
                        
                        // 跳转到 init 方法
                        self.ip = init_func.chunk_index;
                    } else {
                        // 没有 init 方法，直接返回实例
                        // 但需要弹出传入的参数
                        for _ in 0..arg_count {
                            self.pop()?;
                        }
                        self.push(instance_value);
                    }
                }
                
                OpCode::GetStatic => {
                    let class_name_index = self.read_u16() as usize;
                    let field_name_index = self.read_u16() as usize;
                    
                    let class_name = if let Some(s) = self.chunk.constants[class_name_index].as_string() {
                        s.clone()
                    } else {
                        return Err(self.runtime_error("Invalid class name"));
                    };
                    
                    let field_name = if let Some(s) = self.chunk.constants[field_name_index].as_string() {
                        s.clone()
                    } else {
                        return Err(self.runtime_error("Invalid field name"));
                    };
                    
                    // 检查是否是枚举变体访问
                    if let Some(enum_info) = self.chunk.get_enum(&class_name).cloned() {
                        // 查找变体
                        let variant = enum_info.variants.iter().find(|v| v.name == field_name);
                        if let Some(variant) = variant {
                            // 创建枚举实例
                            let value = if let Some(value_idx) = variant.value_index {
                                Some(self.chunk.constants[value_idx as usize].clone())
                            } else {
                                None
                            };
                            let enum_val = super::value::EnumVariantValue {
                                enum_name: class_name.clone(),
                                variant_name: variant.name.clone(),
                                value,
                                associated_data: std::collections::HashMap::new(),
                            };
                            self.push(Value::enum_val(Box::new(enum_val)));
                            continue;
                        } else {
                            return Err(self.runtime_error(&format!(
                                "Enum '{}' has no variant '{}'", class_name, field_name
                            )));
                        }
                    }
                    
                    // 构造缓存键
                    let cache_key = format!("{}::{}", class_name, field_name);
                    
                    // 检查缓存
                    if let Some(value) = self.static_fields.get(&cache_key) {
                        self.push(value.clone());
                    } else {
                        // 第一次访问，需要执行初始化函数
                        if let Some(type_info) = self.chunk.get_type(&class_name) {
                            if let Some(init_func_index) = type_info.static_fields.get(&field_name) {
                                let init_func_index = *init_func_index as usize;
                                // 获取初始化函数
                                if let Some(func) = self.chunk.constants[init_func_index].as_function() {
                                    // 保存当前状态
                                    let saved_ip = self.ip;
                                    let saved_base = self.current_base;
                                    let saved_stack_len = self.stack.len();
                                    
                                    // 执行初始化函数
                                    self.ip = func.chunk_index;
                                    self.current_base = self.stack.len();
                                    
                                    // 创建一个临时调用帧
                                    let frame = CallFrame {
                                        return_ip: 0, // 不会使用
                                        base_slot: self.current_base as u16,
                                        is_method_call: false,
                                    };
                                    self.frames.push(frame);
                                    
                                    // 执行直到返回
                                    let mut result_value: Option<Value> = None;
                                    loop {
                                        let op_byte = self.read_byte();
                                        let op = OpCode::from(op_byte);
                                        
                                        match op {
                                            OpCode::Return => {
                                                let result = self.pop()?;
                                                self.frames.pop();
                                                
                                                // 缓存结果
                                                self.static_fields.insert(cache_key.clone(), result.clone());
                                                result_value = Some(result);
                                                
                                                // 恢复状态
                                                self.ip = saved_ip;
                                                self.current_base = saved_base;
                                                self.stack.truncate(saved_stack_len);
                                                break;
                                            }
                                            OpCode::Const => {
                                                let idx = self.read_u16() as usize;
                                                let value = self.chunk.constants[idx].clone();
                                                self.push(value);
                                            }
                                            OpCode::ConstInt8 => {
                                                let value = self.read_byte() as i8 as i64;
                                                self.push(Value::int(value));
                                            }
                                            _ => {
                                                // 其他指令可能需要处理
                                                return Err(self.runtime_error(&format!(
                                                    "Unsupported opcode in static initializer: {:?}", op
                                                )));
                                            }
                                        }
                                    }
                                    
                                    // 在恢复栈后推送结果
                                    if let Some(result) = result_value {
                                        self.push(result);
                                    }
                                } else {
                                    // 不是函数，直接使用常量值
                                    let value = self.chunk.constants[init_func_index].clone();
                                    self.static_fields.insert(cache_key, value.clone());
                                    self.push(value);
                                }
                            } else {
                                return Err(self.runtime_error(&format!(
                                    "Unknown static field '{}.{}'", class_name, field_name
                                )));
                            }
                        } else {
                            return Err(self.runtime_error(&format!(
                                "Unknown class '{}'", class_name
                            )));
                        }
                    }
                }
                
                OpCode::SetStatic => {
                    let _class_name_index = self.read_u16();
                    let _field_name_index = self.read_u16();
                    // 静态字段设置 - 简化实现
                }
                
                OpCode::InvokeStatic => {
                    let class_name_index = self.read_u16() as usize;
                    let method_name_index = self.read_u16() as usize;
                    let arg_count = self.read_byte() as usize;
                    
                    let class_name = if let Some(s) = self.chunk.constants[class_name_index].as_string() {
                        s.clone()
                    } else {
                        return Err(self.runtime_error("Invalid class name"));
                    };
                    
                    let method_name = if let Some(s) = self.chunk.constants[method_name_index].as_string() {
                        s.clone()
                    } else {
                        return Err(self.runtime_error("Invalid method name"));
                    };
                    
                    // 检查是否是枚举的内置方法
                    if let Some(enum_info) = self.chunk.get_enum(&class_name).cloned() {
                        if method_name == "fromValue" {
                            // Enum.fromValue(value) - 根据值查找枚举变体
                            if arg_count != 1 {
                                return Err(self.runtime_error("fromValue() expects exactly 1 argument"));
                            }
                            let search_value = self.pop()?;
                            
                            // 遍历所有变体查找匹配的值
                            let mut found = None;
                            for variant in &enum_info.variants {
                                if let Some(value_idx) = variant.value_index {
                                    let variant_value = &self.chunk.constants[value_idx as usize];
                                    if *variant_value == search_value {
                                        // 创建枚举实例
                                        let enum_val = super::value::EnumVariantValue {
                                            enum_name: class_name.clone(),
                                            variant_name: variant.name.clone(),
                                            value: Some(search_value.clone()),
                                            associated_data: std::collections::HashMap::new(),
                                        };
                                        found = Some(Value::enum_val(Box::new(enum_val)));
                                        break;
                                    }
                                }
                            }
                            
                            if let Some(enum_val) = found {
                                self.push(enum_val);
                            } else {
                                // 未找到匹配的值，返回 null
                                self.push(Value::null());
                            }
                            continue;
                        } else if method_name == "values" {
                            // Enum.values() - 返回所有变体的数组
                            let mut values = Vec::new();
                            for variant in &enum_info.variants {
                                let value = if let Some(value_idx) = variant.value_index {
                                    Some(self.chunk.constants[value_idx as usize].clone())
                                } else {
                                    None
                                };
                                let enum_val = super::value::EnumVariantValue {
                                    enum_name: class_name.clone(),
                                    variant_name: variant.name.clone(),
                                    value,
                                    associated_data: std::collections::HashMap::new(),
                                };
                                values.push(Value::enum_val(Box::new(enum_val)));
                            }
                            
                            // 弹出可能的参数（虽然 values() 不需要参数）
                            for _ in 0..arg_count {
                                self.pop()?;
                            }
                            
                            self.push(Value::array(Arc::new(Mutex::new(values))));
                            continue;
                        }
                    }
                    
                    // 查找静态方法
                    let func_index = match self.chunk.get_static_method(&class_name, &method_name) {
                        Some(idx) => idx as usize,
                        None => return Err(self.runtime_error(&format!(
                            "Class '{}' has no static method '{}'",
                            class_name, method_name
                        ))),
                    };
                    
                    let func = if let Some(f) = self.chunk.constants[func_index].as_function() {
                        f.clone()
                    } else {
                        return Err(self.runtime_error("Static method is not a function"));
                    };
                    
                    // 检查参数
                    if arg_count < func.required_params {
                        let msg = format!(
                            "Static method '{}' expected at least {} arguments but got {}",
                            method_name, func.required_params, arg_count
                        );
                        return Err(self.runtime_error(&msg));
                    }
                    
                    // 创建调用帧
                    let base = self.stack.len() - arg_count;
                    
                    // 填充默认参数
                    let missing = func.arity.saturating_sub(arg_count);
                    if missing > 0 && !func.defaults.is_empty() {
                        let start = func.defaults.len().saturating_sub(missing);
                        for i in 0..missing {
                            if start + i < func.defaults.len() {
                                self.push(func.defaults[start + i].clone());
                            }
                        }
                    }
                    
                    if self.frames.len() >= MAX_FRAMES {
                        return Err(self.runtime_error("Stack overflow"));
                    }
                    
                    // 静态方法不需要在栈上插入函数值，直接使用参数位置
                    let frame = CallFrame {
                        return_ip: self.ip as u32,
                        base_slot: base as u16,
                        is_method_call: true, // 没有函数值在栈上，类似方法调用
                    };
                    self.frames.push(frame);
                    self.current_base = base;
                    self.ip = func.chunk_index;
                }
                
                OpCode::InvokeSuper => {
                    let method_name_index = self.read_u16() as usize;
                    let arg_count = self.read_byte() as usize;
                    
                    let method_name = if let Some(s) = self.chunk.constants[method_name_index].as_string() {
                        s.clone()
                    } else {
                        return Err(self.runtime_error("Invalid method name"));
                    };
                    
                    // 获取 this（在参数下方）
                    let receiver_idx = self.stack.len() - arg_count - 1;
                    let receiver = self.stack[receiver_idx].clone();
                    
                    // 获取父类名
                    let parent_class = if let Some(c) = receiver.as_class() {
                        c.lock().parent_class.clone()
                    } else {
                        return Err(self.runtime_error("super can only be used in a class method"));
                    };
                    
                    let parent_name = match parent_class {
                        Some(name) => name,
                        None => return Err(self.runtime_error("Class has no parent")),
                    };
                    
                    // 查找父类方法
                    let func_index = match self.chunk.get_method(&parent_name, &method_name) {
                        Some(idx) => idx as usize,
                        None => return Err(self.runtime_error(&format!(
                            "Parent class '{}' has no method '{}'",
                            parent_name, method_name
                        ))),
                    };
                    
                    let func = if let Some(f) = self.chunk.constants[func_index].as_function() {
                        f.clone()
                    } else {
                        return Err(self.runtime_error("Method is not a function"));
                    };
                    
                    if self.frames.len() >= MAX_FRAMES {
                        return Err(self.runtime_error("Stack overflow"));
                    }
                    
                    let frame = CallFrame {
                        return_ip: self.ip as u32,
                        base_slot: receiver_idx as u16,
                        is_method_call: true, // super 方法调用
                    };
                    self.frames.push(frame);
                    self.current_base = receiver_idx;
                    self.ip = func.chunk_index;
                }
                
                OpCode::Dup => {
                    let value = self.peek()?.clone();
                    self.push(value);
                }
                
                OpCode::SetupTry => {
                    // 读取 catch 块的偏移量
                    let catch_offset = self.read_u16() as i16;
                    // 记录异常处理器（当前 IP + 偏移量 = catch 块地址）
                    // 注意：这里需要更完整的实现，使用异常处理器栈
                    // 暂时存储在一个简单的字段中
                    let catch_ip = (self.ip as i32 + catch_offset as i32) as usize;
                    self.exception_handlers.push(ExceptionHandler {
                        catch_ip,
                        stack_depth: self.stack.len(),
                        frame_depth: self.frames.len(),
                    });
                }
                
                OpCode::Throw => {
                    use crate::stdlib::exception::THROWABLE_TYPES;
                    
                    // 弹出异常值
                    let exception = self.pop()?;
                    
                    // 检查抛出的值是否是 Throwable 类型
                    let is_valid_throwable = if let Some(instance) = exception.as_class() {
                        // 检查类实例是否是 Throwable 或其子类
                        let class_name = &instance.lock().class_name;
                        self.is_throwable_class(class_name)
                    } else if let Some(s) = exception.as_string() {
                        // 检查字符串格式的异常（临时兼容）
                        THROWABLE_TYPES.iter().any(|t| {
                            s.starts_with(&format!("{}:", t)) || s == *t
                        })
                    } else {
                        false
                    };
                    
                    if !is_valid_throwable {
                        return Err(self.runtime_error(
                            "throw statement requires a Throwable instance"
                        ));
                    }
                    
                    // 查找最近的异常处理器
                    if let Some(handler) = self.exception_handlers.pop() {
                        // 恢复栈到处理器设置时的深度
                        self.stack.truncate(handler.stack_depth);
                        // 恢复调用帧
                        while self.frames.len() > handler.frame_depth {
                            self.frames.pop();
                        }
                        // 压入异常值（供 catch 块使用）
                        self.push(exception);
                        // 跳转到 catch 块
                        self.ip = handler.catch_ip;
                    } else {
                        // 没有异常处理器，返回错误
                        return Err(self.runtime_error(&format!("Uncaught exception: {}", exception)));
                    }
                }
                
                OpCode::Halt => {
                    return Ok(());
                }
                
                // ============ 并发指令 ============
                OpCode::GoSpawn => {
                    use super::value::{ChannelState, WaitGroupState};
                    use std::sync::atomic::Ordering;
                    
                    let arg_count = self.read_byte() as usize;
                    
                    // 从栈上获取参数
                    let mut args = Vec::with_capacity(arg_count);
                    for _ in 0..arg_count {
                        args.push(self.pop()?);
                    }
                    args.reverse(); // 恢复参数顺序
                    let callee = self.pop()?;
                    
                    // 获取函数对象
                    if let Some(func) = callee.as_function() {
                        let chunk = self.chunk.clone();
                        let func = func.clone();
                        
                        // 简化实现：使用标准线程执行协程
                        // 注意：这是一个临时的简化实现，后续会改为真正的协程调度
                        std::thread::spawn(move || {
                            // 创建协程 VM（同步执行）
                            let mut coroutine_vm = VM::new_sync(chunk, Locale::En);
                            
                            // 压入函数值（占位）
                            coroutine_vm.push_fast(Value::null());
                            
                            // 设置参数
                            for arg in &args {
                                coroutine_vm.push_fast(arg.clone());
                            }
                            
                            // 创建调用帧，return_ip 设为一个特殊值表示协程退出
                            coroutine_vm.frames.push(CallFrame {
                                return_ip: u32::MAX, // 特殊标记：协程返回时退出
                                base_slot: 1,
                                is_method_call: false,
                            });
                            coroutine_vm.current_base = 1;
                            
                            // 跳转到函数体
                            coroutine_vm.ip = func.chunk_index;
                            
                            // 同步执行协程
                            if let Err(e) = coroutine_vm.run_coroutine() {
                                eprintln!("Coroutine error at line {}: {}", e.line, e.message);
                            }
                        });
                    } else {
                        return Err(self.runtime_error(&format!("Cannot spawn {}", callee.type_name())));
                    }
                    
                    // go 表达式返回 null
                    self.push_fast(Value::null());
                }
                
                OpCode::ChannelNew => {
                    use super::value::ChannelState;
                    
                    // 创建 Channel（容量为 0 表示无缓冲）
                    let capacity = self.read_u16() as usize;
                    
                    let (sender, receiver) = if capacity == 0 {
                        // 无缓冲 channel（rendezvous）
                        crossbeam_channel::bounded(0)
                    } else {
                        // 有缓冲 channel
                        crossbeam_channel::bounded(capacity)
                    };
                    
                    let state = Arc::new(Mutex::new(ChannelState {
                        sender: Arc::new(Mutex::new(Some(sender))),
                        receiver: Arc::new(Mutex::new(Some(receiver))),
                        closed: Arc::new(AtomicBool::new(false)),
                    }));
                    
                    self.push_fast(Value::channel(state));
                }
                
                OpCode::ChannelSend => {
                    let value = self.pop()?;
                    let channel = self.pop()?;
                    
                    if let Some(ch_state) = channel.as_channel() {
                        let state = ch_state.lock();
                        let sender = state.sender.lock();
                        
                        if let Some(ref s) = *sender {
                            if s.send(value).is_err() {
                                return Err(self.runtime_error("Channel send failed: receiver closed"));
                            }
                        } else {
                            return Err(self.runtime_error("Channel is closed"));
                        }
                    } else {
                        return Err(self.runtime_error(&format!("Cannot send to {}", channel.type_name())));
                    }
                    
                    self.push_fast(Value::null());
                }
                
                OpCode::ChannelReceive => {
                    let channel = self.pop()?;
                    
                    if let Some(ch_state) = channel.as_channel() {
                        let state = ch_state.lock();
                        let receiver = state.receiver.lock();
                        
                        if let Some(ref r) = *receiver {
                            // 阻塞接收
                            match r.recv() {
                                Ok(value) => {
                                    self.push_fast(value);
                                }
                                Err(_) => {
                                    // Channel 已关闭且为空
                                    self.push_fast(Value::null());
                                }
                            }
                        } else {
                            return Err(self.runtime_error("Channel receiver is closed"));
                        }
                    } else {
                        return Err(self.runtime_error(&format!("Cannot receive from {}", channel.type_name())));
                    }
                }
                
                OpCode::ChannelTrySend => {
                    let value = self.pop()?;
                    let channel = self.pop()?;
                    
                    if let Some(ch_state) = channel.as_channel() {
                        let state = ch_state.lock();
                        let sender = state.sender.lock();
                        
                        if let Some(ref s) = *sender {
                            let success = s.send(value).is_ok();
                            self.push_fast(Value::bool(success));
                        } else {
                            self.push_fast(Value::bool(false));
                        }
                    } else {
                        return Err(self.runtime_error(&format!("Cannot send to {}", channel.type_name())));
                    }
                }
                
                OpCode::ChannelTryReceive => {
                    let channel = self.pop()?;
                    
                    if let Some(ch_state) = channel.as_channel() {
                        let state = ch_state.lock();
                        let receiver = state.receiver.lock();
                        
                        if let Some(ref r) = *receiver {
                            match r.try_recv() {
                                Ok(value) => {
                                    self.push_fast(value);
                                    self.push_fast(Value::bool(true));
                                }
                                Err(_) => {
                                    self.push_fast(Value::null());
                                    self.push_fast(Value::bool(false));
                                }
                            }
                        } else {
                            self.push_fast(Value::null());
                            self.push_fast(Value::bool(false));
                        }
                    } else {
                        return Err(self.runtime_error(&format!("Cannot receive from {}", channel.type_name())));
                    }
                }
                
                OpCode::ChannelClose => {
                    use std::sync::atomic::Ordering;
                    
                    let channel = self.pop()?;
                    
                    if let Some(ch_state) = channel.as_channel() {
                        let state = ch_state.lock();
                        state.closed.store(true, Ordering::Relaxed);
                        
                        // 关闭 sender
                        let mut sender = state.sender.lock();
                        *sender = None;
                    } else {
                        return Err(self.runtime_error(&format!("Cannot close {}", channel.type_name())));
                    }
                    
                    self.push_fast(Value::null());
                }
                
                OpCode::MutexNew => {
                    let initial_value = self.pop()?;
                    let mutex = Arc::new(Mutex::new(initial_value));
                    self.push_fast(Value::mutex(mutex));
                }
                
                OpCode::MutexLock => {
                    let mutex_val = self.pop()?;
                    
                    if let Some(m) = mutex_val.as_mutex() {
                        // 返回被锁定的值（简化版本，实际应该返回 guard）
                        let value = m.lock().clone();
                        self.push_fast(value);
                    } else {
                        return Err(self.runtime_error(&format!("Cannot lock {}", mutex_val.type_name())));
                    }
                }
                
                OpCode::WaitGroupNew => {
                    use super::value::WaitGroupState;
                    
                    // 使用优化的 WaitGroupState
                    let state = Arc::new(WaitGroupState::new());
                    self.push_fast(Value::waitgroup(state));
                }
                
                OpCode::WaitGroupAdd => {
                    let delta = self.pop()?;
                    let wg = self.pop()?;
                    
                    if let Some(state) = wg.as_waitgroup() {
                        if let Some(n) = delta.as_int() {
                            // 使用优化的 add 方法
                            state.add(n as isize);
                        } else {
                            return Err(self.runtime_error("WaitGroup add requires int"));
                        }
                    } else {
                        return Err(self.runtime_error(&format!("Cannot add to {}", wg.type_name())));
                    }
                    
                    self.push_fast(Value::null());
                }
                
                OpCode::WaitGroupDone => {
                    let wg = self.pop()?;
                    
                    if let Some(state) = wg.as_waitgroup() {
                        // 使用优化的 done 方法
                        state.done();
                    } else {
                        return Err(self.runtime_error(&format!("Cannot done on {}", wg.type_name())));
                    }
                    
                    self.push_fast(Value::null());
                }
                
                OpCode::WaitGroupWait => {
                    let wg = self.pop()?;
                    
                    if let Some(state) = wg.as_waitgroup() {
                        // 使用优化的 wait 方法（包含快速路径 + 自旋 + 阻塞）
                        state.wait();
                    } else {
                        return Err(self.runtime_error(&format!("Cannot wait on {}", wg.type_name())));
                    }
                    
                    self.push_fast(Value::null());
                }
                
                // ============ Select 指令 ============
                OpCode::SelectBegin => {
                    // 创建 select builder（使用数组存储 cases）
                    let _case_count = self.read_byte();
                    let cases: Vec<Value> = Vec::new();
                    self.push(Value::array(Arc::new(Mutex::new(cases))));
                }
                
                OpCode::SelectAddSend => {
                    // 添加发送 case
                    // 栈: [..., builder, channel, value]
                    let value = self.pop()?;
                    let channel = self.pop()?;
                    let builder = self.pop()?;
                    
                    if let Some(arr) = builder.as_array() {
                        let mut arr = arr.lock();
                        // 存储 case 类型 (0=send), channel, value
                        arr.push(Value::int(0)); // type: send
                        arr.push(channel);
                        arr.push(value);
                        drop(arr);
                        self.push(builder);
                    } else {
                        return Err(self.runtime_error("Invalid select builder"));
                    }
                }
                
                OpCode::SelectAddRecv => {
                    // 添加接收 case
                    // 栈: [..., builder, channel]
                    let channel = self.pop()?;
                    let builder = self.pop()?;
                    
                    if let Some(arr) = builder.as_array() {
                        let mut arr = arr.lock();
                        // 存储 case 类型 (1=recv), channel
                        arr.push(Value::int(1)); // type: recv
                        arr.push(channel);
                        arr.push(Value::null()); // placeholder
                        drop(arr);
                        self.push(builder);
                    } else {
                        return Err(self.runtime_error("Invalid select builder"));
                    }
                }
                
                OpCode::SelectAddDefault => {
                    // 添加 default case
                    let builder = self.pop()?;
                    
                    if let Some(arr) = builder.as_array() {
                        let mut arr = arr.lock();
                        arr.push(Value::int(2)); // type: default
                        arr.push(Value::null()); // placeholder
                        arr.push(Value::null()); // placeholder
                        drop(arr);
                        self.push(builder);
                    } else {
                        return Err(self.runtime_error("Invalid select builder"));
                    }
                }
                
                OpCode::SelectExec => {
                    use crossbeam_channel::{select, Sender, Receiver};
                    use std::sync::atomic::Ordering;
                    
                    let builder = self.pop()?;
                    
                    if let Some(arr) = builder.as_array() {
                        let arr = arr.lock();
                        
                        // 收集所有 cases 的信息
                        struct CaseInfo {
                            case_type: i64, // 0=send, 1=recv, 2=default
                            sender: Option<Sender<Value>>,
                            receiver: Option<Receiver<Value>>,
                            value: Option<Value>,
                            closed: bool,
                        }
                        
                        let mut cases_info: Vec<CaseInfo> = Vec::new();
                        let mut default_idx: Option<usize> = None;
                        
                        let mut i = 0;
                        while i + 2 < arr.len() {
                            let case_type = arr[i].as_int().unwrap_or(0);
                            match case_type {
                                0 => {
                                    // Send
                                    if let Some(ch) = arr[i + 1].as_channel() {
                                        let state = ch.lock();
                                        let sender = state.sender.lock().clone();
                                        let closed = state.closed.load(Ordering::Acquire);
                                        cases_info.push(CaseInfo {
                                            case_type: 0,
                                            sender,
                                            receiver: None,
                                            value: Some(arr[i + 2].clone()),
                                            closed,
                                        });
                                    }
                                }
                                1 => {
                                    // Recv
                                    if let Some(ch) = arr[i + 1].as_channel() {
                                        let state = ch.lock();
                                        let receiver = state.receiver.lock().clone();
                                        let closed = state.closed.load(Ordering::Acquire);
                                        cases_info.push(CaseInfo {
                                            case_type: 1,
                                            sender: None,
                                            receiver,
                                            value: None,
                                            closed,
                                        });
                                    }
                                }
                                2 => {
                                    // Default
                                    default_idx = Some(cases_info.len());
                                    cases_info.push(CaseInfo {
                                        case_type: 2,
                                        sender: None,
                                        receiver: None,
                                        value: None,
                                        closed: false,
                                    });
                                }
                                _ => {}
                            }
                            i += 3;
                        }
                        drop(arr);
                        
                        // 简单实现：轮询检查可用的 case
                        let mut result_type = -1i64;
                        let mut result_idx = -1i64;
                        let mut result_value = Value::null();
                        
                        // 首先尝试非阻塞操作
                        for (idx, case) in cases_info.iter().enumerate() {
                            match case.case_type {
                                0 => {
                                    // Try send
                                    if !case.closed {
                                        if let (Some(sender), Some(val)) = (&case.sender, &case.value) {
                                            if let Ok(()) = sender.try_send(val.clone()) {
                                                result_type = 0;
                                                result_idx = idx as i64;
                                                break;
                                            }
                                        }
                                    }
                                }
                                1 => {
                                    // Try recv
                                    if let Some(receiver) = &case.receiver {
                                        match receiver.try_recv() {
                                            Ok(val) => {
                                                result_type = 1;
                                                result_idx = idx as i64;
                                                result_value = val;
                                                break;
                                            }
                                            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                                                result_type = 2; // closed
                                                result_idx = idx as i64;
                                                break;
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        
                        // 如果没有成功且有 default，执行 default
                        if result_type == -1 {
                            if let Some(idx) = default_idx {
                                result_type = 3;
                                result_idx = idx as i64;
                            } else {
                                // 没有 default，需要阻塞等待
                                // 使用简单的轮询
                                'outer: loop {
                                    for (idx, case) in cases_info.iter().enumerate() {
                                        match case.case_type {
                                            0 => {
                                                if !case.closed {
                                                    if let (Some(sender), Some(val)) = (&case.sender, &case.value) {
                                                        if let Ok(()) = sender.try_send(val.clone()) {
                                                            result_type = 0;
                                                            result_idx = idx as i64;
                                                            break 'outer;
                                                        }
                                                    }
                                                }
                                            }
                                            1 => {
                                                if let Some(receiver) = &case.receiver {
                                                    match receiver.try_recv() {
                                                        Ok(val) => {
                                                            result_type = 1;
                                                            result_idx = idx as i64;
                                                            result_value = val;
                                                            break 'outer;
                                                        }
                                                        Err(crossbeam_channel::TryRecvError::Disconnected) => {
                                                            result_type = 2;
                                                            result_idx = idx as i64;
                                                            break 'outer;
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    
                                    // 检查是否所有 channel 都关闭
                                    let all_closed = cases_info.iter().all(|c| {
                                        c.case_type == 2 || c.closed || 
                                        (c.case_type == 1 && c.receiver.as_ref().map(|r| r.is_empty() && r.len() == 0).unwrap_or(true))
                                    });
                                    
                                    if all_closed {
                                        result_type = 4; // all_closed
                                        break;
                                    }
                                    
                                    std::thread::sleep(std::time::Duration::from_micros(10));
                                }
                            }
                        }
                        
                        self.push(Value::int(result_type));
                        self.push(Value::int(result_idx));
                        self.push(result_value);
                    } else {
                        return Err(self.runtime_error("Invalid select builder"));
                    }
                }
                
                OpCode::SelectTryExec => {
                    use std::sync::atomic::Ordering;
                    
                    let builder = self.pop()?;
                    
                    if let Some(arr) = builder.as_array() {
                        let arr = arr.lock();
                        
                        let mut result_type = -1i64;
                        let mut result_idx = -1i64;
                        let mut result_value = Value::null();
                        let mut default_idx: Option<usize> = None;
                        
                        let mut i = 0;
                        let mut case_idx = 0usize;
                        while i + 2 < arr.len() {
                            let case_type = arr[i].as_int().unwrap_or(0);
                            match case_type {
                                0 => {
                                    // Try send
                                    if let Some(ch) = arr[i + 1].as_channel() {
                                        let state = ch.lock();
                                        let closed = state.closed.load(Ordering::Acquire);
                                        let sender = state.sender.lock().clone();
                                        drop(state);
                                        if !closed {
                                            if let Some(ref s) = sender {
                                                if let Ok(()) = s.try_send(arr[i + 2].clone()) {
                                                    result_type = 0;
                                                    result_idx = case_idx as i64;
                                                }
                                            }
                                        }
                                    }
                                }
                                1 => {
                                    // Try recv
                                    if let Some(ch) = arr[i + 1].as_channel() {
                                        let state = ch.lock();
                                        let receiver = state.receiver.lock().clone();
                                        drop(state);
                                        if let Some(ref r) = receiver {
                                            match r.try_recv() {
                                                Ok(val) => {
                                                    result_type = 1;
                                                    result_idx = case_idx as i64;
                                                    result_value = val;
                                                }
                                                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                                                    result_type = 2;
                                                    result_idx = case_idx as i64;
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                }
                                2 => {
                                    // Default
                                    default_idx = Some(case_idx);
                                }
                                _ => {}
                            }
                            
                            if result_type != -1 {
                                break;
                            }
                            
                            i += 3;
                            case_idx += 1;
                        }
                        drop(arr);
                        
                        // 如果没有成功且有 default，执行 default
                        if result_type == -1 {
                            if let Some(idx) = default_idx {
                                result_type = 3;
                                result_idx = idx as i64;
                            }
                        }
                        
                        self.push(Value::int(result_type));
                        self.push(Value::int(result_idx));
                        self.push(result_value);
                    } else {
                        return Err(self.runtime_error("Invalid select builder"));
                    }
                }
                
                // ============ 专用整数指令 (性能优化) ============
                OpCode::AddInt => {
                    // 无类型检查的整数加法
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    // SAFETY: 编译器保证这里一定是整数
                    let x = unsafe { a.as_int().unwrap_unchecked() };
                    let y = unsafe { b.as_int().unwrap_unchecked() };
                    self.push_fast(Value::int(x + y));
                }
                
                OpCode::SubInt => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    let x = unsafe { a.as_int().unwrap_unchecked() };
                    let y = unsafe { b.as_int().unwrap_unchecked() };
                    self.push_fast(Value::int(x - y));
                }
                
                OpCode::MulInt => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    let x = unsafe { a.as_int().unwrap_unchecked() };
                    let y = unsafe { b.as_int().unwrap_unchecked() };
                    self.push_fast(Value::int(x * y));
                }
                
                OpCode::DivInt => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    let x = unsafe { a.as_int().unwrap_unchecked() };
                    let y = unsafe { b.as_int().unwrap_unchecked() };
                    if y == 0 {
                        return Err(self.runtime_error("Division by zero"));
                    }
                    self.push_fast(Value::int(x / y));
                }
                
                OpCode::LtInt => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    let x = unsafe { a.as_int().unwrap_unchecked() };
                    let y = unsafe { b.as_int().unwrap_unchecked() };
                    self.push_fast(Value::bool(x < y));
                }
                
                OpCode::LeInt => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    let x = unsafe { a.as_int().unwrap_unchecked() };
                    let y = unsafe { b.as_int().unwrap_unchecked() };
                    self.push_fast(Value::bool(x <= y));
                }
                
                OpCode::GtInt => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    let x = unsafe { a.as_int().unwrap_unchecked() };
                    let y = unsafe { b.as_int().unwrap_unchecked() };
                    self.push_fast(Value::bool(x > y));
                }
                
                OpCode::GeInt => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    let x = unsafe { a.as_int().unwrap_unchecked() };
                    let y = unsafe { b.as_int().unwrap_unchecked() };
                    self.push_fast(Value::bool(x >= y));
                }
                
                OpCode::EqInt => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    let x = unsafe { a.as_int().unwrap_unchecked() };
                    let y = unsafe { b.as_int().unwrap_unchecked() };
                    self.push_fast(Value::bool(x == y));
                }
                
                OpCode::NeInt => {
                    let b = self.pop_fast();
                    let a = self.pop_fast();
                    let x = unsafe { a.as_int().unwrap_unchecked() };
                    let y = unsafe { b.as_int().unwrap_unchecked() };
                    self.push_fast(Value::bool(x != y));
                }
                
                // ============ 融合指令 ============
                OpCode::ConstInt8 => {
                    // 加载小整数常量
                    let value = self.read_byte() as i8 as i64;
                    self.push_fast(Value::int(value));
                }
                
                OpCode::GetLocalInt => {
                    // 获取局部整数变量 (无类型检查)
                    let slot = self.read_u16() as usize;
                    let actual_slot = self.current_base + slot;
                    let value = unsafe { self.stack.get_unchecked(actual_slot).clone() };
                    self.push_fast(value);
                }
                
                OpCode::GetLocalAddInt => {
                    // 获取局部变量并加整数
                    let slot = self.read_u16() as usize;
                    let add_value = self.read_byte() as i8 as i64;
                    let actual_slot = self.current_base + slot;
                    let base_value = unsafe { self.stack.get_unchecked(actual_slot) };
                    let n = unsafe { base_value.as_int().unwrap_unchecked() };
                    self.push_fast(Value::int(n + add_value));
                }
                
                OpCode::GetLocalSubInt => {
                    // 获取局部变量并减整数
                    let slot = self.read_u16() as usize;
                    let sub_value = self.read_byte() as i8 as i64;
                    let actual_slot = self.current_base + slot;
                    let base_value = unsafe { self.stack.get_unchecked(actual_slot) };
                    let n = unsafe { base_value.as_int().unwrap_unchecked() };
                    self.push_fast(Value::int(n - sub_value));
                }
                
                OpCode::JumpIfFalsePop => {
                    // 条件跳转并弹出
                    let offset = self.read_u16() as usize;
                    let condition = self.pop_fast();
                    if !condition.is_truthy() {
                        self.ip += offset;
                    }
                }
                
                OpCode::TailCall => {
                    // 尾调用优化：复用当前调用帧
                    let arg_count = self.read_byte() as usize;
                    
                    let callee_idx = self.stack.len() - arg_count - 1;
                    let callee = self.stack[callee_idx].clone();
                    
                    if let Some(func) = callee.as_function() {
                        // 将参数移动到当前帧的基址位置
                        let current_base: usize = if self.frames.is_empty() {
                            0
                        } else {
                            self.frames.last().unwrap().base_slot as usize
                        };
                        
                        // 移动参数到正确位置
                        for i in 0..arg_count {
                            let arg = self.stack[callee_idx + 1 + i].clone();
                            self.stack[current_base + i] = arg;
                        }
                        
                        // 截断栈
                        self.stack.truncate(current_base + arg_count);
                        
                        // 直接跳转到函数体，不创建新帧
                        self.ip = func.chunk_index;
                    } else {
                        return Err(self.runtime_error(&format!("Cannot call {}", callee.type_name())));
                    }
                }
                
                OpCode::DecInt => {
                    // 整数递减：x - 1
                    let top = self.pop_fast();
                    let n = unsafe { top.as_int().unwrap_unchecked() };
                    self.push_fast(Value::int(n - 1));
                }
                
                OpCode::GetLocalLeInt => {
                    // 获取局部变量并与整数比较小于等于
                    let slot = self.read_u16() as usize;
                    let cmp_value = self.read_byte() as i8 as i64;
                    let actual_slot = self.current_base + slot;
                    let local = unsafe { self.stack.get_unchecked(actual_slot) };
                    let n = unsafe { local.as_int().unwrap_unchecked() };
                    self.push_fast(Value::bool(n <= cmp_value));
                }
                
                OpCode::ReturnIf => {
                    // 如果条件为真，返回值
                    let condition = self.pop_fast();
                    if condition.is_truthy() {
                        let return_value = self.pop_fast();
                        
                        if self.frames.is_empty() {
                            self.push_fast(return_value);
                            return Ok(());
                        }
                        
                        let frame = unsafe { self.frames.pop().unwrap_unchecked() };
                        let truncate_to = if frame.is_method_call {
                            frame.base_slot as usize
                        } else {
                            (frame.base_slot as usize).saturating_sub(1)
                        };
                        self.stack.truncate(truncate_to);
                        self.push_fast(return_value);
                        self.ip = frame.return_ip as usize;
                        self.current_base = self.frames.last().map(|f| f.base_slot as usize).unwrap_or(0);
                    }
                }
                
                // ============ 枚举操作 ============
                OpCode::NewEnumSimple => {
                    use super::value::EnumVariantValue;
                    
                    let enum_name_idx = self.read_u16();
                    let variant_name_idx = self.read_u16();
                    
                    let enum_name = self.chunk.get_string(enum_name_idx).to_string();
                    let variant_name = self.chunk.get_string(variant_name_idx).to_string();
                    
                    let variant = EnumVariantValue {
                        enum_name,
                        variant_name,
                        value: None,
                        associated_data: HashMap::new(),
                    };
                    self.push(Value::enum_val(Box::new(variant)));
                }
                
                OpCode::NewEnumValue => {
                    use super::value::EnumVariantValue;
                    
                    let enum_name_idx = self.read_u16();
                    let variant_name_idx = self.read_u16();
                    let associated_value = self.pop()?;
                    
                    let enum_name = self.chunk.get_string(enum_name_idx).to_string();
                    let variant_name = self.chunk.get_string(variant_name_idx).to_string();
                    
                    let variant = EnumVariantValue {
                        enum_name,
                        variant_name,
                        value: Some(associated_value),
                        associated_data: HashMap::new(),
                    };
                    self.push(Value::enum_val(Box::new(variant)));
                }
                
                OpCode::NewEnumFields => {
                    use super::value::EnumVariantValue;
                    
                    let enum_name_idx = self.read_u16();
                    let variant_name_idx = self.read_u16();
                    let field_count = self.read_byte() as usize;
                    
                    let enum_name = self.chunk.get_string(enum_name_idx).to_string();
                    let variant_name = self.chunk.get_string(variant_name_idx).to_string();
                    
                    // 收集字段（从栈上弹出 field_name, value 对）
                    let mut associated_data = HashMap::with_capacity(field_count);
                    for _ in 0..field_count {
                        let value = self.pop()?;
                        let field_name = self.pop()?;
                        if let Some(name) = field_name.as_string() {
                            associated_data.insert(name.clone(), value);
                        }
                    }
                    
                    let variant = EnumVariantValue {
                        enum_name,
                        variant_name,
                        value: None,
                        associated_data,
                    };
                    self.push(Value::enum_val(Box::new(variant)));
                }
                
                OpCode::EnumVariantName => {
                    let enum_val = self.pop()?;
                    if let Some(variant) = enum_val.as_enum() {
                        self.push(Value::string(variant.variant_name.clone()));
                    } else {
                        return Err(self.runtime_error(&format!(
                            "Expected enum value, found {}",
                            enum_val.type_name()
                        )));
                    }
                }
                
                OpCode::EnumGetValue => {
                    let enum_val = self.pop()?;
                    if let Some(variant) = enum_val.as_enum() {
                        if let Some(value) = &variant.value {
                            self.push(*value);
                        } else {
                            self.push(Value::null());
                        }
                    } else {
                        return Err(self.runtime_error(&format!(
                            "Expected enum value, found {}",
                            enum_val.type_name()
                        )));
                    }
                }
                
                OpCode::EnumGetField => {
                    let field_name_idx = self.read_u16();
                    let enum_val = self.pop()?;
                    
                    let field_name = self.chunk.get_string(field_name_idx);
                    
                    if let Some(variant) = enum_val.as_enum() {
                        if let Some(value) = variant.associated_data.get(field_name) {
                            self.push(*value);
                        } else {
                            return Err(self.runtime_error(&format!(
                                "Enum variant '{}::{}' has no field '{}'",
                                variant.enum_name, variant.variant_name, field_name
                            )));
                        }
                    } else {
                        return Err(self.runtime_error(&format!(
                            "Expected enum value, found {}",
                            enum_val.type_name()
                        )));
                    }
                }
                
                OpCode::EnumMatch => {
                    let variant_name_idx = self.read_u16();
                    let enum_val = self.pop()?;
                    
                    let variant_name = self.chunk.get_string(variant_name_idx);
                    
                    if let Some(variant) = enum_val.as_enum() {
                        let is_match = variant.variant_name == variant_name;
                        self.push(Value::bool(is_match));
                    } else {
                        // 非枚举类型总是不匹配
                        self.push(Value::bool(false));
                    }
                }
                
                // ====== 超级指令（冷路径备用） ======
                // 这些指令在热路径中已处理，这里是冷路径备用实现
                OpCode::AddLocals => {
                    let slot1 = self.read_byte() as usize;
                    let slot2 = self.read_byte() as usize;
                    let actual1 = self.current_base + slot1;
                    let actual2 = self.current_base + slot2;
                    let a = self.stack[actual1].clone();
                    let b = self.stack[actual2].clone();
                    let result = (a + b).map_err(|e| self.runtime_error(&e))?;
                    self.push_fast(result);
                }
                
                OpCode::SubLocals => {
                    let slot1 = self.read_byte() as usize;
                    let slot2 = self.read_byte() as usize;
                    let actual1 = self.current_base + slot1;
                    let actual2 = self.current_base + slot2;
                    let a = self.stack[actual1].clone();
                    let b = self.stack[actual2].clone();
                    let result = (a - b).map_err(|e| self.runtime_error(&e))?;
                    self.push_fast(result);
                }
                
                OpCode::JumpIfLocalLeConst => {
                    let slot = self.read_byte() as usize;
                    let const_val = self.read_byte() as i8 as i64;
                    let offset = self.read_i16();
                    let actual = self.current_base + slot;
                    if let Some(n) = self.stack[actual].as_int() {
                        if n <= const_val {
                            self.ip = (self.ip as isize + offset as isize) as usize;
                        }
                    }
                }
                
                OpCode::JumpIfLocalLtConst => {
                    let slot = self.read_byte() as usize;
                    let const_val = self.read_byte() as i8 as i64;
                    let offset = self.read_i16();
                    let actual = self.current_base + slot;
                    if let Some(n) = self.stack[actual].as_int() {
                        if n < const_val {
                            self.ip = (self.ip as isize + offset as isize) as usize;
                        }
                    }
                }
                
                OpCode::CallWithLocal => {
                    // 简化实现：读取操作数但不执行特殊处理
                    let _slot = self.read_byte();
                    // 使用普通调用逻辑处理
                }
                
                OpCode::ReturnLocal => {
                    let slot = self.read_byte() as usize;
                    let actual = self.current_base + slot;
                    let return_value = self.stack[actual].clone();
                    
                    if self.frames.is_empty() {
                        self.push_fast(return_value);
                        return Ok(());
                    }
                    
                    let frame = self.frames.pop().unwrap();
                    let truncate_to = if frame.is_method_call {
                        frame.base_slot as usize
                    } else {
                        (frame.base_slot as usize).saturating_sub(1)
                    };
                    self.stack.truncate(truncate_to);
                    self.push_fast(return_value);
                    self.ip = frame.return_ip as usize;
                    self.current_base = self.frames.last().map(|f| f.base_slot as usize).unwrap_or(0);
                }
                
                OpCode::ReturnInt => {
                    let value = self.read_byte() as i8 as i64;
                    let return_value = Value::int(value);
                    
                    if self.frames.is_empty() {
                        self.push_fast(return_value);
                        return Ok(());
                    }
                    
                    let frame = self.frames.pop().unwrap();
                    let truncate_to = if frame.is_method_call {
                        frame.base_slot as usize
                    } else {
                        (frame.base_slot as usize).saturating_sub(1)
                    };
                    self.stack.truncate(truncate_to);
                    self.push_fast(return_value);
                    self.ip = frame.return_ip as usize;
                    self.current_base = self.frames.last().map(|f| f.base_slot as usize).unwrap_or(0);
                }
                
                OpCode::LoadLocals2 => {
                    let slot1 = self.read_byte() as usize;
                    let slot2 = self.read_byte() as usize;
                    let actual1 = self.current_base + slot1;
                    let actual2 = self.current_base + slot2;
                    let v1 = self.stack[actual1].clone();
                    let v2 = self.stack[actual2].clone();
                    self.push_fast(v1);
                    self.push_fast(v2);
                }
                
                OpCode::RecursiveCall => {
                    // 简化实现：读取操作数但使用普通调用
                    let _arg_count = self.read_byte();
                    // 实际递归优化需要更复杂的处理
                }
            }
        }
    }
    
    /// 查看栈顶值（不弹出）
    fn peek(&self) -> Result<&Value, RuntimeError> {
        self.stack.last().ok_or_else(|| {
            let msg = format_message(messages::ERR_RUNTIME_STACK_UNDERFLOW, self.locale, &[]);
            self.runtime_error(&msg)
        })
    }

    /// 读取一个字节
    #[inline(always)]
    fn read_byte(&mut self) -> u8 {
        // SAFETY: 编译器保证 ip 在有效范围内
        let byte = unsafe { *self.chunk.code.get_unchecked(self.ip) };
        self.ip += 1;
        byte
    }

    /// 读取一个 u16（大端序）
    #[inline(always)]
    fn read_u16(&mut self) -> u16 {
        // SAFETY: 编译器保证 ip+1 在有效范围内
        unsafe {
            let high = *self.chunk.code.get_unchecked(self.ip) as u16;
            let low = *self.chunk.code.get_unchecked(self.ip + 1) as u16;
            self.ip += 2;
        (high << 8) | low
        }
    }
    
    /// 读取一个 i16（大端序，有符号）
    #[inline(always)]
    fn read_i16(&mut self) -> i16 {
        self.read_u16() as i16
    }

    /// 压栈
    #[inline(always)]
    fn push(&mut self, value: Value) {
        self.stack.push(value);
    }

    /// 出栈
    #[inline(always)]
    fn pop(&mut self) -> Result<Value, RuntimeError> {
        self.stack.pop().ok_or_else(|| {
            let msg = format_message(messages::ERR_RUNTIME_STACK_UNDERFLOW, self.locale, &[]);
            self.runtime_error(&msg)
        })
    }
    
    /// 快速出栈（无检查，仅在确定栈非空时使用）
    /// 
    /// 使用 unsafe 直接操作长度和指针，避免边界检查开销
    #[inline(always)]
    fn pop_fast(&mut self) -> Value {
        unsafe {
            let new_len = self.stack.len() - 1;
            self.stack.set_len(new_len);
            std::ptr::read(self.stack.as_ptr().add(new_len))
        }
    }
    
    /// 快速入栈（无检查，假设容量足够）
    /// 
    /// 使用 unsafe 直接写入，避免容量检查开销
    #[inline(always)]
    fn push_fast(&mut self, value: Value) {
        unsafe {
            let len = self.stack.len();
            // 确保容量足够（仅在调试模式下检查）
            debug_assert!(len < self.stack.capacity(), "stack overflow");
            std::ptr::write(self.stack.as_mut_ptr().add(len), value);
            self.stack.set_len(len + 1);
        }
    }

    /// 创建运行时错误
    /// 检查类名是否是 Throwable 或其子类
    fn is_throwable_class(&self, class_name: &str) -> bool {
        use crate::stdlib::exception::THROWABLE_TYPES;
        
        // 检查类型注册表中的继承关系
        let mut current = class_name.to_string();
        loop {
            // 检查当前类名是否在 Throwable 类型列表中
            if THROWABLE_TYPES.contains(&current.as_str()) {
                return true;
            }
            
            // 查找父类
            if let Some(type_info) = self.chunk.get_type(&current) {
                if let Some(ref parent) = type_info.parent {
                    current = parent.clone();
                    continue;
                }
            }
            
            // 没有父类了，检查失败
            break;
        }
        
        false
    }
    
    fn runtime_error(&self, message: &str) -> RuntimeError {
        let line = self.chunk.get_line(self.ip.saturating_sub(1));
        let stack_trace = self.capture_stack_trace();
        RuntimeError::with_trace(message.to_string(), line, stack_trace)
    }
    
    /// 捕获当前的栈追踪
    fn capture_stack_trace(&self) -> Vec<StackFrame> {
        let mut trace = Vec::new();
        
        // 当前执行位置
        let current_line = self.chunk.get_line(self.ip.saturating_sub(1));
        let current_func = self.get_current_function_name();
        trace.push(StackFrame {
            function_name: current_func,
            file_name: None, // TODO: 添加文件名跟踪
            line: current_line,
            column: None,
        });
        
        // 遍历调用帧（从最近的到最远的）
        for frame in self.frames.iter().rev() {
            let return_ip = frame.return_ip as usize;
            let frame_line = if return_ip > 0 {
                self.chunk.get_line(return_ip.saturating_sub(1))
            } else {
                0
            };
            
            // 获取函数名（如果可能）
            let func_name = self.get_function_name_at(return_ip);
            
            trace.push(StackFrame {
                function_name: func_name,
                file_name: None,
                line: frame_line,
                column: None,
            });
        }
        
        trace
    }
    
    /// 获取当前执行的函数名
    fn get_current_function_name(&self) -> String {
        // 尝试从调用帧中推断函数名
        // 如果没有调用帧，说明在顶层代码
        if self.frames.is_empty() {
            "<main>".to_string()
        } else {
            "<function>".to_string()
        }
    }
    
    /// 获取指定位置的函数名
    fn get_function_name_at(&self, _ip: usize) -> String {
        // TODO: 从字节码元数据中获取函数名
        // 目前返回通用名称
        "<caller>".to_string()
    }
    
    /// 调用闭包函数并返回结果
    /// 用于高阶数组方法（map、filter、reduce 等）
    fn call_closure(&mut self, func: &Arc<Function>, args: &[Value]) -> Result<Value, RuntimeError> {
        // 检查参数数量
        let provided = args.len();
        let expected = func.arity;
        
        // 保存当前状态
        let saved_ip = self.ip;
        let saved_base = self.current_base;
        
        // 压入函数值（占位，不实际使用）
        let callee_idx = self.stack.len();
        self.push_fast(Value::null());
        
        // 压入参数
        let base_slot = callee_idx + 1;
        let args_to_push = provided.min(expected);
        for i in 0..args_to_push {
            self.push_fast(args[i].clone());
        }
        
        // 填充缺失参数为 null
        for _ in args_to_push..expected {
            self.push_fast(Value::null());
        }
        
        // 检查调用深度
        if self.frames.len() >= MAX_FRAMES {
            return Err(self.runtime_error("Stack overflow in closure call"));
        }
        
        // 创建调用帧
        self.frames.push(CallFrame {
            return_ip: saved_ip as u32,
            base_slot: base_slot as u16,
            is_method_call: false,
        });
        self.current_base = base_slot;
        
        // 跳转到函数体
        self.ip = func.chunk_index;
        
        // 执行直到返回
        self.run_until_return()?;
        
        // 获取返回值
        let result = self.pop_fast();
        
        // 恢复状态（移除占位的函数值）
        self.stack.truncate(callee_idx);
        self.current_base = saved_base;
        self.ip = saved_ip;
        
        Ok(result)
    }
    
    /// 执行直到当前帧返回
    fn run_until_return(&mut self) -> Result<(), RuntimeError> {
        let target_frame_depth = self.frames.len() - 1;
        
        loop {
            let op = self.read_byte();
            
            // 检查是否是返回指令
            if op == 82 { // OpCode::Return
                let return_value = self.pop_fast();
                
                // 弹出调用帧
                let frame = unsafe { self.frames.pop().unwrap_unchecked() };
                let truncate_to = if frame.is_method_call {
                    frame.base_slot as usize
                } else {
                    (frame.base_slot as usize).saturating_sub(1)
                };
                self.stack.truncate(truncate_to);
                self.push_fast(return_value);
                
                // 恢复指令指针
                self.ip = frame.return_ip as usize;
                self.current_base = self.frames.last().map(|f| f.base_slot as usize).unwrap_or(0);
                
                // 检查是否回到目标帧
                if self.frames.len() <= target_frame_depth {
                    return Ok(());
                }
                continue;
            }
            
            // 其他指令回退让主循环处理
            self.ip -= 1;
            
            // 执行单条指令
            self.execute_single_instruction()?;
            
            // 检查帧深度
            if self.frames.len() <= target_frame_depth {
                return Ok(());
            }
        }
    }
    
    /// 执行单条指令（用于闭包内部调用）
    fn execute_single_instruction(&mut self) -> Result<(), RuntimeError> {
        let op = self.read_byte();
        let opcode = OpCode::from(op);
        
        match opcode {
            OpCode::ConstInt8 => {
                let value = self.read_byte() as i8 as i64;
                self.push_fast(Value::int(value));
            }
            OpCode::GetLocal | OpCode::GetLocalInt => {
                let slot = self.read_u16() as usize;
                let actual_slot = self.current_base + slot;
                let value = unsafe { self.stack.get_unchecked(actual_slot).clone() };
                self.push_fast(value);
            }
            OpCode::SetLocal => {
                let slot = self.read_u16() as usize;
                let value = self.peek()?.clone();
                let actual_slot = self.current_base + slot;
                self.stack[actual_slot] = value;
            }
            OpCode::GetUpvalue => {
                let _index = self.read_u16() as usize;
                self.push_fast(Value::null());
            }
            OpCode::SetUpvalue => {
                let _index = self.read_u16() as usize;
            }
            OpCode::CloseUpvalue => {
                let _slot = self.read_u16() as usize;
            }
            OpCode::Const => {
                let index = self.read_u16() as usize;
                let value = unsafe { self.chunk.constants.get_unchecked(index).clone() };
                self.push_fast(value);
            }
            OpCode::Pop => {
                self.pop()?;
            }
            OpCode::AddInt => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                let x = unsafe { a.as_int().unwrap_unchecked() };
                let y = unsafe { b.as_int().unwrap_unchecked() };
                self.push_fast(Value::int(x + y));
            }
            OpCode::SubInt => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                let x = unsafe { a.as_int().unwrap_unchecked() };
                let y = unsafe { b.as_int().unwrap_unchecked() };
                self.push_fast(Value::int(x - y));
            }
            OpCode::MulInt => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                let x = unsafe { a.as_int().unwrap_unchecked() };
                let y = unsafe { b.as_int().unwrap_unchecked() };
                self.push_fast(Value::int(x * y));
            }
            OpCode::LeInt => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                let x = unsafe { a.as_int().unwrap_unchecked() };
                let y = unsafe { b.as_int().unwrap_unchecked() };
                self.push_fast(Value::bool(x <= y));
            }
            OpCode::LtInt => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                let x = unsafe { a.as_int().unwrap_unchecked() };
                let y = unsafe { b.as_int().unwrap_unchecked() };
                self.push_fast(Value::bool(x < y));
            }
            OpCode::GtInt => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                let x = unsafe { a.as_int().unwrap_unchecked() };
                let y = unsafe { b.as_int().unwrap_unchecked() };
                self.push_fast(Value::bool(x > y));
            }
            OpCode::GeInt => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                let x = unsafe { a.as_int().unwrap_unchecked() };
                let y = unsafe { b.as_int().unwrap_unchecked() };
                self.push_fast(Value::bool(x >= y));
            }
            OpCode::EqInt => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                let x = unsafe { a.as_int().unwrap_unchecked() };
                let y = unsafe { b.as_int().unwrap_unchecked() };
                self.push_fast(Value::bool(x == y));
            }
            OpCode::Add => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                    self.push_fast(Value::int(x + y));
                } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                    self.push_fast(Value::float(x + y));
                } else {
                    let result = (a + b).map_err(|e| self.runtime_error(&e))?;
                    self.push_fast(result);
                }
            }
            OpCode::Sub => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                    self.push_fast(Value::int(x - y));
                } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                    self.push_fast(Value::float(x - y));
                } else {
                    let result = (a - b).map_err(|e| self.runtime_error(&e))?;
                    self.push_fast(result);
                }
            }
            OpCode::Mul => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                    self.push_fast(Value::int(x * y));
                } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                    self.push_fast(Value::float(x * y));
                } else {
                    let result = (a * b).map_err(|e| self.runtime_error(&e))?;
                    self.push_fast(result);
                }
            }
            OpCode::Div => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                    if y == 0 {
                        return Err(self.runtime_error("Division by zero"));
                    }
                    self.push_fast(Value::int(x / y));
                } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                    self.push_fast(Value::float(x / y));
                } else {
                    let result = (a / b).map_err(|e| self.runtime_error(&e))?;
                    self.push_fast(result);
                }
            }
            OpCode::Mod => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                    if y == 0 {
                        return Err(self.runtime_error("Modulo by zero"));
                    }
                    self.push_fast(Value::int(x % y));
                } else {
                    let result = (a % b).map_err(|e| self.runtime_error(&e))?;
                    self.push_fast(result);
                }
            }
            OpCode::Lt => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                    self.push_fast(Value::bool(x < y));
                } else {
                    let result = a.lt(&b).map_err(|e| self.runtime_error(&e))?;
                    self.push_fast(result);
                }
            }
            OpCode::Le => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                    self.push_fast(Value::bool(x <= y));
                } else {
                    let result = a.le(&b).map_err(|e| self.runtime_error(&e))?;
                    self.push_fast(result);
                }
            }
            OpCode::Gt => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                    self.push_fast(Value::bool(x > y));
                } else {
                    let result = a.gt(&b).map_err(|e| self.runtime_error(&e))?;
                    self.push_fast(result);
                }
            }
            OpCode::Ge => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                    self.push_fast(Value::bool(x >= y));
                } else {
                    let result = a.ge(&b).map_err(|e| self.runtime_error(&e))?;
                    self.push_fast(result);
                }
            }
            OpCode::Eq => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                self.push_fast(a.eq_value(&b));
            }
            OpCode::Ne => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                self.push_fast(a.ne_value(&b));
            }
            OpCode::Not => {
                let a = self.pop_fast();
                self.push_fast(a.not());
            }
            OpCode::Neg => {
                let a = self.pop_fast();
                if let Some(x) = a.as_int() {
                    self.push_fast(Value::int(-x));
                } else if let Some(x) = a.as_float() {
                    self.push_fast(Value::float(-x));
                } else {
                    let result = (-a).map_err(|e| self.runtime_error(&e))?;
                    self.push_fast(result);
                }
            }
            OpCode::Jump => {
                let offset = self.read_u16() as usize;
                self.ip += offset;
            }
            OpCode::JumpIfFalse => {
                let offset = self.read_u16() as usize;
                let top = unsafe { self.stack.last().unwrap_unchecked() };
                if !top.is_truthy() {
                    self.ip += offset;
                }
            }
            OpCode::JumpIfFalsePop => {
                let offset = self.read_u16() as usize;
                let condition = self.pop_fast();
                if !condition.is_truthy() {
                    self.ip += offset;
                }
            }
            OpCode::JumpIfTrue => {
                let offset = self.read_u16() as usize;
                let top = unsafe { self.stack.last().unwrap_unchecked() };
                if top.is_truthy() {
                    self.ip += offset;
                }
            }
            OpCode::Loop => {
                // 安全点：向后跳转（循环）是抢占检查点
                if self.should_preempt() {
                    self.clear_preempt();
                }
                let offset = self.read_u16() as usize;
                self.ip -= offset;
            }
            OpCode::GetLocalAddInt => {
                let slot = self.read_u16() as usize;
                let add_value = self.read_byte() as i8 as i64;
                let actual_slot = self.current_base + slot;
                let base_value = unsafe { self.stack.get_unchecked(actual_slot) };
                let n = unsafe { base_value.as_int().unwrap_unchecked() };
                self.push_fast(Value::int(n + add_value));
            }
            OpCode::GetLocalSubInt => {
                let slot = self.read_u16() as usize;
                let sub_value = self.read_byte() as i8 as i64;
                let actual_slot = self.current_base + slot;
                let base_value = unsafe { self.stack.get_unchecked(actual_slot) };
                let n = unsafe { base_value.as_int().unwrap_unchecked() };
                self.push_fast(Value::int(n - sub_value));
            }
            OpCode::GetLocalLeInt => {
                let slot = self.read_u16() as usize;
                let cmp_value = self.read_byte() as i8 as i64;
                let actual_slot = self.current_base + slot;
                let local = unsafe { self.stack.get_unchecked(actual_slot) };
                let n = unsafe { local.as_int().unwrap_unchecked() };
                self.push_fast(Value::bool(n <= cmp_value));
            }
            OpCode::Call => {
                let arg_count = self.read_byte() as usize;
                let callee_idx = self.stack.len() - arg_count - 1;
                let callee = self.stack[callee_idx].clone();
                
                if let Some(func) = callee.as_function() {
                    if self.frames.len() >= MAX_FRAMES {
                        return Err(self.runtime_error("Stack overflow"));
                    }
                    let base_slot = callee_idx + 1;
                    self.frames.push(CallFrame {
                        return_ip: self.ip as u32,
                        base_slot: base_slot as u16,
                        is_method_call: false,
                    });
                    self.current_base = base_slot;
                    self.ip = func.chunk_index;
                } else {
                    return Err(self.runtime_error(&format!("Cannot call {}", callee.type_name())));
                }
            }
            OpCode::Return => {
                // 由 run_until_return 处理
                self.ip -= 1;
            }
            OpCode::Print => {
                let value = self.pop_fast();
                print!("{}", value);
                self.push_fast(Value::null());
            }
            OpCode::PrintLn => {
                let value = self.pop_fast();
                println!("{}", value);
                self.push_fast(Value::null());
            }
            _ => {
                // 其他指令暂不支持在闭包中使用
                return Err(self.runtime_error(&format!("Unsupported opcode {:?} in closure", opcode)));
            }
        }
        
        Ok(())
    }
    
    /// 尝试将值转换为指定类型，失败返回 null
    fn try_cast_value(&self, value: Value, target_type: &str) -> Value {
        match target_type {
            "int" => {
                if let Some(n) = value.as_int() {
                    Value::int(n)
                } else if let Some(f) = value.as_float() {
                    Value::int(f as i64)
                } else if let Some(b) = value.as_bool() {
                    Value::int(if b { 1 } else { 0 })
                } else if let Some(s) = value.as_string() {
                    s.parse::<i64>().map(Value::int).unwrap_or(Value::null())
                } else if let Some(c) = value.as_char() {
                    Value::int(c as i64)
                } else {
                    Value::null()
                }
            },
            "float" | "f64" => {
                if let Some(f) = value.as_float() {
                    Value::float(f)
                } else if let Some(n) = value.as_int() {
                    Value::float(n as f64)
                } else if let Some(s) = value.as_string() {
                    s.parse::<f64>().map(Value::float).unwrap_or(Value::null())
                } else {
                    Value::null()
                }
            },
            "string" => {
                let s = if let Some(s) = value.as_string() {
                    s.clone()
                } else if let Some(n) = value.as_int() {
                    n.to_string()
                } else if let Some(f) = value.as_float() {
                    f.to_string()
                } else if let Some(b) = value.as_bool() {
                    b.to_string()
                } else if let Some(c) = value.as_char() {
                    c.to_string()
                } else if value.is_null() {
                    "null".to_string()
                } else {
                    format!("{}", value)
                };
                Value::string(s)
            },
            "bool" => {
                if let Some(b) = value.as_bool() {
                    Value::bool(b)
                } else if let Some(n) = value.as_int() {
                    Value::bool(n != 0)
                } else if let Some(f) = value.as_float() {
                    Value::bool(f != 0.0)
                } else if let Some(s) = value.as_string() {
                    Value::bool(!s.is_empty())
                } else if value.is_null() {
                    Value::bool(false)
                } else {
                    Value::bool(value.is_truthy())
                }
            },
            _ => {
                // 对于自定义类型，检查类型名称是否匹配
                if let Some(s) = value.as_struct() {
                    if s.lock().type_name == target_type {
                        return value;
                    }
                }
                if let Some(c) = value.as_class() {
                    if c.lock().class_name == target_type {
                        return value;
                    }
                }
                if let Some(e) = value.as_enum() {
                    if e.enum_name == target_type {
                        return value;
                    }
                }
                Value::null()
            }
        }
    }
    
    /// 检查值是否是指定类型
    fn check_value_type(&self, value: &Value, type_name: &str) -> bool {
        match type_name {
            "int" => value.is_int(),
            "float" | "f64" => value.is_float(),
            "string" => value.is_string(),
            "bool" => value.is_bool(),
            "null" => value.is_null(),
            "array" => value.is_array(),
            "map" => value.is_map(),
            "function" => value.is_function(),
            _ => {
                // 检查自定义类型
                if let Some(s) = value.as_struct() {
                    s.lock().type_name == type_name
                } else if let Some(c) = value.as_class() {
                    c.lock().class_name == type_name
                } else if let Some(e) = value.as_enum() {
                    e.enum_name == type_name
                } else {
                    false
                }
            }
        }
    }
    
    /// 尝试调用运算符重载方法
    /// 返回 Some(result) 如果找到重载方法，否则返回 None
    fn try_operator_overload(&mut self, a: &Value, b: &Value, op_name: &str) -> Result<Option<Value>, RuntimeError> {
        // 只检查 Class 类型的运算符重载
        if let Some(instance) = a.as_class() {
            let class_name = instance.lock().class_name.clone();
            let method_name = format!("operator_{}", op_name);
            
            // 检查是否有对应的运算符重载方法
            if let Some(func_index) = self.chunk.get_method(&class_name, &method_name) {
                let func = if let Some(f) = self.chunk.constants[func_index as usize].as_function() {
                    f.clone()
                } else {
                    return Err(self.runtime_error("Operator method is not a function"));
                };
                
                // 保存当前状态
                let saved_ip = self.ip;
                
                // 设置调用帧
                let base_slot = self.stack.len();
                self.push(a.clone()); // this
                self.push(b.clone()); // other
                
                self.frames.push(CallFrame {
                    return_ip: saved_ip as u32,
                    base_slot: base_slot as u16,
                    is_method_call: true,
                });
                
                // 跳转到方法体
                self.ip = func.chunk_index;
                
                // 执行方法直到返回
                loop {
                    let op = self.read_byte();
                    let opcode = OpCode::from(op);
                    
                    if matches!(opcode, OpCode::Return) {
                        // 获取返回值
                        let result = self.pop()?;
                        
                        // 恢复调用帧
                        if let Some(frame) = self.frames.pop() {
                            self.stack.truncate(frame.base_slot as usize);
                            self.ip = frame.return_ip as usize;
                        }
                        
                        return Ok(Some(result));
                    }
                    
                    // 执行其他指令（简化处理，只支持简单的运算符重载方法）
                    self.execute_operator_instruction(opcode)?;
                }
            }
        }
        
        Ok(None)
    }
    
    /// 执行单条指令（用于运算符重载）
    fn execute_operator_instruction(&mut self, opcode: OpCode) -> Result<(), RuntimeError> {
        match opcode {
            OpCode::Const => {
                let index = self.read_u16();
                let value = self.chunk.constants[index as usize].clone();
                self.push(value);
            }
            OpCode::Pop => {
                self.pop()?;
            }
            OpCode::GetLocal => {
                let slot = self.read_u16() as usize;
                let base = self.frames.last().map(|f| f.base_slot as usize).unwrap_or(0);
                let value = self.stack[base + slot].clone();
                self.push(value);
            }
            OpCode::SetLocal => {
                let slot = self.read_u16() as usize;
                let base = self.frames.last().map(|f| f.base_slot as usize).unwrap_or(0);
                let value = self.stack.last().cloned().unwrap_or(Value::null());
                self.stack[base + slot] = value;
            }
            OpCode::GetUpvalue => {
                let _index = self.read_u16() as usize;
                self.push(Value::null());
            }
            OpCode::SetUpvalue => {
                let _index = self.read_u16() as usize;
            }
            OpCode::CloseUpvalue => {
                let _slot = self.read_u16() as usize;
            }
            OpCode::Add => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = (a + b).map_err(|e| self.runtime_error(&e))?;
                self.push(result);
            }
            OpCode::Sub => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = (a - b).map_err(|e| self.runtime_error(&e))?;
                self.push(result);
            }
            OpCode::Mul => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = (a * b).map_err(|e| self.runtime_error(&e))?;
                self.push(result);
            }
            OpCode::Div => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = (a / b).map_err(|e| self.runtime_error(&e))?;
                self.push(result);
            }
            OpCode::GetField => {
                let field_index = self.read_u16() as usize;
                let field_name = if let Some(s) = self.chunk.constants[field_index].as_string() {
                    s.clone()
                } else {
                    return Err(self.runtime_error("Invalid field name"));
                };
                let instance = self.pop()?;
                if let Some(c) = instance.as_class() {
                    let c = c.lock();
                    if let Some(value) = c.fields.get(&field_name) {
                        self.push(value.clone());
                    } else {
                        return Err(self.runtime_error(&format!(
                            "Field '{}' not found", field_name
                        )));
                    }
                } else {
                    return Err(self.runtime_error("Cannot access field on non-class"));
                }
            }
            _ => {
                // 其他指令暂不支持在运算符重载中使用
                return Err(self.runtime_error("Unsupported instruction in operator overload"));
            }
        }
        Ok(())
    }
    
    // ============== 协程支持方法 ==============
    
    /// 获取当前指令指针
    #[inline]
    pub fn ip(&self) -> usize {
        self.ip
    }
    
    /// 设置指令指针
    #[inline]
    pub fn set_ip_value(&mut self, ip: usize) {
        self.ip = ip;
    }
    
    /// 获取当前栈基址
    #[inline]
    pub fn current_base(&self) -> usize {
        self.current_base
    }
    
    /// 设置栈基址
    #[inline]
    pub fn set_current_base(&mut self, base: usize) {
        self.current_base = base;
    }
    
    /// 保存值栈快照
    pub fn save_stack(&self) -> Vec<Value> {
        self.stack.clone()
    }
    
    /// 恢复值栈
    pub fn restore_stack(&mut self, stack: &[Value]) {
        self.stack.clear();
        self.stack.extend(stack.iter().cloned());
    }
    
    /// 保存调用帧快照
    pub fn save_frames(&self) -> Vec<crate::runtime::context::CallFrameSnapshot> {
        self.frames.iter().map(|f| crate::runtime::context::CallFrameSnapshot {
            return_ip: f.return_ip,
            base_slot: f.base_slot,
            is_method_call: f.is_method_call,
        }).collect()
    }
    
    /// 恢复调用帧
    pub fn restore_frames(&mut self, frames: &[crate::runtime::context::CallFrameSnapshot]) {
        self.frames.clear();
        for f in frames {
            self.frames.push(CallFrame {
                return_ip: f.return_ip,
                base_slot: f.base_slot,
                is_method_call: f.is_method_call,
            });
        }
        // 更新 current_base
        self.current_base = self.frames.last().map(|f| f.base_slot as usize).unwrap_or(0);
    }
    
    /// 压入值（公共接口）
    pub fn push_value(&mut self, value: Value) {
        self.stack.push(value);
    }
    
    /// 执行单条指令
    /// 
    /// 返回 Ok(true) 表示继续执行，Ok(false) 表示执行完毕
    pub fn step(&mut self) -> Result<bool, RuntimeError> {
        if self.ip >= self.chunk.code.len() {
            return Ok(false);
        }
        
        let op = self.read_byte();
        let opcode = OpCode::from(op);
        
        // 检查是否结束
        match opcode {
            OpCode::Halt => return Ok(false),
            OpCode::Return => {
                let return_value = self.pop_fast();
                
                if self.frames.is_empty() {
                    return Ok(false);
                }
                
                let frame = self.frames.pop().unwrap();
                
                if frame.return_ip == u32::MAX {
                    return Ok(false);
                }
                
                let truncate_to = if frame.is_method_call {
                    frame.base_slot as usize
                } else {
                    (frame.base_slot as usize).saturating_sub(1)
                };
                self.stack.truncate(truncate_to);
                self.push_fast(return_value);
                self.ip = frame.return_ip as usize;
                self.current_base = self.frames.last().map(|f| f.base_slot as usize).unwrap_or(0);
                return Ok(true);
            }
            _ => {
                // 回退让主循环处理
                self.ip -= 1;
                
                // 使用简化的同步执行
                // 注意：这里我们无法调用 async 方法，所以对于需要异步的指令会报错
                self.step_sync()?;
                return Ok(true);
            }
        }
    }
    
    /// 同步执行单条指令（简化版本）
    fn step_sync(&mut self) -> Result<(), RuntimeError> {
        let op = self.read_byte();
        let opcode = OpCode::from(op);
        
        match opcode {
            OpCode::Const => {
                let index = self.read_u16() as usize;
                let value = unsafe { self.chunk.constants.get_unchecked(index).clone() };
                self.push_fast(value);
            }
            OpCode::Pop => {
                self.pop()?;
            }
            OpCode::Add => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                    self.push_fast(Value::int(x + y));
                } else {
                    let result = (a + b).map_err(|e| self.runtime_error(&e))?;
                    self.push_fast(result);
                }
            }
            OpCode::Sub => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                    self.push_fast(Value::int(x - y));
                } else {
                    let result = (a - b).map_err(|e| self.runtime_error(&e))?;
                    self.push_fast(result);
                }
            }
            OpCode::Mul => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                    self.push_fast(Value::int(x * y));
                } else {
                    let result = (a * b).map_err(|e| self.runtime_error(&e))?;
                    self.push_fast(result);
                }
            }
            OpCode::Div => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                    if y == 0 {
                        return Err(self.runtime_error("Division by zero"));
                    }
                    self.push_fast(Value::int(x / y));
                } else {
                    let result = (a / b).map_err(|e| self.runtime_error(&e))?;
                    self.push_fast(result);
                }
            }
            OpCode::Neg => {
                let v = self.pop_fast();
                if let Some(x) = v.as_int() {
                    self.push_fast(Value::int(-x));
                } else {
                    let result = (-v).map_err(|e| self.runtime_error(&e))?;
                    self.push_fast(result);
                }
            }
            OpCode::GetLocal => {
                let slot = self.read_u16() as usize;
                let actual_slot = self.current_base + slot;
                let value = self.stack[actual_slot].clone();
                self.push_fast(value);
            }
            OpCode::SetLocal => {
                let slot = self.read_u16() as usize;
                let value = self.peek()?.clone();
                let actual_slot = self.current_base + slot;
                self.stack[actual_slot] = value;
            }
            OpCode::GetUpvalue => {
                let _index = self.read_u16() as usize;
                self.push_fast(Value::null());
            }
            OpCode::SetUpvalue => {
                let _index = self.read_u16() as usize;
            }
            OpCode::CloseUpvalue => {
                let _slot = self.read_u16() as usize;
            }
            OpCode::Jump => {
                let offset = self.read_u16() as usize;
                self.ip = offset;
            }
            OpCode::JumpIfFalse => {
                let offset = self.read_u16() as usize;
                let cond = self.peek()?;
                if !cond.is_truthy() {
                    self.ip = offset;
                }
            }
            OpCode::JumpIfFalsePop => {
                let offset = self.read_u16() as usize;
                let cond = self.pop_fast();
                if !cond.is_truthy() {
                    self.ip = offset;
                }
            }
            OpCode::Lt => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                    self.push_fast(Value::bool(x < y));
                } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                    self.push_fast(Value::bool(x < y));
                } else {
                    return Err(self.runtime_error("Cannot compare values"));
                }
            }
            OpCode::Le => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                    self.push_fast(Value::bool(x <= y));
                } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                    self.push_fast(Value::bool(x <= y));
                } else {
                    return Err(self.runtime_error("Cannot compare values"));
                }
            }
            OpCode::Gt => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                    self.push_fast(Value::bool(x > y));
                } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                    self.push_fast(Value::bool(x > y));
                } else {
                    return Err(self.runtime_error("Cannot compare values"));
                }
            }
            OpCode::Ge => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                    self.push_fast(Value::bool(x >= y));
                } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                    self.push_fast(Value::bool(x >= y));
                } else {
                    return Err(self.runtime_error("Cannot compare values"));
                }
            }
            OpCode::Eq => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                self.push_fast(Value::bool(a == b));
            }
            OpCode::Ne => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                self.push_fast(Value::bool(a != b));
            }
            OpCode::PrintLn => {
                let value = self.pop_fast();
                println!("{}", value);
                self.push_fast(Value::null());
            }
            OpCode::Print => {
                let value = self.pop_fast();
                print!("{}", value);
                self.push_fast(Value::null());
            }
            OpCode::Call => {
                let arg_count = self.read_byte() as usize;
                let callee_idx = self.stack.len() - arg_count - 1;
                let callee = self.stack[callee_idx].clone();
                
                if let Some(func) = callee.as_function() {
                    if self.frames.len() >= MAX_FRAMES {
                        return Err(self.runtime_error("Stack overflow"));
                    }
                    let base_slot = callee_idx + 1;
                    self.frames.push(CallFrame {
                        return_ip: self.ip as u32,
                        base_slot: base_slot as u16,
                        is_method_call: false,
                    });
                    self.current_base = base_slot;
                    self.ip = func.chunk_index;
                } else {
                    return Err(self.runtime_error(&format!("Cannot call {}", callee.type_name())));
                }
            }
            OpCode::ConstInt8 => {
                let value = self.read_byte() as i8 as i64;
                self.push_fast(Value::int(value));
            }
            OpCode::AddInt => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                let x = a.as_int().unwrap_or(0);
                let y = b.as_int().unwrap_or(0);
                self.push_fast(Value::int(x + y));
            }
            OpCode::SubInt => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                let x = a.as_int().unwrap_or(0);
                let y = b.as_int().unwrap_or(0);
                self.push_fast(Value::int(x - y));
            }
            OpCode::MulInt => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                let x = a.as_int().unwrap_or(0);
                let y = b.as_int().unwrap_or(0);
                self.push_fast(Value::int(x * y));
            }
            OpCode::LtInt => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                let x = a.as_int().unwrap_or(0);
                let y = b.as_int().unwrap_or(0);
                self.push_fast(Value::bool(x < y));
            }
            OpCode::LeInt => {
                let b = self.pop_fast();
                let a = self.pop_fast();
                let x = a.as_int().unwrap_or(0);
                let y = b.as_int().unwrap_or(0);
                self.push_fast(Value::bool(x <= y));
            }
            OpCode::GetLocalInt => {
                let slot = self.read_u16() as usize;
                let actual_slot = self.current_base + slot;
                let value = self.stack[actual_slot].clone();
                self.push_fast(value);
            }
            _ => {
                return Err(self.runtime_error(&format!(
                    "Unsupported instruction {:?} in coroutine step", opcode
                )));
            }
        }
        
        Ok(())
    }
    
    // ============ VTable 和运行时类型支持 ============
    
    /// 获取或创建类型的 VTable
    pub fn get_or_create_vtable(&mut self, type_name: &str) -> std::sync::Arc<super::vtable::VTable> {
        self.vtable_registry.get_or_create(type_name)
    }
    
    /// 注册类型的 VTable
    pub fn register_vtable(&mut self, vtable: super::vtable::VTable) -> std::sync::Arc<super::vtable::VTable> {
        self.vtable_registry.register(vtable)
    }
    
    /// 检查值是否实现了指定的 trait
    pub fn value_implements_trait(&self, value: &Value, trait_name: &str) -> bool {
        // 获取值的类型名
        let type_name = if let Some(instance) = value.as_class() {
            instance.lock().class_name.clone()
        } else if let Some(instance) = value.as_struct() {
            instance.lock().type_name.clone()
        } else {
            return false;
        };
        
        // 查找类型的 VTable
        if let Some(vtable) = self.vtable_registry.lookup_by_name(&type_name) {
            return vtable.implements_trait(trait_name);
        }
        
        false
    }
    
    /// 通过 VTable 调用方法
    pub fn vtable_dispatch(&mut self, receiver: &Value, method_name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
        // 获取值的类型名
        let type_name = if let Some(instance) = receiver.as_class() {
            instance.lock().class_name.clone()
        } else if let Some(instance) = receiver.as_struct() {
            instance.lock().type_name.clone()
        } else {
            return Err(self.runtime_error(&format!(
                "Cannot dispatch method '{}' on non-object type",
                method_name
            )));
        };
        
        // 查找 VTable
        if let Some(vtable) = self.vtable_registry.lookup_by_name(&type_name) {
            if let Some(func_index) = vtable.get_method_func_index(method_name) {
                // 获取函数并克隆以避免借用冲突
                let func = self.chunk.constants[func_index].as_function().cloned();
                if let Some(func) = func {
                    return self.call_closure(&func, &args);
                }
            }
        }
        
        // 回退到类型定义中的方法查找
        if let Some(type_info) = self.chunk.get_type(&type_name).cloned() {
            if let Some(&method_index) = type_info.methods.get(method_name) {
                let func = self.chunk.constants[method_index as usize].as_function().cloned();
                if let Some(func) = func {
                    return self.call_closure(&func, &args);
                }
            }
        }
        
        Err(self.runtime_error(&format!(
            "Method '{}' not found on type '{}'",
            method_name, type_name
        )))
    }
    
    /// 初始化类型的 VTable（从类型信息构建）
    pub fn init_type_vtable(&mut self, type_name: &str) -> Option<std::sync::Arc<super::vtable::VTable>> {
        let type_info = self.chunk.get_type(type_name)?.clone();
        
        let type_id = self.vtable_registry.allocate_type_id();
        let mut vtable = if let Some(parent) = &type_info.parent {
            // 如果有父类，先获取或创建父类的 VTable
            if let Some(parent_vtable) = self.vtable_registry.lookup_by_name(parent) {
                super::vtable::VTable::with_parent(type_id, type_name, (*parent_vtable).clone())
            } else {
                super::vtable::VTable::new(type_id, type_name)
            }
        } else {
            super::vtable::VTable::new(type_id, type_name)
        };
        
        // 注册方法
        for (method_name, method_index) in &type_info.methods {
            vtable.register_method(method_name.clone(), *method_index as usize);
        }
        
        // 注：trait 实现需要在编译时处理，通过 TraitVTable 注册
        // 目前 TypeInfo 没有 traits 字段，trait 支持需要后续版本完善
        
        Some(self.vtable_registry.register(vtable))
    }
    
    /// 获取值的运行时类型信息
    pub fn get_runtime_type_info(&self, value: &Value) -> Option<super::vtable::RuntimeTypeInfo> {
        let type_name = if let Some(instance) = value.as_class() {
            instance.lock().class_name.clone()
        } else if let Some(instance) = value.as_struct() {
            instance.lock().type_name.clone()
        } else {
            return None;
        };
        
        let type_info = self.chunk.get_type(&type_name)?;
        let vtable = self.vtable_registry.lookup_by_name(&type_name);
        
        let type_id = vtable.as_ref().map(|v| v.type_id).unwrap_or(0);
        let mut info = super::vtable::RuntimeTypeInfo::new(type_id, &type_name);
        
        // 添加字段信息
        for field_name in &type_info.fields {
            info.add_field(field_name.clone(), true);
        }
        
        info.vtable = vtable;
        Some(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_code(source: &str) -> Result<(), RuntimeError> {
        use crate::lexer::Scanner;
        use crate::parser::Parser;
        use crate::compiler::Compiler;

        let mut scanner = Scanner::new(source);
        let tokens = scanner.scan_tokens();
        let mut parser = Parser::new(tokens, Locale::En);
        let program = parser.parse().unwrap();
        let mut compiler = Compiler::new(Locale::En);
        let chunk = compiler.compile(&program).unwrap();
        let chunk_arc = Arc::new(chunk);
        let mut vm = VM::new(chunk_arc, Locale::En);
        vm.run()
    }

    #[test]
    fn test_arithmetic() {
        assert!(run_code("print(1 + 2)").is_ok());
        assert!(run_code("print(10 - 3)").is_ok());
        assert!(run_code("print(4 * 5)").is_ok());
        assert!(run_code("print(20 / 4)").is_ok());
    }

    #[test]
    fn test_precedence() {
        assert!(run_code("print(1 + 2 * 3)").is_ok());
        assert!(run_code("print((1 + 2) * 3)").is_ok());
    }
    
    #[test]
    fn test_variables() {
        // 变量声明和使用
        assert!(run_code("var x = 10\nprintln(x)").is_ok());
        // 变量赋值
        assert!(run_code("var x = 10\nx = 20\nprintln(x)").is_ok());
        // 多变量
        assert!(run_code("var a = 1\nvar b = 2\nprintln(a + b)").is_ok());
    }
    
    #[test]
    fn test_constants() {
        // 常量声明和使用
        assert!(run_code("const PI = 3.14\nprintln(PI)").is_ok());
    }
    
    #[test]
    fn test_if_statement() {
        // 简单 if
        assert!(run_code("var x = 10\nif x > 5 { println(x) }").is_ok());
        // if-else
        assert!(run_code("var x = 3\nif x > 5 { println(1) } else { println(2) }").is_ok());
    }
    
    #[test]
    fn test_for_loop() {
        // 条件循环
        assert!(run_code("var i = 0\nfor i < 3 { println(i)\ni = i + 1 }").is_ok());
        // 无限循环 + break
        assert!(run_code("var i = 0\nfor { if i >= 3 { break }\nprintln(i)\ni = i + 1 }").is_ok());
    }
    
    #[test]
    fn test_scope() {
        // 作用域测试
        assert!(run_code("var x = 1\n{ var x = 2\nprintln(x) }\nprintln(x)").is_ok());
    }
    
    #[test]
    fn test_comparison() {
        assert!(run_code("println(1 == 1)").is_ok());
        assert!(run_code("println(1 != 2)").is_ok());
        assert!(run_code("println(3 < 5)").is_ok());
        assert!(run_code("println(5 <= 5)").is_ok());
        assert!(run_code("println(5 > 3)").is_ok());
        assert!(run_code("println(5 >= 5)").is_ok());
    }
    
    #[test]
    fn test_logical_not() {
        assert!(run_code("println(!true)").is_ok());
        assert!(run_code("println(!false)").is_ok());
    }
    
    #[test]
    fn test_closure_basic() {
        // 简单闭包定义和调用
        assert!(run_code("var add = func(a:int, b:int) int { return a + b }\nprintln(add(1, 2))").is_ok());
    }
    
    #[test]
    fn test_closure_no_params() {
        // 无参闭包
        assert!(run_code("var greet = func() string { return \"Hello\" }\nprintln(greet())").is_ok());
    }
    
    #[test]
    fn test_closure_implicit_return() {
        // 隐式返回 null
        assert!(run_code("var f = func() { println(42) }\nf()").is_ok());
    }
    
    #[test]
    fn test_closure_nested_calls() {
        // 嵌套调用
        let code = r#"
var double = func(x:int) int { return x * 2 }
var add_one = func(x:int) int { return x + 1 }
println(double(add_one(5)))
"#;
        assert!(run_code(code).is_ok());
    }
    
    #[test]
    fn test_logical_short_circuit_and() {
        // && 短路测试：false && 不会执行右侧
        assert!(run_code("var x = false && true\nprintln(x)").is_ok());
        assert!(run_code("var x = true && false\nprintln(x)").is_ok());
        assert!(run_code("var x = true && true\nprintln(x)").is_ok());
    }
    
    #[test]
    fn test_logical_short_circuit_or() {
        // || 短路测试：true || 不会执行右侧
        assert!(run_code("var x = true || false\nprintln(x)").is_ok());
        assert!(run_code("var x = false || true\nprintln(x)").is_ok());
        assert!(run_code("var x = false || false\nprintln(x)").is_ok());
    }
}
