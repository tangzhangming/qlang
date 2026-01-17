//! 国际化模块
//! 
//! 提供多语言消息支持，目前支持英文和中文

pub mod messages;
pub mod en;
pub mod zh;

/// 支持的语言
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    /// 英文（默认）
    #[default]
    En,
    /// 中文
    Zh,
}

/// 获取指定语言的消息
pub fn get_message(key: &str, locale: Locale) -> &'static str {
    match locale {
        Locale::En => en::get(key),
        Locale::Zh => zh::get(key),
    }
}

/// 获取带参数的消息（使用 {} 占位符）
pub fn format_message(key: &str, locale: Locale, args: &[&str]) -> String {
    let mut msg = get_message(key, locale).to_string();
    for arg in args {
        if let Some(pos) = msg.find("{}") {
            msg.replace_range(pos..pos + 2, arg);
        }
    }
    msg
}
