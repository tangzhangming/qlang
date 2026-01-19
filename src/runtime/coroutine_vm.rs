//! 协程虚拟机
//!
//! 为协程调度优化的 VM 执行器

use std::sync::Arc;
use parking_lot::Mutex;

use crate::compiler::Chunk;
use crate::i18n::Locale;
use crate::vm::{VM, Value};
use crate::vm::vm::RuntimeError;
use crate::vm::value::Function;

use super::goroutine::{Goroutine, GoroutineStatus};
use super::context::{VMState, CallFrameSnapshot};
use super::scheduler::Scheduler;

/// 协程执行结果
#[derive(Debug)]
pub enum ExecuteResult {
    /// 执行完成
    Completed,
    /// 让出（需要重新调度）
    Yielded,
    /// 阻塞在某个操作上
    Blocked,
    /// 发生错误
    Error(RuntimeError),
}

/// 协程 VM 执行器
///
/// 管理协程的 VM 状态和执行
pub struct CoroutineVM {
    /// 底层 VM
    vm: VM,
    /// 最大执行指令数（用于时间片）
    max_instructions: usize,
    /// 当前执行的指令数
    instruction_count: usize,
}

impl CoroutineVM {
    /// 创建新的协程 VM
    pub fn new(chunk: Arc<Chunk>, locale: Locale) -> Self {
        Self {
            vm: VM::new_sync(chunk, locale),
            max_instructions: 10000,  // 默认时间片
            instruction_count: 0,
        }
    }

    /// 设置时间片大小
    pub fn set_time_slice(&mut self, instructions: usize) {
        self.max_instructions = instructions;
    }

    /// 从协程状态初始化 VM
    pub fn init_from_goroutine(&mut self, g: &Goroutine) {
        let ctx = g.context.lock();
        let state = &ctx.vm_state;
        
        // 恢复 IP
        self.vm.set_ip_value(state.ip);
        
        // 恢复栈基址
        self.vm.set_current_base(state.current_base);
        
        // 恢复值栈
        self.vm.restore_stack(&state.value_stack);
        
        // 恢复调用帧
        self.vm.restore_frames(&state.call_frames);
    }

    /// 保存 VM 状态到协程
    pub fn save_to_goroutine(&self, g: &Goroutine) {
        let mut ctx = g.context.lock();
        let state = &mut ctx.vm_state;
        
        // 保存 IP
        state.ip = self.vm.ip();
        
        // 保存栈基址
        state.current_base = self.vm.current_base();
        
        // 保存值栈
        state.value_stack = self.vm.save_stack();
        
        // 保存调用帧
        state.call_frames = self.vm.save_frames();
    }

    /// 执行协程（带时间片限制）
    pub fn execute(&mut self, g: &Goroutine) -> ExecuteResult {
        self.instruction_count = 0;
        
        // 如果协程还没开始，初始化它
        {
            let mut ctx = g.context.lock();
            if !ctx.started {
                ctx.mark_started();
                
                // 设置函数调用
                if let Some(func) = &g.func {
                    self.vm.set_ip_value(func.chunk_index);
                    
                    // 压入参数
                    for arg in &g.args {
                        self.vm.push_value(arg.clone());
                    }
                }
            } else {
                // 恢复之前的状态
                drop(ctx);
                self.init_from_goroutine(g);
            }
        }
        
        // 执行循环
        loop {
            // 检查时间片
            if self.instruction_count >= self.max_instructions {
                // 保存状态并让出
                self.save_to_goroutine(g);
                g.set_status(GoroutineStatus::Runnable);
                return ExecuteResult::Yielded;
            }
            
            // 检查抢占标记
            if g.should_preempt() {
                g.clear_preempt();
                self.save_to_goroutine(g);
                g.set_status(GoroutineStatus::Runnable);
                return ExecuteResult::Yielded;
            }
            
            // 执行一条指令
            match self.vm.step() {
                Ok(true) => {
                    // 继续执行
                    self.instruction_count += 1;
                }
                Ok(false) => {
                    // 执行完成
                    g.mark_dead();
                    return ExecuteResult::Completed;
                }
                Err(e) => {
                    // 发生错误
                    g.mark_dead();
                    return ExecuteResult::Error(e);
                }
            }
        }
    }

    /// 执行直到完成（不限时间片）
    pub fn execute_to_completion(&mut self, g: &Goroutine) -> ExecuteResult {
        // 如果协程还没开始，初始化它
        {
            let mut ctx = g.context.lock();
            if !ctx.started {
                ctx.mark_started();
                
                // 设置函数调用
                if let Some(func) = &g.func {
                    self.vm.set_ip_value(func.chunk_index);
                    
                    // 压入参数
                    for arg in &g.args {
                        self.vm.push_value(arg.clone());
                    }
                }
            } else {
                drop(ctx);
                self.init_from_goroutine(g);
            }
        }
        
        // 运行直到完成
        match self.vm.run_coroutine() {
            Ok(()) => {
                g.mark_dead();
                ExecuteResult::Completed
            }
            Err(e) => {
                g.mark_dead();
                ExecuteResult::Error(e)
            }
        }
    }
}

/// 为 VM 添加协程支持的扩展 trait
pub trait VMCoroutineExt {
    /// 获取当前 IP
    fn ip(&self) -> usize;
    
    /// 设置 IP
    fn set_ip(&mut self, ip: usize);
    
    /// 获取当前栈基址
    fn current_base(&self) -> usize;
    
    /// 设置当前栈基址
    fn set_current_base(&mut self, base: usize);
    
    /// 保存值栈
    fn save_stack(&self) -> Vec<Value>;
    
    /// 恢复值栈
    fn restore_stack(&mut self, stack: &[Value]);
    
    /// 保存调用帧
    fn save_frames(&self) -> Vec<CallFrameSnapshot>;
    
    /// 恢复调用帧
    fn restore_frames(&mut self, frames: &[CallFrameSnapshot]);
    
    /// 压入值
    fn push_value(&mut self, value: Value);
    
    /// 执行单步
    fn step(&mut self) -> Result<bool, RuntimeError>;
}

// VM 的协程扩展实现在 vm.rs 中添加
