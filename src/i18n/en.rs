//! 英文消息

use super::messages::*;

/// 获取英文消息
pub fn get(key: &str) -> &'static str {
    match key {
        // 编译错误
        ERR_COMPILE_UNEXPECTED_TOKEN => "Unexpected token: {}",
        ERR_COMPILE_EXPECTED_EXPRESSION => "Expected expression",
        ERR_COMPILE_UNTERMINATED_STRING => "Unterminated string",
        ERR_COMPILE_INVALID_NUMBER => "Invalid number: {}",
        ERR_COMPILE_EXPECTED_TOKEN => "Expected '{}', found '{}'",
        ERR_COMPILE_EXPECTED_TYPE => "Expected type",
        ERR_COMPILE_EXPECTED_IDENTIFIER => "Expected identifier",
        ERR_COMPILE_UNDEFINED_VARIABLE => "Undefined variable: {}",
        ERR_COMPILE_VARIABLE_ALREADY_DEFINED => "Variable '{}' is already defined",
        ERR_COMPILE_CANNOT_ASSIGN_TO_CONST => "Cannot assign to constant '{}'",
        ERR_COMPILE_TYPE_MISMATCH => "Type mismatch: expected {}, found {}",
        ERR_COMPILE_BREAK_OUTSIDE_LOOP => "'break' outside of loop",
        ERR_COMPILE_CONTINUE_OUTSIDE_LOOP => "'continue' outside of loop",
        ERR_COMPILE_UNKNOWN_FUNCTION => "Unknown function: {}",
        
        // 运行时错误
        ERR_RUNTIME_DIVISION_BY_ZERO => "Division by zero",
        ERR_RUNTIME_TYPE_MISMATCH => "Type mismatch: expected {}, found {}",
        ERR_RUNTIME_STACK_OVERFLOW => "Stack overflow",
        ERR_RUNTIME_STACK_UNDERFLOW => "Stack underflow",
        
        // CLI 消息
        MSG_CLI_USAGE => "Usage: {} <command> [options] <file>",
        MSG_CLI_VERSION => "{} version {}",
        MSG_CLI_COMPILING => "Compiling {}...",
        MSG_CLI_RUNNING => "Running {}...",
        MSG_CLI_DONE => "Done.",
        MSG_CLI_ERROR => "Error: {}",
        MSG_CLI_FILE_NOT_FOUND => "File not found: {}",
        MSG_CLI_INVALID_EXTENSION => "Invalid file extension: '{}'. Expected '.{}' file",
        
        // 未知消息键
        _ => "Unknown message key",
    }
}
