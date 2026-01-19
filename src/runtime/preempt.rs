//! 抢占式调度
//!
//! 实现基于安全点和信号的抢占机制

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// 抢占配置
#[derive(Debug, Clone)]
pub struct PreemptConfig {
    /// 时间片大小（微秒）
    pub time_slice_us: u64,
    /// 是否启用抢占
    pub enabled: bool,
    /// 安全点检查间隔（指令数）
    pub safepoint_interval: usize,
}

impl Default for PreemptConfig {
    fn default() -> Self {
        Self {
            time_slice_us: 10_000,  // 10ms
            enabled: true,
            safepoint_interval: 1000,
        }
    }
}

/// 全局抢占状态
pub struct PreemptState {
    /// 是否启用抢占
    enabled: AtomicBool,
    /// 时间片（微秒）
    time_slice_us: AtomicU64,
    /// 安全点检查间隔
    safepoint_interval: AtomicU64,
}

impl PreemptState {
    /// 创建新的抢占状态
    pub const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(true),
            time_slice_us: AtomicU64::new(10_000),
            safepoint_interval: AtomicU64::new(1000),
        }
    }

    /// 应用配置
    pub fn configure(&self, config: &PreemptConfig) {
        self.enabled.store(config.enabled, Ordering::Release);
        self.time_slice_us.store(config.time_slice_us, Ordering::Release);
        self.safepoint_interval.store(config.safepoint_interval as u64, Ordering::Release);
    }

    /// 检查是否启用抢占
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// 获取时间片
    #[inline]
    pub fn time_slice(&self) -> Duration {
        Duration::from_micros(self.time_slice_us.load(Ordering::Relaxed))
    }

    /// 获取安全点间隔
    #[inline]
    pub fn safepoint_interval(&self) -> usize {
        self.safepoint_interval.load(Ordering::Relaxed) as usize
    }
}

/// 全局抢占状态
pub static PREEMPT_STATE: PreemptState = PreemptState::new();

/// 协程抢占追踪器
///
/// 跟踪单个协程的执行时间和抢占状态
pub struct PreemptTracker {
    /// 开始执行时间
    start_time: Instant,
    /// 执行的指令数
    instruction_count: usize,
    /// 是否应该抢占
    should_preempt: bool,
}

impl PreemptTracker {
    /// 创建新的追踪器
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            instruction_count: 0,
            should_preempt: false,
        }
    }

    /// 重置追踪器
    pub fn reset(&mut self) {
        self.start_time = Instant::now();
        self.instruction_count = 0;
        self.should_preempt = false;
    }

    /// 检查并更新抢占状态
    ///
    /// 在安全点调用，返回是否应该抢占
    #[inline]
    pub fn check(&mut self) -> bool {
        if self.should_preempt {
            return true;
        }

        if !PREEMPT_STATE.is_enabled() {
            return false;
        }

        self.instruction_count += 1;

        // 检查指令数
        let interval = PREEMPT_STATE.safepoint_interval();
        if self.instruction_count >= interval {
            // 检查时间片
            let elapsed = self.start_time.elapsed();
            if elapsed >= PREEMPT_STATE.time_slice() {
                self.should_preempt = true;
                return true;
            }
            // 重置指令计数
            self.instruction_count = 0;
        }

        false
    }

    /// 强制标记为需要抢占
    pub fn force_preempt(&mut self) {
        self.should_preempt = true;
    }

    /// 获取执行时间
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// 获取指令计数
    pub fn instruction_count(&self) -> usize {
        self.instruction_count
    }
}

impl Default for PreemptTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// 安全点宏
///
/// 在代码中插入安全点检查
#[macro_export]
macro_rules! safepoint {
    ($tracker:expr) => {
        if $tracker.check() {
            // 需要抢占，返回让出
            return;
        }
    };
    ($tracker:expr, $yield_action:expr) => {
        if $tracker.check() {
            $yield_action;
        }
    };
}

/// Windows 平台特定的抢占实现
#[cfg(windows)]
pub mod platform {
    use super::*;
    
    /// 初始化 Windows 平台抢占支持
    pub fn init() {
        // Windows 上使用协作式抢占（基于安全点）
        // 不需要特殊初始化
    }
    
    /// 请求线程抢占（Windows 实现）
    /// 
    /// 注意：Windows 上使用 SuspendThread 有风险，
    /// 我们改用协作式方法
    pub fn request_preempt(_thread_id: u64) {
        // 通过设置标记来请求抢占
        // 实际抢占发生在安全点检查时
    }
}

/// Unix 平台特定的抢占实现
#[cfg(unix)]
pub mod platform {
    use super::*;
    
    /// 初始化 Unix 平台抢占支持
    pub fn init() {
        // 可以设置信号处理器
        // 但为了简单和安全，我们使用协作式抢占
    }
    
    /// 请求线程抢占（Unix 实现）
    pub fn request_preempt(_thread_id: u64) {
        // 通过设置标记来请求抢占
    }
}

/// 抢占守卫
///
/// RAII 风格的抢占控制
pub struct PreemptGuard {
    was_enabled: bool,
}

impl PreemptGuard {
    /// 禁用抢占并创建守卫
    pub fn disable() -> Self {
        let was_enabled = PREEMPT_STATE.enabled.swap(false, Ordering::AcqRel);
        Self { was_enabled }
    }
}

impl Drop for PreemptGuard {
    fn drop(&mut self) {
        if self.was_enabled {
            PREEMPT_STATE.enabled.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_preempt_tracker() {
        // 配置短时间片以便测试
        PREEMPT_STATE.time_slice_us.store(1000, Ordering::Release);  // 1ms
        PREEMPT_STATE.safepoint_interval.store(1, Ordering::Release);  // 每条指令检查
        
        let mut tracker = PreemptTracker::new();
        
        // 初始不应该抢占
        assert!(!tracker.check());
        
        // 模拟运行一段时间
        thread::sleep(Duration::from_millis(2));
        
        // 现在应该抢占
        assert!(tracker.check());
    }

    #[test]
    fn test_preempt_guard() {
        // 确保初始启用
        PREEMPT_STATE.enabled.store(true, Ordering::Release);
        
        {
            let _guard = PreemptGuard::disable();
            assert!(!PREEMPT_STATE.is_enabled());
        }
        
        // 守卫销毁后应该恢复
        assert!(PREEMPT_STATE.is_enabled());
    }
}
