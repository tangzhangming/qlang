//! 无锁队列实现
//!
//! 用于协程调度的本地运行队列

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use crossbeam_utils::CachePadded;

use super::goroutine::Goroutine;

/// 本地队列容量（必须是 2 的幂）
const LOCAL_QUEUE_SIZE: usize = 256;

/// 本地运行队列
///
/// 使用无锁环形缓冲区实现，支持单生产者多消费者（SPMC）模式：
/// - 拥有者可以 push/pop（头部操作）
/// - 其他线程可以 steal（尾部操作）
pub struct LocalQueue {
    /// 头部索引（拥有者操作）
    head: CachePadded<AtomicU32>,
    /// 尾部索引（窃取者操作）
    tail: CachePadded<AtomicU32>,
    /// 环形缓冲区
    buffer: Box<[CachePadded<AtomicUsize>; LOCAL_QUEUE_SIZE]>,
}

impl LocalQueue {
    /// 创建新的本地队列
    pub fn new() -> Self {
        // 使用 Box 分配以避免栈溢出
        let buffer: Box<[CachePadded<AtomicUsize>; LOCAL_QUEUE_SIZE]> = {
            let mut vec = Vec::with_capacity(LOCAL_QUEUE_SIZE);
            for _ in 0..LOCAL_QUEUE_SIZE {
                vec.push(CachePadded::new(AtomicUsize::new(0)));
            }
            vec.into_boxed_slice().try_into().unwrap()
        };

        Self {
            head: CachePadded::new(AtomicU32::new(0)),
            tail: CachePadded::new(AtomicU32::new(0)),
            buffer,
        }
    }

    /// 获取队列长度
    #[inline]
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Relaxed);
        head.wrapping_sub(tail) as usize
    }

    /// 检查队列是否为空
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 检查队列是否已满
    #[inline]
    pub fn is_full(&self) -> bool {
        self.len() >= LOCAL_QUEUE_SIZE
    }

    /// 推入协程（仅拥有者调用）
    ///
    /// 返回 true 表示成功，false 表示队列已满
    pub fn push(&self, g: Arc<Goroutine>) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);

        // 检查是否已满
        if head.wrapping_sub(tail) as usize >= LOCAL_QUEUE_SIZE {
            return false;
        }

        let idx = (head as usize) & (LOCAL_QUEUE_SIZE - 1);
        
        // 将 Arc 转换为原始指针存储
        let ptr = Arc::into_raw(g) as usize;
        self.buffer[idx].store(ptr, Ordering::Relaxed);

        // 更新头部（release 语义确保 buffer 写入对其他线程可见）
        self.head.store(head.wrapping_add(1), Ordering::Release);

        true
    }

    /// 弹出协程（仅拥有者调用）
    pub fn pop(&self) -> Option<Arc<Goroutine>> {
        let mut head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Relaxed);

        // 检查是否为空
        if head == tail {
            return None;
        }

        head = head.wrapping_sub(1);
        self.head.store(head, Ordering::Relaxed);

        let idx = (head as usize) & (LOCAL_QUEUE_SIZE - 1);
        
        // 加载元素
        let ptr = self.buffer[idx].load(Ordering::Relaxed);
        
        // 检查是否被窃取
        let new_tail = self.tail.load(Ordering::Acquire);
        if head < new_tail {
            // 被窃取了，恢复头部
            self.head.store(head.wrapping_add(1), Ordering::Relaxed);
            return None;
        }

        // 从原始指针恢复 Arc
        if ptr != 0 {
            Some(unsafe { Arc::from_raw(ptr as *const Goroutine) })
        } else {
            None
        }
    }

    /// 窃取协程（其他线程调用）
    ///
    /// 从尾部窃取一个协程
    pub fn steal(&self) -> Option<Arc<Goroutine>> {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);

        // 检查是否为空
        if tail >= head {
            return None;
        }

        let idx = (tail as usize) & (LOCAL_QUEUE_SIZE - 1);
        let ptr = self.buffer[idx].load(Ordering::Relaxed);

        // CAS 更新尾部
        if self.tail.compare_exchange(
            tail,
            tail.wrapping_add(1),
            Ordering::AcqRel,
            Ordering::Relaxed,
        ).is_err() {
            // 竞争失败
            return None;
        }

        // 从原始指针恢复 Arc
        if ptr != 0 {
            Some(unsafe { Arc::from_raw(ptr as *const Goroutine) })
        } else {
            None
        }
    }

    /// 批量窃取（窃取一半）
    pub fn steal_batch(&self, dst: &LocalQueue) -> usize {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);

        let n = head.wrapping_sub(tail) as usize;
        if n == 0 {
            return 0;
        }

        // 窃取一半
        let steal_count = (n + 1) / 2;
        let steal_count = steal_count.min(LOCAL_QUEUE_SIZE - dst.len());

        let mut stolen = 0;
        for _ in 0..steal_count {
            if let Some(g) = self.steal() {
                if dst.push(g) {
                    stolen += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        stolen
    }
    
    /// 批量窃取到向量（不需要目标队列）
    /// 
    /// 返回窃取的协程列表
    pub fn steal_batch_to_vec(&self, max: usize) -> Vec<Arc<Goroutine>> {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);

        let n = head.wrapping_sub(tail) as usize;
        if n == 0 {
            return Vec::new();
        }

        // 窃取一半，但不超过 max
        let steal_count = ((n + 1) / 2).min(max);
        let mut stolen = Vec::with_capacity(steal_count);

        for _ in 0..steal_count {
            if let Some(g) = self.steal() {
                stolen.push(g);
            } else {
                break;
            }
        }

        stolen
    }
}

