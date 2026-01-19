//! 类型检查器模块
//! 
//! 实现静态类型检查、类型推导、泛型实例化等功能

mod environment;
mod unify;
mod constraint;
mod error;
mod checker;
mod monomorphize;

pub use environment::{TypeEnvironment, TypeScope, TypeInfo, FunctionInfo, ClassInfo, TraitInfo};
pub use unify::{Unifier, UnifyResult};
pub use constraint::{Constraint, ConstraintKind, ConstraintSolver};
pub use error::{TypeError, TypeErrorKind};
pub use checker::TypeChecker;
pub use monomorphize::{Monomorphizer, MonoKey, MonomorphizedClass, MonomorphizedStruct, MonomorphizedFunction};
