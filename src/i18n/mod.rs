//! 国际化模块
//! 
//! 提供多语言消息支持，目前支持英文、中文和日语

pub mod messages;
pub mod en;
pub mod zh;
pub mod ja;

/// 支持的语言
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    /// 英文（默认）
    #[default]
    En,
    /// 中文
    Zh,
    /// 日语
    Ja,
}

impl Locale {
    /// 从字符串解析语言
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "en" | "english" => Some(Locale::En),
            "zh" | "chinese" | "cn" => Some(Locale::Zh),
            "ja" | "japanese" | "jp" => Some(Locale::Ja),
            _ => None,
        }
    }
    
    /// 获取语言代码
    pub fn code(&self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::Zh => "zh",
            Locale::Ja => "ja",
        }
    }
    
    /// 获取语言名称
    pub fn name(&self) -> &'static str {
        match self {
            Locale::En => "English",
            Locale::Zh => "中文",
            Locale::Ja => "日本語",
        }
    }
    
    /// 从系统环境检测语言
    pub fn from_env() -> Self {
        if let Ok(lang) = std::env::var("LANG") {
            let lang = lang.to_lowercase();
            if lang.starts_with("zh") {
                return Locale::Zh;
            } else if lang.starts_with("ja") {
                return Locale::Ja;
            }
        }
        Locale::En
    }
}

/// 获取指定语言的消息
pub fn get_message(key: &str, locale: Locale) -> &'static str {
    match locale {
        Locale::En => en::get(key),
        Locale::Zh => zh::get(key),
        Locale::Ja => ja::get(key),
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

/// 带命名参数的消息格式化（使用 {name} 占位符）
pub fn format_message_named(key: &str, locale: Locale, args: &[(&str, &str)]) -> String {
    let mut msg = get_message(key, locale).to_string();
    for (name, value) in args {
        let placeholder = format!("{{{}}}", name);
        msg = msg.replace(&placeholder, value);
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_locale_from_str() {
        assert_eq!(Locale::from_str("en"), Some(Locale::En));
        assert_eq!(Locale::from_str("zh"), Some(Locale::Zh));
        assert_eq!(Locale::from_str("ja"), Some(Locale::Ja));
        assert_eq!(Locale::from_str("invalid"), None);
    }
    
    #[test]
    fn test_format_message() {
        let msg = format_message(
            messages::ERR_COMPILE_EXPECTED_TOKEN,
            Locale::En,
            &[")", "("],
        );
        assert_eq!(msg, "Expected ')', found '('");
    }
}
