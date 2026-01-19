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
