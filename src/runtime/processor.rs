//! 逻辑处理器 (Processor)
//!
//! P - 逻辑处理器，管理协程的本地运行队列

use std::sync::atomic::{AtomicU8, AtomicU64, AtomicPtr, Ordering};
use std::sync::Arc;
use std::ptr;
use parking_lot::Mutex;

use super::goroutine::Goroutine;
use super::queue::LocalQueue;

/// 处理器状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ProcessorStatus {
    /// 空闲
    Idle = 0,
    /// 运行中
    Running = 1,
    /// 执行系统调用
    Syscall = 2,
    /// 停止
    Stopped = 3,
}

impl From<u8> for ProcessorStatus {
    fn from(v: u8) -> Self {
        match v {
            0 => ProcessorStatus::Idle,
            1 => ProcessorStatus::Running,
            2 => ProcessorStatus::Syscall,
            3 => ProcessorStatus::Stopped,
            _ => ProcessorStatus::Stopped,
        }
    }
}

/// 逻辑处理器
///
/// 每个 P 管理一个本地运行队列，并绑定到一个 M（工作线程）执行
pub struct Processor {
    /// 处理器 ID
    pub id: usize,
    /// 处理器状态
    status: AtomicU8,
    /// 本地运行队列
    pub local_queue: LocalQueue,
    /// 当前正在运行的协程
    current_g: AtomicPtr<Goroutine>,
    /// 下一个要运行的协程（快速路径）
    next_g: Mutex<Option<Arc<Goroutine>>>,
    /// 关联的 Machine ID
    machine_id: AtomicU64,
    /// 调度计数
    schedule_count: AtomicU64,
    /// 最后一次调度时间（纳秒）
    last_schedule_time: AtomicU64,
}

impl Processor {
    /// 创建新的处理器
    pub fn new(id: usize) -> Self {
        Self {
            id,
            status: AtomicU8::new(ProcessorStatus::Idle as u8),
            local_queue: LocalQueue::new(),
            current_g: AtomicPtr::new(ptr::null_mut()),
            next_g: Mutex::new(None),
            machine_id: AtomicU64::new(u64::MAX),
            schedule_count: AtomicU64::new(0),
            last_schedule_time: AtomicU64::new(0),
        }
    }

    /// 获取处理器状态
    #[inline]
    pub fn status(&self) -> ProcessorStatus {
        ProcessorStatus::from(self.status.load(Ordering::Acquire))
    }

    /// 设置处理器状态
    #[inline]
    pub fn set_status(&self, status: ProcessorStatus) {
        self.status.store(status as u8, Ordering::Release);
    }

    /// 检查是否空闲
    #[inline]
    pub fn is_idle(&self) -> bool {
        self.status() == ProcessorStatus::Idle
    }

    /// 检查是否运行中
    #[inline]
    pub fn is_running(&self) -> bool {
        self.status() == ProcessorStatus::Running
    }

    /// 获取当前协程
    #[inline]
    pub fn current(&self) -> Option<Arc<Goroutine>> {
        let ptr = self.current_g.load(Ordering::Acquire);
        if ptr.is_null() {
            None
        } else {
            // 增加引用计数
            unsafe {
                Arc::increment_strong_count(ptr);
                Some(Arc::from_raw(ptr))
            }
        }
    }

    /// 设置当前协程
    pub fn set_current(&self, g: Option<Arc<Goroutine>>) {
        let old_ptr = self.current_g.load(Ordering::Relaxed);
        
        let new_ptr = match g {
            Some(g) => Arc::into_raw(g) as *mut Goroutine,
            None => ptr::null_mut(),
        };
        
        self.current_g.store(new_ptr, Ordering::Release);
        
        // 释放旧的协程引用
        if !old_ptr.is_null() {
            unsafe {
                drop(Arc::from_raw(old_ptr));
            }
        }
    }

    /// 设置下一个要运行的协程
    pub fn set_next(&self, g: Arc<Goroutine>) {
        *self.next_g.lock() = Some(g);
    }

    /// 获取下一个要运行的协程
    pub fn take_next(&self) -> Option<Arc<Goroutine>> {
        self.next_g.lock().take()
    }

