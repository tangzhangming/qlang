//! 配置常量模块
//! 
//! 所有可配置的语言相关常量都在这里定义，便于后期修改

/// 语言名称
pub const LANG_NAME: &str = "Q";

/// 源码文件扩展名
#[allow(dead_code)]
pub const SOURCE_EXTENSION: &str = "q";

/// 字节码文件扩展名
#[allow(dead_code)]
pub const BYTECODE_EXTENSION: &str = "qlc";

/// 标准库前缀
#[allow(dead_code)]
pub const STD_PREFIX: &str = "std";

/// 版本号
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
