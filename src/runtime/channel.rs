//! Channel 实现
//!
//! 基于调度器实现的 Go 风格 Channel

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::collections::VecDeque;
use parking_lot::{Mutex, Condvar};

use super::goroutine::Goroutine;
use crate::vm::Value;

/// Channel ID 计数器
static CHANNEL_ID: AtomicU64 = AtomicU64::new(1);

/// Channel 状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelStatus {
    /// 打开
    Open,
    /// 已关闭
    Closed,
}

/// 等待者
struct Waiter {
    /// 等待的协程
    goroutine: Arc<Goroutine>,
    /// 发送的值（仅用于发送等待）
    value: Option<Value>,
}

/// Channel
///
/// 支持带缓冲和无缓冲两种模式
pub struct Channel {
    /// Channel ID
    id: u64,
    /// 缓冲区容量（0 表示无缓冲）
    capacity: usize,
    /// 缓冲区
    buffer: Mutex<VecDeque<Value>>,
    /// 发送等待队列
    send_waiters: Mutex<VecDeque<Waiter>>,
    /// 接收等待队列
    recv_waiters: Mutex<VecDeque<Waiter>>,
    /// 是否已关闭
    closed: AtomicBool,
    /// 同步条件变量（用于阻塞操作）
    send_cond: Condvar,
    recv_cond: Condvar,
}

impl Channel {
    /// 创建无缓冲 Channel
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    /// 创建带缓冲 Channel
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            id: CHANNEL_ID.fetch_add(1, Ordering::Relaxed),
            capacity,
            buffer: Mutex::new(VecDeque::with_capacity(capacity)),
            send_waiters: Mutex::new(VecDeque::new()),
            recv_waiters: Mutex::new(VecDeque::new()),
            closed: AtomicBool::new(false),
            send_cond: Condvar::new(),
            recv_cond: Condvar::new(),
        }
    }

    /// 获取 Channel ID
    #[inline]
    pub fn id(&self) -> u64 {
        self.id
    }

    /// 获取容量
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// 获取当前缓冲区长度
    pub fn len(&self) -> usize {
        self.buffer.lock().len()
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.buffer.lock().is_empty()
    }

    /// 检查是否已关闭
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// 发送值（阻塞）
    ///
    /// 返回 Ok(()) 表示发送成功，Err(value) 表示 Channel 已关闭
    pub fn send(&self, value: Value) -> Result<(), Value> {
        if self.is_closed() {
            return Err(value);
        }

        // 尝试非阻塞发送
        if self.try_send_internal(value.clone()) {
            return Ok(());
        }

        // 需要阻塞等待
        let mut buffer = self.buffer.lock();
        
        loop {
            if self.is_closed() {
                return Err(value);
            }

            // 检查是否有接收者等待
            {
                let mut recv_waiters = self.recv_waiters.lock();
                if let Some(_waiter) = recv_waiters.pop_front() {
                    // 直接交付给等待者
                    buffer.push_back(value);
                    self.recv_cond.notify_one();
                    return Ok(());
                }
            }

            // 检查缓冲区是否有空间
            if self.capacity == 0 || buffer.len() < self.capacity {
                buffer.push_back(value);
                self.recv_cond.notify_one();
                return Ok(());
            }

            // 等待空间
            self.send_cond.wait(&mut buffer);
        }
    }

    /// 尝试发送（非阻塞）
    pub fn try_send(&self, value: Value) -> bool {
        if self.is_closed() {
            return false;
        }
        self.try_send_internal(value)
    }

    fn try_send_internal(&self, value: Value) -> bool {
        let mut buffer = self.buffer.lock();
        
        // 无缓冲 Channel 需要有接收者等待
        if self.capacity == 0 {
            // 检查是否有接收者等待
            let recv_waiters = self.recv_waiters.lock();
            if recv_waiters.is_empty() {
                return false;
            }
            drop(recv_waiters);
            buffer.push_back(value);
            self.recv_cond.notify_one();
            return true;
        }

        // 带缓冲 Channel
        if buffer.len() < self.capacity {
            buffer.push_back(value);
            self.recv_cond.notify_one();
            true
        } else {
            false
        }
    }

    /// 接收值（阻塞）
    ///
    /// 返回 Some(value) 表示接收成功，None 表示 Channel 已关闭且为空
    pub fn receive(&self) -> Option<Value> {
        // 尝试非阻塞接收
        if let Some(value) = self.try_receive() {
            return Some(value);
        }

        // 需要阻塞等待
        let mut buffer = self.buffer.lock();
        
        loop {
            if let Some(value) = buffer.pop_front() {
                self.send_cond.notify_one();
                return Some(value);
            }

            if self.is_closed() {
                return None;
            }

            // 等待数据
            self.recv_cond.wait(&mut buffer);
        }
    }

    /// 尝试接收（非阻塞）
    pub fn try_receive(&self) -> Option<Value> {
        let mut buffer = self.buffer.lock();
        if let Some(value) = buffer.pop_front() {
            self.send_cond.notify_one();
            Some(value)
        } else {
            None
        }
    }

    /// 关闭 Channel
    pub fn close(&self) -> bool {
        if self.closed.swap(true, Ordering::AcqRel) {
            return false;  // 已经关闭
        }

        // 唤醒所有等待者
        self.send_cond.notify_all();
        self.recv_cond.notify_all();

        true
    }

    /// 获取发送等待者数量
    pub fn send_waiters_count(&self) -> usize {
        self.send_waiters.lock().len()
    }

    /// 获取接收等待者数量
    pub fn recv_waiters_count(&self) -> usize {
        self.recv_waiters.lock().len()
    }
}

