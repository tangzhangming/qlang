//! 全局调度器
//!
//! 实现 GMP 调度模型的核心调度逻辑

use std::sync::atomic::{AtomicU64, AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;
use parking_lot::{Mutex, RwLock};

use super::goroutine::{Goroutine, GoroutineStatus};
use super::processor::Processor;
use super::machine::Machine;
use super::queue::GlobalQueue;
use super::GoId;
use crate::compiler::Chunk;
use crate::vm::value::Function;
use crate::vm::Value;

/// 全局调度器单例
pub static SCHEDULER: OnceLock<Scheduler> = OnceLock::new();

/// 获取全局调度器
pub fn get_scheduler() -> &'static Scheduler {
    SCHEDULER.get_or_init(|| Scheduler::new())
}

/// 调度器配置
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// 处理器数量（默认为 CPU 核心数）
    pub num_processors: usize,
    /// 最大工作线程数
    pub max_machines: usize,
    /// 全局队列批量获取大小
    pub global_batch_size: usize,
    /// 抢占时间片（微秒）
    pub preempt_time_slice_us: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        let num_cpus = num_cpus::get();
        Self {
            num_processors: num_cpus,
            max_machines: num_cpus * 4,
            global_batch_size: 32,
            preempt_time_slice_us: 10_000,  // 10ms
        }
    }
}

/// 全局调度器
pub struct Scheduler {
    /// 配置
    config: SchedulerConfig,
    /// 所有处理器
    processors: Vec<Arc<Processor>>,
    /// 所有工作线程
    machines: RwLock<Vec<Arc<Machine>>>,
    /// 全局运行队列
    global_queue: GlobalQueue,
    /// 空闲处理器列表
    idle_processors: Mutex<Vec<usize>>,
    /// 空闲处理器数量
    idle_count: AtomicUsize,
    /// 协程 ID 计数器
    next_goid: AtomicU64,
    /// 总协程数量
    goroutine_count: AtomicU64,
    /// 是否正在运行
    running: AtomicBool,
    /// 启动时间
    start_time: Instant,
    /// 当前执行的字节码
    chunk: RwLock<Option<Arc<Chunk>>>,
    /// 协程执行回调
    executor: RwLock<Option<Box<dyn Fn(&Goroutine) + Send + Sync>>>,
}

impl Scheduler {
    /// 创建新的调度器
    pub fn new() -> Self {
        Self::with_config(SchedulerConfig::default())
    }

    /// 使用指定配置创建调度器
    pub fn with_config(config: SchedulerConfig) -> Self {
        let num_p = config.num_processors;
        
        // 创建处理器
        let processors: Vec<_> = (0..num_p)
            .map(|id| Arc::new(Processor::new(id)))
            .collect();
        
        // 初始时所有处理器都是空闲的
        let idle_processors: Vec<_> = (0..num_p).collect();
        
        Self {
            config,
            processors,
            machines: RwLock::new(Vec::new()),
            global_queue: GlobalQueue::new(),
            idle_processors: Mutex::new(idle_processors),
            idle_count: AtomicUsize::new(num_p),
            next_goid: AtomicU64::new(1),  // 0 保留给主协程
            goroutine_count: AtomicU64::new(0),
            running: AtomicBool::new(false),
            start_time: Instant::now(),
            chunk: RwLock::new(None),
            executor: RwLock::new(None),
        }
    }

    /// 设置字节码
    pub fn set_chunk(&self, chunk: Arc<Chunk>) {
        *self.chunk.write() = Some(chunk);
    }

    /// 获取字节码
    pub fn chunk(&self) -> Option<Arc<Chunk>> {
        self.chunk.read().clone()
    }

    /// 设置协程执行器
    pub fn set_executor<F>(&self, executor: F)
    where
        F: Fn(&Goroutine) + Send + Sync + 'static,
    {
        *self.executor.write() = Some(Box::new(executor));
    }

    /// 启动调度器
    pub fn start(&self) {
        if self.running.swap(true, Ordering::AcqRel) {
            return;  // 已经在运行
        }

        // 创建工作线程
        let num_machines = self.config.num_processors;
        let mut machines = self.machines.write();
        
        for i in 0..num_machines {
            let m = Machine::new(i as u64);
            m.set_scheduler(self as *const Scheduler as *mut Scheduler);
            
            // 绑定处理器
            if let Some(p) = self.processors.get(i) {
                m.bind_processor(p);
                self.mark_processor_busy(i);
            }
            
            m.start();
            machines.push(m);
        }
    }