// ============================================================================
// 自适应窃取策略
// ============================================================================

/// 窃取统计
pub struct StealStats {
    /// 总窃取尝试次数
    attempts: AtomicUsize,
    /// 成功窃取次数
    successes: AtomicUsize,
    /// 窃取的任务总数
    stolen_tasks: AtomicUsize,
    /// 最近成功率（滑动窗口）
    recent_success_rate: std::sync::atomic::AtomicU32,
}

impl StealStats {
    /// 创建新的统计
    pub const fn new() -> Self {
        Self {
            attempts: AtomicUsize::new(0),
            successes: AtomicUsize::new(0),
            stolen_tasks: AtomicUsize::new(0),
            recent_success_rate: std::sync::atomic::AtomicU32::new(50), // 50%
        }
    }
    
    /// 记录窃取尝试
    pub fn record_attempt(&self, success: bool, tasks_stolen: usize) {
        self.attempts.fetch_add(1, Ordering::Relaxed);
        if success {
            self.successes.fetch_add(1, Ordering::Relaxed);
            self.stolen_tasks.fetch_add(tasks_stolen, Ordering::Relaxed);
        }
        
        // 更新滑动窗口成功率（使用指数移动平均）
        let old_rate = self.recent_success_rate.load(Ordering::Relaxed);
        let new_sample = if success { 100 } else { 0 };
        // 新值 = 0.8 * 旧值 + 0.2 * 新采样
        let new_rate = (old_rate * 4 + new_sample) / 5;
        self.recent_success_rate.store(new_rate, Ordering::Relaxed);
    }
    
    /// 获取成功率
    pub fn success_rate(&self) -> f64 {
        let attempts = self.attempts.load(Ordering::Relaxed);
        let successes = self.successes.load(Ordering::Relaxed);
        if attempts == 0 {
            0.5 // 默认 50%
        } else {
            successes as f64 / attempts as f64
        }
    }
    
    /// 获取最近成功率（0-100）
    pub fn recent_success_rate(&self) -> u32 {
        self.recent_success_rate.load(Ordering::Relaxed)
    }
    
    /// 获取平均每次成功窃取的任务数
    pub fn avg_stolen_per_success(&self) -> f64 {
        let successes = self.successes.load(Ordering::Relaxed);
        let stolen = self.stolen_tasks.load(Ordering::Relaxed);
        if successes == 0 {
            1.0
        } else {
            stolen as f64 / successes as f64
        }
    }
}

impl Default for StealStats {
    fn default() -> Self {
        Self::new()
    }
}

/// 自适应窃取策略
/// 
/// 根据历史窃取成功率调整窃取行为
pub struct AdaptiveStealStrategy {
    /// 窃取统计
    stats: StealStats,
    /// 窃取间隔（纳秒）
    steal_interval_ns: std::sync::atomic::AtomicU64,
    /// 最小窃取间隔
    min_interval_ns: u64,
    /// 最大窃取间隔
    max_interval_ns: u64,
}

impl AdaptiveStealStrategy {
    /// 创建新的自适应策略
    pub fn new() -> Self {
        Self {
            stats: StealStats::new(),
            steal_interval_ns: std::sync::atomic::AtomicU64::new(1_000), // 1μs
            min_interval_ns: 100,      // 0.1μs
            max_interval_ns: 100_000,  // 100μs
        }
    }
    