impl Default for Channel {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Channel")
            .field("id", &self.id)
            .field("capacity", &self.capacity)
            .field("len", &self.len())
            .field("closed", &self.is_closed())
            .finish()
    }
}

unsafe impl Send for Channel {}
unsafe impl Sync for Channel {}

/// 带类型的 Channel 包装
pub struct TypedChannel<T> {
    inner: Channel,
    _marker: std::marker::PhantomData<T>,
}

impl<T: Into<Value> + TryFrom<Value>> TypedChannel<T> {
    /// 创建无缓冲 Channel
    pub fn new() -> Self {
        Self {
            inner: Channel::new(),
            _marker: std::marker::PhantomData,
        }
    }

    /// 创建带缓冲 Channel
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Channel::with_capacity(capacity),
            _marker: std::marker::PhantomData,
        }
    }

    /// 发送值
    pub fn send(&self, value: T) -> Result<(), T> 
    where 
        T: Clone,
    {
        match self.inner.send(value.clone().into()) {
            Ok(()) => Ok(()),
            Err(_) => Err(value),
        }
    }

    /// 尝试发送
    pub fn try_send(&self, value: T) -> bool {
        self.inner.try_send(value.into())
    }

    /// 接收值
    pub fn receive(&self) -> Option<T> {
        self.inner.receive().and_then(|v| T::try_from(v).ok())
    }

    /// 尝试接收
    pub fn try_receive(&self) -> Option<T> {
        self.inner.try_receive().and_then(|v| T::try_from(v).ok())
    }

    /// 关闭 Channel
    pub fn close(&self) -> bool {
        self.inner.close()
    }

    /// 检查是否已关闭
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }
}

impl<T: Into<Value> + TryFrom<Value>> Default for TypedChannel<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_unbuffered() {
        use std::thread;
        
        let ch = Arc::new(Channel::new());
        let ch_clone = Arc::clone(&ch);
        
        let sender = thread::spawn(move || {
            ch_clone.send(Value::int(42)).unwrap();
        });
        
        // 需要接收者，否则发送会阻塞
        // 由于无缓冲，我们在另一个线程接收
        let ch_clone2 = Arc::clone(&ch);
        let receiver = thread::spawn(move || {
            ch_clone2.receive()
        });
        
        sender.join().unwrap();
        let value = receiver.join().unwrap();
        
        assert!(value.is_some());
    }

    #[test]
    fn test_channel_buffered() {
        let ch = Channel::with_capacity(2);
        
        assert!(ch.try_send(Value::int(1)));
        assert!(ch.try_send(Value::int(2)));
        assert!(!ch.try_send(Value::int(3)));  // 缓冲区满
        
        assert_eq!(ch.try_receive().map(|v| v.as_int()), Some(Some(1)));
        assert_eq!(ch.try_receive().map(|v| v.as_int()), Some(Some(2)));
        assert!(ch.try_receive().is_none());
    }

    #[test]
    fn test_channel_close() {
        let ch = Channel::with_capacity(1);
        
        ch.try_send(Value::int(42));
        ch.close();
        
        assert!(ch.is_closed());
        assert!(!ch.try_send(Value::int(100)));  // 发送失败
        
        // 仍可接收已缓冲的值
        assert_eq!(ch.try_receive().map(|v| v.as_int()), Some(Some(42)));
        assert!(ch.try_receive().is_none());
    }
}
