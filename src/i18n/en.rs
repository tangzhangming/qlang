//! English messages

use super::messages::*;

/// Get English message
pub fn get(key: &str) -> &'static str {
    match key {
        // Compile errors
        ERR_COMPILE_UNEXPECTED_TOKEN => "Unexpected token: {}",
        ERR_COMPILE_EXPECTED_EXPRESSION => "Expected expression",
        ERR_COMPILE_UNTERMINATED_STRING => "Unterminated string",
        ERR_COMPILE_INVALID_NUMBER => "Invalid number: {}",
        ERR_COMPILE_EXPECTED_TOKEN => "Expected '{}', found '{}'",
        ERR_COMPILE_EXPECTED_TYPE => "Expected type annotation",
        ERR_COMPILE_EXPECTED_IDENTIFIER => "Expected identifier",
        ERR_COMPILE_UNDEFINED_VARIABLE => "Undefined variable: '{}'",
        ERR_COMPILE_VARIABLE_ALREADY_DEFINED => "Variable '{}' is already defined in this scope",
        ERR_COMPILE_CANNOT_ASSIGN_TO_CONST => "Cannot assign to constant '{}'",
        ERR_COMPILE_TYPE_MISMATCH => "Type mismatch: expected '{}', found '{}'",
        ERR_COMPILE_BREAK_OUTSIDE_LOOP => "'break' can only be used inside a loop",
        ERR_COMPILE_CONTINUE_OUTSIDE_LOOP => "'continue' can only be used inside a loop",
        ERR_COMPILE_UNKNOWN_FUNCTION => "Unknown function: '{}'",
        ERR_COMPILE_CONSTRUCTOR_OVERLOAD => "Constructor overloading is not allowed. Only one 'init' method is permitted",
        ERR_COMPILE_CONSTRUCTOR_RETURN => "Constructor 'init' cannot have a return type",
        ERR_COMPILE_CONSTRUCTOR_VISIBILITY => "Constructor 'init' must be public (default visibility)",
        ERR_COMPILE_EXPECTED_VAR_OR_FUNC => "Expected 'var', 'const', or 'func' in class body",
        ERR_COMPILE_CATCH_MISSING_TYPE => "Catch parameter must have a type annotation, e.g. catch (e:Exception)",
        
        // Type check errors
        ERR_TYPE_UNDEFINED_TYPE => "Undefined type: '{}'",
        ERR_TYPE_INCOMPATIBLE => "Incompatible types: '{}' and '{}'",
        ERR_TYPE_CANNOT_CALL => "Cannot call non-function type '{}'",
        ERR_TYPE_WRONG_ARG_COUNT => "Expected {} argument(s), but found {}",
        ERR_TYPE_UNDEFINED_FIELD => "Type '{}' has no field '{}'",
        ERR_TYPE_UNDEFINED_METHOD => "Type '{}' has no method '{}'",
        ERR_TYPE_CANNOT_INDEX => "Cannot index type '{}'",
        ERR_TYPE_CANNOT_ITERATE => "Cannot iterate over type '{}'",
        ERR_TYPE_NOT_NULLABLE => "Type '{}' is not nullable. Use '?' to make it nullable",
        ERR_TYPE_ABSTRACT_INSTANTIATE => "Cannot instantiate abstract class '{}'",
        ERR_TYPE_TRAIT_NOT_IMPL => "Type '{}' does not implement trait '{}'",
        ERR_TYPE_GENERIC_ARGS => "Wrong number of type arguments: expected {}, found {}",
        ERR_TYPE_TOP_LEVEL_CODE => "Top-level code not allowed: only class, struct, function, enum, interface, trait, and type definitions are permitted",
        ERR_TYPE_NO_MAIN => "Entry file missing main function: define 'func main()' as the entry point",
        ERR_TYPE_DUPLICATE_MAIN => "Duplicate main function: only one main function is allowed per package",
        ERR_TYPE_INVALID_MAIN_SIGNATURE => "Invalid main function signature: should be 'func main()' with no parameters and no return value",
        ERR_TYPE_PACKAGE_MISMATCH => "Package name mismatch: expected '{}', found '{}'",
        ERR_TYPE_PACKAGE_NOT_ALLOWED => "Package declaration not allowed in standalone file",
        
        // Runtime errors
        ERR_RUNTIME_DIVISION_BY_ZERO => "Division by zero",
        ERR_RUNTIME_TYPE_MISMATCH => "Runtime type error: expected '{}', found '{}'",
        ERR_RUNTIME_STACK_OVERFLOW => "Stack overflow",
        ERR_RUNTIME_STACK_UNDERFLOW => "Stack underflow",
        ERR_RUNTIME_INDEX_OUT_OF_BOUNDS => "Index {} is out of bounds (length: {})",
        ERR_RUNTIME_NULL_POINTER => "Null pointer dereference",
        ERR_RUNTIME_ASSERTION_FAILED => "Assertion failed: {}",
        ERR_RUNTIME_INVALID_OPERATION => "Invalid operation: {}",
        
        // Concurrent errors
        ERR_CONCURRENT_CHANNEL_CLOSED => "Cannot send on closed channel",
        ERR_CONCURRENT_DEADLOCK => "Potential deadlock detected",
        ERR_CONCURRENT_SEND_FAILED => "Failed to send value to channel",
        ERR_CONCURRENT_RECV_FAILED => "Failed to receive value from channel",
        ERR_CONCURRENT_MUTEX_POISONED => "Mutex is poisoned (a thread panicked while holding the lock)",
        
        // GC messages
        MSG_GC_STARTED => "GC started (generation: {})",
        MSG_GC_COMPLETED => "GC completed in {} ms",
        MSG_GC_FREED => "GC freed {} objects ({} bytes)",
        
        // CLI messages
        MSG_CLI_USAGE => "Usage: {} <command> [options] <file>",
        MSG_CLI_VERSION => "{} version {}",
        MSG_CLI_COMPILING => "Compiling {}...",
        MSG_CLI_RUNNING => "Running {}...",
        MSG_CLI_DONE => "Done.",
        MSG_CLI_ERROR => "Error: {}",
        MSG_CLI_FILE_NOT_FOUND => "File not found: {}",
        MSG_CLI_INVALID_EXTENSION => "Invalid file extension: '{}'. Expected '.{}' file",
        MSG_CLI_CANNOT_READ_FILE => "Cannot read file {}: {}",
        MSG_CLI_PARSE_FAILED => "Failed to parse {}:\n{}",
        MSG_CLI_SYNTAX_ERROR => "[Syntax Error]",
        MSG_CLI_IMPORT_ERROR => "[Import Error]",
        MSG_CLI_TYPE_ERROR => "[Type Error]",
        MSG_CLI_COMPILE_ERROR => "[Compile Error]",
        MSG_CLI_RUNTIME_ERROR => "[Runtime Error]",
        MSG_CLI_HELP => "Q Language - A modern, production-ready programming language",
        MSG_CLI_COMMANDS => "Commands:\n  run <file>     Run a Q source file\n  build <file>   Compile a Q source file\n  repl           Start interactive REPL\n  help           Show this help message",
        
        // Hints
        HINT_DID_YOU_MEAN => "Did you mean '{}'?",
        HINT_CHECK_SPELLING => "Check the spelling or make sure the item is defined",
        HINT_MISSING_IMPORT => "You might need to import this module first",
        HINT_TYPE_ANNOTATION => "Consider adding a type annotation here",
        HINT_USE_NULL_CHECK => "Use '?.' for safe access or '!' for non-null assertion",
        
        // Unknown message key
        _ => "Unknown message key",
    }
}
