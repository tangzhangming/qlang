//! 协程 (Goroutine) 结构
//!
//! G - 协程，是调度的基本单位

use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;

use super::stack::Stack;
use super::context::Context;
use super::GoId;
use crate::vm::value::Function;
use crate::vm::Value;

/// 协程状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GoroutineStatus {
    /// 可运行，等待被调度
    Runnable = 0,
    /// 正在运行
    Running = 1,
    /// 等待中（阻塞在 Channel、Mutex 等）
    Waiting = 2,
    /// 已死亡（执行完成或发生错误）
    Dead = 3,
    /// 空闲（在对象池中）
    Idle = 4,
}

impl From<u8> for GoroutineStatus {
    fn from(v: u8) -> Self {
        match v {
            0 => GoroutineStatus::Runnable,
            1 => GoroutineStatus::Running,
            2 => GoroutineStatus::Waiting,
            3 => GoroutineStatus::Dead,
            4 => GoroutineStatus::Idle,
            _ => GoroutineStatus::Dead,
        }
    }
}

/// 协程
///
/// 包含协程的所有状态信息
pub struct Goroutine {
    /// 协程唯一 ID
    pub id: GoId,
    /// 协程状态（原子操作）
    status: AtomicU8,
    /// 协程栈
    pub stack: Mutex<Stack>,
    /// 执行上下文
    pub context: Mutex<Context>,
    /// 要执行的函数
    pub func: Option<Arc<Function>>,
    /// 函数参数
    pub args: Vec<Value>,
    /// 父协程 ID（用于调试）
    pub parent_id: Option<GoId>,
    /// 抢占标记
    preempt: AtomicU8,
    /// 调度计数（用于公平调度）
    schedule_count: AtomicU64,
}

impl Goroutine {
    /// 创建新的协程
    pub fn new(id: GoId, func: Arc<Function>, args: Vec<Value>) -> Result<Self, super::stack::StackOverflow> {
        Ok(Self {
            id,
            status: AtomicU8::new(GoroutineStatus::Runnable as u8),
            stack: Mutex::new(Stack::new()?),
            context: Mutex::new(Context::with_ip(func.chunk_index)),
            func: Some(func),
            args,
            parent_id: None,
            preempt: AtomicU8::new(0),
            schedule_count: AtomicU64::new(0),
        })
    }

    /// 创建主协程（用于主函数）
    pub fn new_main(id: GoId) -> Result<Self, super::stack::StackOverflow> {
        Ok(Self {
            id,
            status: AtomicU8::new(GoroutineStatus::Running as u8),
            stack: Mutex::new(Stack::new()?),
            context: Mutex::new(Context::new()),
            func: None,
            args: Vec::new(),
            parent_id: None,
            preempt: AtomicU8::new(0),
            schedule_count: AtomicU64::new(0),
        })
    }

    /// 创建空闲协程（用于对象池）
    pub fn new_idle(id: GoId) -> Result<Self, super::stack::StackOverflow> {
        Ok(Self {
            id,
            status: AtomicU8::new(GoroutineStatus::Idle as u8),
            stack: Mutex::new(Stack::new()?),
            context: Mutex::new(Context::new()),
            func: None,
            args: Vec::new(),
            parent_id: None,
            preempt: AtomicU8::new(0),
            schedule_count: AtomicU64::new(0),
        })
    }

    /// 获取协程状态
    #[inline]
    pub fn status(&self) -> GoroutineStatus {
        GoroutineStatus::from(self.status.load(Ordering::Acquire))
    }

    /// 设置协程状态
    #[inline]
    pub fn set_status(&self, status: GoroutineStatus) {
        self.status.store(status as u8, Ordering::Release);
    }

    /// 尝试将状态从 expected 改为 new
    #[inline]
    pub fn cas_status(&self, expected: GoroutineStatus, new: GoroutineStatus) -> bool {
        self.status.compare_exchange(
            expected as u8,
            new as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        ).is_ok()
    }

