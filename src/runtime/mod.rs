//! 协程运行时模块
//!
//! 实现类似 Go 语言的 M:N 协程调度系统
//!
//! 核心组件：
//! - G (Goroutine): 协程，包含自己的栈和执行上下文
//! - P (Processor): 逻辑处理器，管理本地运行队列
//! - M (Machine): 操作系统线程，执行协程

pub mod goroutine;
pub mod stack;
pub mod queue;
pub mod processor;
pub mod machine;
pub mod scheduler;
pub mod channel;
pub mod context;
pub mod coroutine_vm;
pub mod preempt;
pub mod runtime;

pub use runtime::{Runtime, RuntimeConfig, RuntimeHandle};

pub use goroutine::{Goroutine, GoroutineStatus};
pub use stack::Stack;
pub use queue::LocalQueue;
pub use processor::Processor;
pub use machine::Machine;
pub use scheduler::{Scheduler, SCHEDULER};
pub use channel::Channel;
pub use context::Context;

/// 协程 ID 类型
pub type GoId = u64;

/// 获取当前 CPU 核心数
pub fn num_processors() -> usize {
    num_cpus::get()
}
