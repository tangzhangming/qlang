//! 运行时入口
//!
//! 协调 VM 和调度器的集成

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use parking_lot::{Mutex, RwLock};

use crate::compiler::Chunk;
use crate::i18n::Locale;
use crate::vm::{VM, Value};
use crate::vm::value::Function;
use crate::vm::vm::RuntimeError;

use super::scheduler::{Scheduler, get_scheduler};
use super::goroutine::{Goroutine, GoroutineStatus};
use super::coroutine_vm::{CoroutineVM, ExecuteResult};
use super::preempt::PreemptTracker;
use super::channel::Channel;

/// 运行时配置
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// 语言环境
    pub locale: Locale,
    /// 是否启用并发
    pub enable_concurrency: bool,
    /// 工作线程数（0 表示使用 CPU 核心数）
    pub num_workers: usize,
    /// 时间片大小（微秒）
    pub time_slice_us: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            locale: Locale::Zh,
            enable_concurrency: true,
            num_workers: 0,
            time_slice_us: 10_000,
        }
    }
}

/// Q 语言运行时
///
/// 管理 VM 和协程调度
pub struct Runtime {
    /// 配置
    config: RuntimeConfig,
    /// 字节码
    chunk: Arc<Chunk>,
    /// 调度器引用
    scheduler: &'static Scheduler,
    /// 是否正在运行
    running: AtomicBool,
    /// 主协程返回值
    result: Mutex<Option<Result<(), RuntimeError>>>,
    /// 全局 Channel 注册表
    channels: RwLock<Vec<Arc<Channel>>>,
}

impl Runtime {
    /// 创建新的运行时
    pub fn new(chunk: Arc<Chunk>, config: RuntimeConfig) -> Self {
        Self {
            config,
            chunk,
            scheduler: get_scheduler(),
            running: AtomicBool::new(false),
            result: Mutex::new(None),
            channels: RwLock::new(Vec::new()),
        }
    }

    /// 使用默认配置创建运行时
    pub fn with_chunk(chunk: Arc<Chunk>) -> Self {
        Self::new(chunk, RuntimeConfig::default())
    }

    /// 运行程序（阻塞）
    pub fn run(&self) -> Result<(), RuntimeError> {
        self.running.store(true, Ordering::Release);
        
        // 设置调度器的字节码
        self.scheduler.set_chunk(Arc::clone(&self.chunk));
        
        // 设置协程执行器
        let chunk = Arc::clone(&self.chunk);
        let locale = self.config.locale;
        
        self.scheduler.set_executor(move |g| {
            let mut cvm = CoroutineVM::new(Arc::clone(&chunk), locale);
            cvm.set_time_slice(10000);  // 10000 条指令
            
            match cvm.execute(g) {
                ExecuteResult::Completed => {
                    g.mark_dead();
                }
                ExecuteResult::Yielded => {
                    // 协程让出，保持 Runnable 状态
                }
                ExecuteResult::Blocked => {
                    // 协程阻塞
                }
                ExecuteResult::Error(e) => {
                    eprintln!("Coroutine {} error: {}", g.id, e.message);
                    g.mark_dead();
                }
            }
        });
        
        // 如果启用并发，启动调度器
        if self.config.enable_concurrency {
            self.scheduler.start();
        }
        
        // 使用主 VM 运行主函数
        let mut main_vm = VM::new(Arc::clone(&self.chunk), self.config.locale);
        
        // 设置运行时引用以支持 go 语句
        // （VM 会通过调度器创建新协程）
        
        // 运行主 VM（同步执行）
        let result = main_vm.run();
        
        self.running.store(false, Ordering::Release);
        
        // 停止调度器
        if self.config.enable_concurrency {
            self.scheduler.stop();
        }
        
        result
    }

    /// 创建新协程
    pub fn spawn(&self, func: Arc<Function>, args: Vec<Value>) -> Option<u64> {
        self.scheduler.spawn(func, args)
    }

    /// 创建新 Channel
    pub fn create_channel(&self, capacity: usize) -> Arc<Channel> {
        let ch = Arc::new(Channel::with_capacity(capacity));
        self.channels.write().push(Arc::clone(&ch));
        ch
    }

    /// 检查是否正在运行
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// 获取调度器统计信息
    pub fn stats(&self) -> super::scheduler::SchedulerStats {
        self.scheduler.stats()
    }
}

/// 运行时句柄（用于在 VM 中访问运行时功能）
pub struct RuntimeHandle {
    scheduler: &'static Scheduler,
    chunk: Arc<Chunk>,
    locale: Locale,
}

impl RuntimeHandle {
    /// 创建新的运行时句柄
    pub fn new(chunk: Arc<Chunk>, locale: Locale) -> Self {
        Self {
            scheduler: get_scheduler(),
            chunk,
            locale,
        }
    }

    /// 创建协程
    pub fn go(&self, func: Arc<Function>, args: Vec<Value>) -> Option<u64> {
        self.scheduler.spawn(func, args)
    }

    /// 让出当前协程
    pub fn yield_now(&self) {
        // 在协作式调度中，这会在下一个安全点生效
    }
}

/// 线程本地运行时句柄
thread_local! {
    static RUNTIME_HANDLE: std::cell::RefCell<Option<RuntimeHandle>> = std::cell::RefCell::new(None);
}

/// 设置当前线程的运行时句柄
pub fn set_runtime_handle(handle: RuntimeHandle) {
    RUNTIME_HANDLE.with(|h| {
        *h.borrow_mut() = Some(handle);
    });
}

/// 获取当前线程的运行时句柄
pub fn with_runtime_handle<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&RuntimeHandle) -> R,
{
    RUNTIME_HANDLE.with(|h| {
        h.borrow().as_ref().map(f)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_config() {
        let config = RuntimeConfig::default();
        assert!(config.enable_concurrency);
    }
}