    /// 检查是否可运行
    #[inline]
    pub fn is_runnable(&self) -> bool {
        self.status() == GoroutineStatus::Runnable
    }

    /// 检查是否正在运行
    #[inline]
    pub fn is_running(&self) -> bool {
        self.status() == GoroutineStatus::Running
    }

    /// 检查是否等待中
    #[inline]
    pub fn is_waiting(&self) -> bool {
        self.status() == GoroutineStatus::Waiting
    }

    /// 检查是否已死亡
    #[inline]
    pub fn is_dead(&self) -> bool {
        self.status() == GoroutineStatus::Dead
    }

    /// 检查是否需要抢占
    #[inline]
    pub fn should_preempt(&self) -> bool {
        self.preempt.load(Ordering::Relaxed) != 0
    }

    /// 请求抢占
    #[inline]
    pub fn request_preempt(&self) {
        self.preempt.store(1, Ordering::Relaxed);
    }

    /// 清除抢占标记
    #[inline]
    pub fn clear_preempt(&self) {
        self.preempt.store(0, Ordering::Relaxed);
    }

    /// 增加调度计数
    #[inline]
    pub fn inc_schedule_count(&self) {
        self.schedule_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 获取调度计数
    #[inline]
    pub fn schedule_count(&self) -> u64 {
        self.schedule_count.load(Ordering::Relaxed)
    }

    /// 重置协程以便复用
    pub fn reset(&self, id: GoId, func: Arc<Function>, args: Vec<Value>) {
        // 重置状态
        self.status.store(GoroutineStatus::Runnable as u8, Ordering::Release);
        
        // 重置栈
        self.stack.lock().reset();
        
        // 重置上下文
        let mut ctx = self.context.lock();
        ctx.reset();
        ctx.vm_state.ip = func.chunk_index;
        
        // 清除抢占标记
        self.preempt.store(0, Ordering::Relaxed);
        self.schedule_count.store(0, Ordering::Relaxed);
        
        // 注意：id, func, args 需要通过其他方式更新
        // 因为它们不是内部可变的
    }

    /// 标记为死亡
    pub fn mark_dead(&self) {
        self.set_status(GoroutineStatus::Dead);
        let mut ctx = self.context.lock();
        ctx.mark_finished();
    }

    /// 暂停协程（进入等待状态）
    pub fn park(&self) {
        self.set_status(GoroutineStatus::Waiting);
    }

    /// 唤醒协程（进入可运行状态）
    pub fn unpark(&self) {
        if self.status() == GoroutineStatus::Waiting {
            self.set_status(GoroutineStatus::Runnable);
        }
    }
}

impl std::fmt::Debug for Goroutine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Goroutine")
            .field("id", &self.id)
            .field("status", &self.status())
            .field("parent_id", &self.parent_id)
            .finish()
    }
}

// Goroutine 可以安全地在线程间传递
unsafe impl Send for Goroutine {}
unsafe impl Sync for Goroutine {}

/// 协程句柄（用于外部引用）
#[derive(Clone)]
pub struct GoroutineHandle {
    inner: Arc<Goroutine>,
}

impl GoroutineHandle {
    /// 创建新的句柄
    pub fn new(g: Arc<Goroutine>) -> Self {
        Self { inner: g }
    }

    /// 获取协程 ID
    pub fn id(&self) -> GoId {
        self.inner.id
    }

    /// 获取协程状态
    pub fn status(&self) -> GoroutineStatus {
        self.inner.status()
    }

    /// 检查是否完成
    pub fn is_done(&self) -> bool {
        self.inner.is_dead()
    }

    /// 获取内部引用
    pub fn inner(&self) -> &Arc<Goroutine> {
        &self.inner
    }
}

impl std::fmt::Debug for GoroutineHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoroutineHandle")
            .field("id", &self.id())
            .field("status", &self.status())
            .finish()
    }
}
