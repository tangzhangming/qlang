//! 中文消息

use super::messages::*;

/// 获取中文消息
pub fn get(key: &str) -> &'static str {
    match key {
        // 编译错误
        ERR_COMPILE_UNEXPECTED_TOKEN => "意外的标记: {}",
        ERR_COMPILE_EXPECTED_EXPRESSION => "期望表达式",
        ERR_COMPILE_UNTERMINATED_STRING => "未闭合的字符串",
        ERR_COMPILE_INVALID_NUMBER => "无效的数字: {}",
        ERR_COMPILE_EXPECTED_TOKEN => "期望 '{}'，实际是 '{}'",
        ERR_COMPILE_EXPECTED_TYPE => "期望类型",
        ERR_COMPILE_EXPECTED_IDENTIFIER => "期望标识符",
        ERR_COMPILE_UNDEFINED_VARIABLE => "未定义的变量: {}",
        ERR_COMPILE_VARIABLE_ALREADY_DEFINED => "变量 '{}' 已经定义",
        ERR_COMPILE_CANNOT_ASSIGN_TO_CONST => "不能给常量 '{}' 赋值",
        ERR_COMPILE_TYPE_MISMATCH => "类型不匹配: 期望 {}，实际是 {}",
        ERR_COMPILE_BREAK_OUTSIDE_LOOP => "'break' 在循环外使用",
        ERR_COMPILE_CONTINUE_OUTSIDE_LOOP => "'continue' 在循环外使用",
        ERR_COMPILE_UNKNOWN_FUNCTION => "未知的函数: {}",
        
        // 运行时错误
        ERR_RUNTIME_DIVISION_BY_ZERO => "除以零错误",
        ERR_RUNTIME_TYPE_MISMATCH => "类型不匹配: 期望 {}，实际是 {}",
        ERR_RUNTIME_STACK_OVERFLOW => "栈溢出",
        ERR_RUNTIME_STACK_UNDERFLOW => "栈下溢",
        
        // CLI 消息
        MSG_CLI_USAGE => "用法: {} <命令> [选项] <文件>",
        MSG_CLI_VERSION => "{} 版本 {}",
        MSG_CLI_COMPILING => "正在编译 {}...",
        MSG_CLI_RUNNING => "正在运行 {}...",
        MSG_CLI_DONE => "完成。",
        MSG_CLI_ERROR => "错误: {}",
        MSG_CLI_FILE_NOT_FOUND => "文件未找到: {}",
        MSG_CLI_INVALID_EXTENSION => "无效的文件扩展名: '{}'。请使用 '.{}' 文件",
        
        // 未知消息键
        _ => "未知的消息键",
    }
}
