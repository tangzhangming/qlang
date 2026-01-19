//! 配置常量模块
//! 
//! 所有可配置的语言相关常量都在这里定义，便于后期修改

/// 语言名称
pub const LANG_NAME: &str = "Q";

/// 源码文件扩展名
pub const SOURCE_EXTENSION: &str = "q";

/// 字节码文件扩展名
#[allow(dead_code)]
pub const BYTECODE_EXTENSION: &str = "qlc";

/// 标准库前缀（以此开头的包为标准库）
pub const STD_PREFIX: &str = "std";

/// 项目配置文件名
pub const PROJECT_FILE: &str = "project.toml";

/// 标准库目录名（相对于编译器安装目录）
pub const STDLIB_DIR: &str = "stdlib";

/// 版本号
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