    /// 停止调度器
    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
        
        // 停止所有工作线程
        let machines = self.machines.read();
        for m in machines.iter() {
            m.stop();
        }
        
        // 等待所有线程结束
        for m in machines.iter() {
            m.join();
        }
    }

    /// 检查是否正在运行
    #[inline]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// 生成新的协程 ID
    #[inline]
    pub fn next_goid(&self) -> GoId {
        self.next_goid.fetch_add(1, Ordering::Relaxed)
    }

    /// 获取总协程数量
    #[inline]
    pub fn goroutine_count(&self) -> u64 {
        self.goroutine_count.load(Ordering::Relaxed)
    }

    /// 创建并调度新协程
    pub fn spawn(&self, func: Arc<Function>, args: Vec<Value>) -> Option<GoId> {
        let goid = self.next_goid();
        
        let g = match Goroutine::new(goid, func, args) {
            Ok(g) => Arc::new(g),
            Err(_) => return None,
        };
        
        self.goroutine_count.fetch_add(1, Ordering::Relaxed);
        self.schedule(g);
        
        Some(goid)
    }

    /// 调度协程
    pub fn schedule(&self, g: Arc<Goroutine>) {
        // 尝试放入空闲处理器的本地队列
        if let Some(p_idx) = self.get_idle_processor() {
            if let Some(p) = self.processors.get(p_idx) {
                if p.push(Arc::clone(&g)) {
                    self.wake_machine();
                    return;
                }
            }
        }
        
        // 尝试放入随机处理器的本地队列
        let p_idx = (g.id as usize) % self.processors.len();
        if let Some(p) = self.processors.get(p_idx) {
            if p.push(Arc::clone(&g)) {
                self.wake_machine();
                return;
            }
        }
        
        // 本地队列满，放入全局队列
        self.global_queue.push(g);
        self.wake_machine();
    }

    /// 从全局队列获取协程
    pub fn get_from_global(&self) -> Option<Arc<Goroutine>> {
        self.global_queue.pop()
    }

    /// 批量从全局队列获取
    pub fn get_batch_from_global(&self) -> Vec<Arc<Goroutine>> {
        self.global_queue.pop_batch(self.config.global_batch_size)
    }

    /// 工作窃取
    pub fn steal_work(&self, thief: &Processor) -> Option<Arc<Goroutine>> {
        let num_p = self.processors.len();
        let start = thief.id;
        
        // 先尝试从全局队列获取
        if let Some(g) = self.global_queue.pop() {
            return Some(g);
        }
        
        // 随机选择一个起点，避免总是从同一个 P 窃取
        // 使用当前调度计数作为伪随机源
        let random_offset = (thief.schedule_count() as usize) % num_p;
        
        for i in 0..num_p {
            let offset = (random_offset + i + 1) % num_p;
            let idx = (start + offset) % num_p;
            
            // 不从自己窃取
            if idx == thief.id {
                continue;
            }
            
            if let Some(victim) = self.processors.get(idx) {
                // 只有当 victim 队列有足够多的任务时才窃取
                if victim.queue_len() > 1 {
                    if let Some(g) = victim.local_queue.steal() {
                        return Some(g);
                    }
                }
            }
        }
        
        // 最后再尝试从任何有任务的 P 窃取
        for offset in 1..num_p {
            let idx = (start + offset) % num_p;
            if let Some(victim) = self.processors.get(idx) {
                if let Some(g) = victim.local_queue.steal() {
                    return Some(g);
                }
            }
        }
        
        None
    }
    
    /// 批量工作窃取（窃取一半）
    pub fn steal_work_batch(&self, thief: &Processor, batch_size: usize) -> Vec<Arc<Goroutine>> {
        let num_p = self.processors.len();
        let start = thief.id;
        let mut stolen = Vec::with_capacity(batch_size);
        
        for offset in 1..num_p {
            let idx = (start + offset) % num_p;
            if let Some(victim) = self.processors.get(idx) {
                let victim_len = victim.queue_len();
                if victim_len > 1 {
                    // 窃取一半，但不超过 batch_size
                    let steal_count = (victim_len / 2).min(batch_size - stolen.len());
                    for _ in 0..steal_count {
                        if let Some(g) = victim.local_queue.steal() {
                            stolen.push(g);
                        }
                    }
                }
                
                if stolen.len() >= batch_size {
                    break;
                }
            }
        }
        
        stolen
    }

    /// 获取空闲处理器
    fn get_idle_processor(&self) -> Option<usize> {
        self.idle_processors.lock().pop()
    }

    /// 标记处理器为忙碌
    fn mark_processor_busy(&self, idx: usize) {
        let mut idle = self.idle_processors.lock();
        idle.retain(|&i| i != idx);
        self.idle_count.store(idle.len(), Ordering::Release);
    }

    /// 标记处理器为空闲
    pub fn mark_processor_idle(&self, idx: usize) {
        let mut idle = self.idle_processors.lock();
        if !idle.contains(&idx) {
            idle.push(idx);
        }
        self.idle_count.store(idle.len(), Ordering::Release);
    }

    /// 唤醒工作线程
    fn wake_machine(&self) {
        let machines = self.machines.read();
        for m in machines.iter() {
            if m.is_parking() {
                m.unpark();
                break;
            }
        }
    }

    /// 执行协程（由 Machine 调用）
    pub fn execute_goroutine(&self, g: &Goroutine) {
        if let Some(executor) = self.executor.read().as_ref() {
            executor(g);
        }
    }

    /// 协程完成
    pub fn finish_goroutine(&self, _g: &Goroutine) {
        self.goroutine_count.fetch_sub(1, Ordering::Relaxed);
    }

    /// 协程让出
    pub fn yield_goroutine(&self, g: Arc<Goroutine>) {
        g.set_status(GoroutineStatus::Runnable);
        self.schedule(g);
    }

    /// 协程阻塞
    pub fn park_goroutine(&self, g: &Goroutine) {
        g.set_status(GoroutineStatus::Waiting);
    }

    /// 唤醒协程
    pub fn unpark_goroutine(&self, g: Arc<Goroutine>) {
        g.set_status(GoroutineStatus::Runnable);
        self.schedule(g);
    }

    /// 检查是否应该抢占
    pub fn should_preempt(&self) -> bool {
        // 简单实现：检查是否运行太久
        // 实际应该基于时间片或信号
        false
    }

    /// 获取运行时间
    pub fn elapsed(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    /// 获取处理器数量
    #[inline]
    pub fn num_processors(&self) -> usize {
        self.processors.len()
    }

    /// 获取处理器
    pub fn processor(&self, idx: usize) -> Option<&Arc<Processor>> {
        self.processors.get(idx)
    }

    /// 获取全局队列长度
    #[inline]
    pub fn global_queue_len(&self) -> usize {
        self.global_queue.len()
    }

    /// 获取调度统计信息
    pub fn stats(&self) -> SchedulerStats {
        let mut total_local = 0;
        let mut schedule_counts = Vec::new();
        
        for p in &self.processors {
            total_local += p.queue_len();
            schedule_counts.push(p.schedule_count());
        }
        
        SchedulerStats {
            goroutine_count: self.goroutine_count(),
            global_queue_len: self.global_queue_len(),
            total_local_queue_len: total_local,
            processor_schedule_counts: schedule_counts,
            idle_processors: self.idle_count.load(Ordering::Relaxed),
            elapsed: self.elapsed(),
        }
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// 调度统计信息
#[derive(Debug)]
pub struct SchedulerStats {
    pub goroutine_count: u64,
    pub global_queue_len: usize,
    pub total_local_queue_len: usize,
    pub processor_schedule_counts: Vec<u64>,
    pub idle_processors: usize,
    pub elapsed: std::time::Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_new() {
        let scheduler = Scheduler::new();
        assert!(!scheduler.is_running());
        assert_eq!(scheduler.goroutine_count(), 0);
    }

    #[test]
    fn test_scheduler_goid() {
        let scheduler = Scheduler::new();
        let id1 = scheduler.next_goid();
        let id2 = scheduler.next_goid();
        assert_eq!(id1 + 1, id2);
    }
}