    /// 记录窃取结果
    pub fn record(&self, success: bool, tasks_stolen: usize) {
        self.stats.record_attempt(success, tasks_stolen);
        
        // 自适应调整间隔
        let current = self.steal_interval_ns.load(Ordering::Relaxed);
        let new_interval = if success {
            // 成功：减少间隔（更积极窃取）
            (current * 9 / 10).max(self.min_interval_ns)
        } else {
            // 失败：增加间隔（减少窃取频率）
            (current * 11 / 10).min(self.max_interval_ns)
        };
        self.steal_interval_ns.store(new_interval, Ordering::Relaxed);
    }
    
    /// 获取建议的窃取间隔
    pub fn suggested_interval(&self) -> std::time::Duration {
        let ns = self.steal_interval_ns.load(Ordering::Relaxed);
        std::time::Duration::from_nanos(ns)
    }
    
    /// 获取统计信息
    pub fn stats(&self) -> &StealStats {
        &self.stats
    }
    
    /// 是否应该尝试窃取
    /// 
    /// 基于最近成功率决定是否值得尝试
    pub fn should_steal(&self) -> bool {
        // 如果成功率太低（< 10%），减少尝试
        let rate = self.stats.recent_success_rate();
        if rate < 10 {
            // 10% 的概率尝试
            (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u32) % 10 == 0
        } else {
            true
        }
    }
}

impl Default for AdaptiveStealStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for LocalQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LocalQueue {
    fn drop(&mut self) {
        // 清理所有残留的协程
        while let Some(_) = self.pop() {
            // Arc 会自动释放
        }
    }
}

unsafe impl Send for LocalQueue {}
unsafe impl Sync for LocalQueue {}

/// 全局队列（使用互斥锁）
pub struct GlobalQueue {
    queue: parking_lot::Mutex<std::collections::VecDeque<Arc<Goroutine>>>,
    len: AtomicUsize,
}

impl GlobalQueue {
    /// 创建新的全局队列
    pub fn new() -> Self {
        Self {
            queue: parking_lot::Mutex::new(std::collections::VecDeque::new()),
            len: AtomicUsize::new(0),
        }
    }

    /// 获取队列长度
    #[inline]
    pub fn len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }

    /// 检查是否为空
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 推入协程
    pub fn push(&self, g: Arc<Goroutine>) {
        let mut queue = self.queue.lock();
        queue.push_back(g);
        self.len.fetch_add(1, Ordering::Relaxed);
    }

    /// 批量推入
    pub fn push_batch(&self, batch: Vec<Arc<Goroutine>>) {
        let count = batch.len();
        let mut queue = self.queue.lock();
        for g in batch {
            queue.push_back(g);
        }
        self.len.fetch_add(count, Ordering::Relaxed);
    }

    /// 弹出协程
    pub fn pop(&self) -> Option<Arc<Goroutine>> {
        let mut queue = self.queue.lock();
        if let Some(g) = queue.pop_front() {
            self.len.fetch_sub(1, Ordering::Relaxed);
            Some(g)
        } else {
            None
        }
    }

    /// 批量弹出
    pub fn pop_batch(&self, max: usize) -> Vec<Arc<Goroutine>> {
        let mut queue = self.queue.lock();
        let count = queue.len().min(max);
        let mut batch = Vec::with_capacity(count);
        for _ in 0..count {
            if let Some(g) = queue.pop_front() {
                batch.push(g);
            }
        }
        self.len.fetch_sub(batch.len(), Ordering::Relaxed);
        batch
    }
}

impl Default for GlobalQueue {
    fn default() -> Self {
        Self::new()
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
    fn test_local_queue_push_pop() {
        let queue = LocalQueue::new();
        
        let g1 = make_test_goroutine(1);
        let g2 = make_test_goroutine(2);
        
        assert!(queue.push(g1));
        assert!(queue.push(g2));
        assert_eq!(queue.len(), 2);
        
        let popped = queue.pop().unwrap();
        assert_eq!(popped.id, 2);  // LIFO
        
        let popped = queue.pop().unwrap();
        assert_eq!(popped.id, 1);
        
        assert!(queue.is_empty());
    }

    #[test]
    fn test_local_queue_steal() {
        let queue = LocalQueue::new();
        
        for i in 0..10 {
            queue.push(make_test_goroutine(i));
        }
        
        let stolen = queue.steal().unwrap();
        assert_eq!(stolen.id, 0);  // FIFO from tail
        
        assert_eq!(queue.len(), 9);
    }

    #[test]
    fn test_global_queue() {
        let queue = GlobalQueue::new();
        
        queue.push(make_test_goroutine(1));
        queue.push(make_test_goroutine(2));
        
        assert_eq!(queue.len(), 2);
        
        let g = queue.pop().unwrap();
        assert_eq!(g.id, 1);  // FIFO
    }
}
