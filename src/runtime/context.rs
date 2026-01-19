//! 协程执行上下文
//!
//! 保存协程的执行状态，用于上下文切换

use crate::vm::Value;

/// 调用帧快照
#[derive(Debug, Clone)]
pub struct CallFrameSnapshot {
    /// 返回地址
    pub return_ip: u32,
    /// 栈基址
    pub base_slot: u16,
    /// 是否是方法调用
    pub is_method_call: bool,
}

/// VM 状态快照
#[derive(Debug, Clone)]
pub struct VMState {
    /// 指令指针
    pub ip: usize,
    /// 当前栈基址
    pub current_base: usize,
    /// 值栈
    pub value_stack: Vec<Value>,
    /// 调用帧栈
    pub call_frames: Vec<CallFrameSnapshot>,
}

impl VMState {
    /// 创建空的 VM 状态
    pub fn new() -> Self {
        Self {
            ip: 0,
            current_base: 0,
            value_stack: Vec::with_capacity(256),
            call_frames: Vec::with_capacity(64),
        }
    }

    /// 重置状态
    pub fn reset(&mut self) {
        self.ip = 0;
        self.current_base = 0;
        self.value_stack.clear();
        self.call_frames.clear();
    }
}

impl Default for VMState {
    fn default() -> Self {
        Self::new()
    }
}

/// 协程执行上下文
///
/// 包含协程恢复执行所需的所有信息
#[derive(Debug, Clone)]
pub struct Context {
    /// VM 状态
    pub vm_state: VMState,
    /// 协程是否已启动
    pub started: bool,
    /// 协程是否已完成
    pub finished: bool,
    /// 等待的资源（Channel ID 等）
    pub waiting_on: Option<WaitReason>,
}

/// 等待原因
#[derive(Debug, Clone)]
pub enum WaitReason {
    /// 等待 Channel 发送
    ChannelSend(u64),
    /// 等待 Channel 接收
    ChannelReceive(u64),
    /// 等待 Mutex
    Mutex(u64),
    /// 等待 WaitGroup
    WaitGroup(u64),
    /// 等待定时器
    Timer(u64),
    /// 等待 IO
    IO,
    /// 主动让出
    Yield,
}

impl Context {
    /// 创建新的执行上下文
    pub fn new() -> Self {
        Self {
            vm_state: VMState::new(),
            started: false,
            finished: false,
            waiting_on: None,
        }
    }

    /// 创建带初始 IP 的上下文
    pub fn with_ip(ip: usize) -> Self {
        let mut ctx = Self::new();
        ctx.vm_state.ip = ip;
        ctx
    }

    /// 标记开始执行
    pub fn mark_started(&mut self) {
        self.started = true;
    }

    /// 标记执行完成
    pub fn mark_finished(&mut self) {
        self.finished = true;
    }

    /// 检查是否正在等待
    pub fn is_waiting(&self) -> bool {
        self.waiting_on.is_some()
    }

    /// 设置等待原因
    pub fn wait_for(&mut self, reason: WaitReason) {
        self.waiting_on = Some(reason);
    }

    /// 清除等待状态
    pub fn clear_wait(&mut self) {
        self.waiting_on = None;
    }

    /// 重置上下文
    pub fn reset(&mut self) {
        self.vm_state.reset();
        self.started = false;
        self.finished = false;
        self.waiting_on = None;
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}
