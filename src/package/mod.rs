//! 包管理模块
//! 
//! 负责处理包声明、导入解析、依赖管理

mod project;
mod resolver;

pub use project::{ProjectConfig, find_project_root};
pub use resolver::{PackageResolver, ResolvedImport, ImportKind};
