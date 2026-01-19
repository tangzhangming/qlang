//! 虚拟机模块
//! 
//! 执行字节码指令

pub mod value;
pub mod vm;
pub mod vtable;
pub mod gc;

pub use value::Value;
pub use vm::VM;
pub use vtable::{VTable, VTableRegistry, TraitVTable, RuntimeTypeInfo};
pub use gc::{Heap, MarkSweepGc, ConcurrentMarkGc, GcResult, GcStats, get_heap, gc_register, gc_should_run, gc_stats};
