//! Bytecode compiler module
//!
//! Compiles AST to bytecode

pub mod bytecode;
pub mod codegen;
pub mod symbol;

pub use bytecode::{Chunk, OpCode};
pub use codegen::Compiler;
