//! 工作线程 (Machine)
//!
//! M - 操作系统线程，执行协程

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicPtr, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::ptr;
use parking_lot::{Mutex, Condvar};

use super::processor::Processor;
use super::scheduler::Scheduler;
use super::goroutine::{Goroutine, GoroutineStatus};

/// 工作线程
pub struct Machine {
    /// 线程 ID
    pub id: u64,
    /// OS 线程句柄
    thread: Mutex<Option<JoinHandle<()>>>,
    /// 当前绑定的处理器
    processor: AtomicPtr<Processor>,
    /// 是否正在休眠
    parking: AtomicBool,
    /// 是否应该停止
    should_stop: AtomicBool,
    /// 休眠/唤醒同步
    park_mutex: Mutex<bool>,
    park_cond: Condvar,
    /// 关联的调度器
    scheduler: AtomicPtr<Scheduler>,
    /// 执行的协程数量
    goroutine_count: AtomicU64,
}

impl Machine {
    /// 创建新的工作线程
    pub fn new(id: u64) -> Arc<Self> {
        Arc::new(Self {
            id,
            thread: Mutex::new(None),
            processor: AtomicPtr::new(ptr::null_mut()),
            parking: AtomicBool::new(false),
            should_stop: AtomicBool::new(false),
            park_mutex: Mutex::new(false),
            park_cond: Condvar::new(),
            scheduler: AtomicPtr::new(ptr::null_mut()),
            goroutine_count: AtomicU64::new(0),
        })
    }

    /// 设置调度器
    pub fn set_scheduler(&self, scheduler: *mut Scheduler) {
        self.scheduler.store(scheduler, Ordering::Release);
    }

    /// 获取调度器
    fn scheduler(&self) -> Option<&Scheduler> {
        let ptr = self.scheduler.load(Ordering::Acquire);
        if ptr.is_null() {
            None
        } else {
            unsafe { Some(&*ptr) }
        }
    }

    /// 获取当前处理器
    pub fn processor(&self) -> Option<&Processor> {
        let ptr = self.processor.load(Ordering::Acquire);
        if ptr.is_null() {
            None
        } else {
            unsafe { Some(&*ptr) }
        }
    }

    /// 绑定处理器
    pub fn bind_processor(&self, p: &Processor) {
        self.processor.store(p as *const Processor as *mut Processor, Ordering::Release);
        p.bind_machine(self.id);
    }

    /// 解绑处理器
    pub fn unbind_processor(&self) {
        let ptr = self.processor.swap(ptr::null_mut(), Ordering::AcqRel);
        if !ptr.is_null() {
            unsafe {
                (*ptr).unbind_machine();
            }
        }
    }

    /// 启动工作线程
    pub fn start(self: &Arc<Self>) {
        let machine = Arc::clone(self);
        let handle = thread::Builder::new()
            .name(format!("machine-{}", self.id))
            .spawn(move || {
                machine.run_loop();
            })
            .expect("Failed to spawn machine thread");
        
        *self.thread.lock() = Some(handle);
    }

    /// 主执行循环
    fn run_loop(&self) {
        loop {
            // 检查是否应该停止
            if self.should_stop.load(Ordering::Relaxed) {
                break;
            }

            // 尝试查找并执行协程
            if let Some(g) = self.find_work() {
                self.execute(g);
            } else {
                // 没有工作，休眠等待
                self.park();
            }
        }
    }

    /// 查找可执行的协程
    fn find_work(&self) -> Option<Arc<Goroutine>> {
        let scheduler = self.scheduler()?;
        let p = self.processor()?;

        // 1. 检查 next（快速路径）
        if let Some(g) = p.take_next() {
            return Some(g);
        }

        // 2. 从本地队列获取
        if let Some(g) = p.pop() {
            return Some(g);
        }

        // 3. 从全局队列获取
        if let Some(g) = scheduler.get_from_global() {
            return Some(g);
        }

        // 4. 尝试从其他 P 窃取
        if let Some(g) = scheduler.steal_work(p) {
            return Some(g);
        }

        None
    }

    /// 执行协程
    fn execute(&self, g: Arc<Goroutine>) {
        // 更新状态
        g.set_status(GoroutineStatus::Running);
        
        if let Some(p) = self.processor() {
            p.set_current(Some(Arc::clone(&g)));
            p.inc_schedule_count();
        }

        // 增加计数
        self.goroutine_count.fetch_add(1, Ordering::Relaxed);

        // 执行协程
        // 这里调用调度器来实际执行
        if let Some(scheduler) = self.scheduler() {
            scheduler.execute_goroutine(&g);
        }

        // 执行完成，更新状态
        if let Some(p) = self.processor() {
            p.set_current(None);
        }

        // 根据协程状态处理
        match g.status() {
            GoroutineStatus::Runnable => {
                // 协程让出，重新入队
                if let Some(scheduler) = self.scheduler() {
                    scheduler.schedule(g);
                }
            }
            GoroutineStatus::Waiting => {
                // 协程阻塞，等待唤醒
                // 不需要处理，协程会被放入等待队列
            }
            GoroutineStatus::Dead => {
                // 协程完成
                // Arc 会自动释放
            }
            _ => {}
        }
    }

    /// 休眠等待
    pub fn park(&self) {
        self.parking.store(true, Ordering::Release);
        
        let mut guard = self.park_mutex.lock();
        while *guard == false && !self.should_stop.load(Ordering::Relaxed) {
            self.park_cond.wait(&mut guard);
        }
        *guard = false;
        
        self.parking.store(false, Ordering::Release);
    }

    /// 唤醒线程
    pub fn unpark(&self) {
        let mut guard = self.park_mutex.lock();
        *guard = true;
        self.park_cond.notify_one();
    }

    /// 检查是否正在休眠
    #[inline]
    pub fn is_parking(&self) -> bool {
        self.parking.load(Ordering::Acquire)
    }

    /// 停止工作线程
    pub fn stop(&self) {
        self.should_stop.store(true, Ordering::Release);
        self.unpark();
    }

    /// 等待线程结束
    pub fn join(&self) {
        if let Some(handle) = self.thread.lock().take() {
            let _ = handle.join();
        }
    }

    /// 获取执行的协程数量
    #[inline]
    pub fn goroutine_count(&self) -> u64 {
        self.goroutine_count.load(Ordering::Relaxed)
    }
}

impl std::fmt::Debug for Machine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Machine")
            .field("id", &self.id)
            .field("parking", &self.is_parking())
            .field("goroutine_count", &self.goroutine_count())
            .finish()
    }
}

unsafe impl Send for Machine {}
unsafe impl Sync for Machine {}
