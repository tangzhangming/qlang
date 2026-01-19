//! 虚拟机模块
//! 
//! 执行字节码指令

pub mod value;
pub mod vm;
pub mod vtable;

pub use value::Value;
pub use vm::VM;
pub use vtable::{VTable, VTableRegistry, TraitVTable, RuntimeTypeInfo};