    /// 将协程加入本地队列
    pub fn push(&self, g: Arc<Goroutine>) -> bool {
        self.local_queue.push(g)
    }

    /// 从本地队列获取协程
    pub fn pop(&self) -> Option<Arc<Goroutine>> {
        self.local_queue.pop()
    }

    /// 获取本地队列长度
    #[inline]
    pub fn queue_len(&self) -> usize {
        self.local_queue.len()
    }

    /// 绑定到 Machine
    pub fn bind_machine(&self, machine_id: u64) {
        self.machine_id.store(machine_id, Ordering::Release);
        self.set_status(ProcessorStatus::Running);
    }

    /// 解绑 Machine
    pub fn unbind_machine(&self) {
        self.machine_id.store(u64::MAX, Ordering::Release);
        self.set_current(None);
        self.set_status(ProcessorStatus::Idle);
    }

    /// 获取关联的 Machine ID
    #[inline]
    pub fn machine_id(&self) -> Option<u64> {
        let id = self.machine_id.load(Ordering::Acquire);
        if id == u64::MAX {
            None
        } else {
            Some(id)
        }
    }

    /// 增加调度计数
    pub fn inc_schedule_count(&self) {
        self.schedule_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 获取调度计数
    #[inline]
    pub fn schedule_count(&self) -> u64 {
        self.schedule_count.load(Ordering::Relaxed)
    }

    /// 更新最后调度时间
    pub fn update_schedule_time(&self, time_ns: u64) {
        self.last_schedule_time.store(time_ns, Ordering::Relaxed);
    }

    /// 获取最后调度时间
    #[inline]
    pub fn last_schedule_time(&self) -> u64 {
        self.last_schedule_time.load(Ordering::Relaxed)
    }

    /// 进入系统调用状态
    pub fn enter_syscall(&self) {
        self.set_status(ProcessorStatus::Syscall);
    }

    /// 退出系统调用状态
    pub fn exit_syscall(&self) {
        self.set_status(ProcessorStatus::Running);
    }

    /// 停止处理器
    pub fn stop(&self) {
        self.set_status(ProcessorStatus::Stopped);
        self.set_current(None);
    }
}

impl std::fmt::Debug for Processor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Processor")
            .field("id", &self.id)
            .field("status", &self.status())
            .field("queue_len", &self.queue_len())
            .field("machine_id", &self.machine_id())
            .finish()
    }
}

unsafe impl Send for Processor {}
unsafe impl Sync for Processor {}

impl Drop for Processor {
    fn drop(&mut self) {
        // 清理当前协程
        let ptr = self.current_g.load(Ordering::Relaxed);
        if !ptr.is_null() {
            unsafe {
                drop(Arc::from_raw(ptr));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::value::Function;

    fn make_test_goroutine(id: u64) -> Arc<Goroutine> {
        let func = Arc::new(Function {
            name: Some(format!("test_{}", id)),
            arity: 0,
            required_params: 0,
            defaults: Vec::new(),
            has_variadic: false,
            chunk_index: 0,
            local_count: 0,
            upvalues: Vec::new(),
        });
        Arc::new(Goroutine::new(id, func, Vec::new()).unwrap())
    }

    #[test]
    fn test_processor_new() {
        let p = Processor::new(0);
        assert_eq!(p.id, 0);
        assert_eq!(p.status(), ProcessorStatus::Idle);
        assert!(p.current().is_none());
    }

    #[test]
    fn test_processor_queue() {
        let p = Processor::new(0);
        
        let g = make_test_goroutine(1);
        assert!(p.push(g));
        assert_eq!(p.queue_len(), 1);
        
        let popped = p.pop().unwrap();
        assert_eq!(popped.id, 1);
    }

    #[test]
    fn test_processor_current() {
        let p = Processor::new(0);
        
        let g = make_test_goroutine(1);
        p.set_current(Some(g));
        
        let current = p.current().unwrap();
        assert_eq!(current.id, 1);
        
        p.set_current(None);
        assert!(p.current().is_none());
    }
}
