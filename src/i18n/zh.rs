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
        ERR_COMPILE_EXPECTED_TYPE => "期望类型注解",
        ERR_COMPILE_EXPECTED_IDENTIFIER => "期望标识符",
        ERR_COMPILE_UNDEFINED_VARIABLE => "未定义的变量: '{}'",
        ERR_COMPILE_VARIABLE_ALREADY_DEFINED => "变量 '{}' 在当前作用域已定义",
        ERR_COMPILE_CANNOT_ASSIGN_TO_CONST => "不能给常量 '{}' 赋值",
        ERR_COMPILE_TYPE_MISMATCH => "类型不匹配: 期望 '{}'，实际是 '{}'",
        ERR_COMPILE_BREAK_OUTSIDE_LOOP => "'break' 只能在循环内使用",
        ERR_COMPILE_CONTINUE_OUTSIDE_LOOP => "'continue' 只能在循环内使用",
        ERR_COMPILE_UNKNOWN_FUNCTION => "未知的函数: '{}'",
        ERR_COMPILE_CONSTRUCTOR_OVERLOAD => "不允许构造函数重载：只能定义一个 'init' 方法",
        ERR_COMPILE_CONSTRUCTOR_RETURN => "构造函数 'init' 不能有返回类型",
        ERR_COMPILE_CONSTRUCTOR_VISIBILITY => "构造函数 'init' 必须是 public（默认可见性）",
        ERR_COMPILE_EXPECTED_VAR_OR_FUNC => "类中只能定义 'var'、'const' 字段或 'func' 方法",
        
        // 类型检查错误
        ERR_TYPE_UNDEFINED_TYPE => "未定义的类型: '{}'",
        ERR_TYPE_INCOMPATIBLE => "不兼容的类型: '{}' 和 '{}'",
        ERR_TYPE_CANNOT_CALL => "无法调用非函数类型 '{}'",
        ERR_TYPE_WRONG_ARG_COUNT => "期望 {} 个参数，实际 {} 个",
        ERR_TYPE_UNDEFINED_FIELD => "类型 '{}' 没有字段 '{}'",
        ERR_TYPE_UNDEFINED_METHOD => "类型 '{}' 没有方法 '{}'",
        ERR_TYPE_CANNOT_INDEX => "无法对类型 '{}' 进行索引",
        ERR_TYPE_CANNOT_ITERATE => "无法迭代类型 '{}'",
        ERR_TYPE_NOT_NULLABLE => "类型 '{}' 不可为空。使用 '?' 使其可空",
        ERR_TYPE_ABSTRACT_INSTANTIATE => "无法实例化抽象类 '{}'",
        ERR_TYPE_TRAIT_NOT_IMPL => "类型 '{}' 未实现 trait '{}'",
        ERR_TYPE_GENERIC_ARGS => "类型参数数量错误: 期望 {} 个，实际 {} 个",
        ERR_TYPE_TOP_LEVEL_CODE => "顶级代码不允许：只能定义类、结构体、函数、枚举、接口、Trait 和类型别名",
        ERR_TYPE_NO_MAIN => "入口文件缺少 main 函数：请定义 'func main()' 作为入口点",
        ERR_TYPE_DUPLICATE_MAIN => "main 函数重复：同一个包内只允许有一个 main 函数",
        ERR_TYPE_INVALID_MAIN_SIGNATURE => "main 函数签名错误：应为 'func main()'，无参数无返回值",
        ERR_TYPE_PACKAGE_MISMATCH => "包名不匹配：期望 '{}'，实际 '{}'",
        ERR_TYPE_PACKAGE_NOT_ALLOWED => "独立文件不允许 package 声明",
        
        // 运行时错误
        ERR_RUNTIME_DIVISION_BY_ZERO => "除以零错误",
        ERR_RUNTIME_TYPE_MISMATCH => "运行时类型错误: 期望 '{}'，实际是 '{}'",
        ERR_RUNTIME_STACK_OVERFLOW => "栈溢出",
        ERR_RUNTIME_STACK_UNDERFLOW => "栈下溢",
        ERR_RUNTIME_INDEX_OUT_OF_BOUNDS => "索引 {} 越界（长度: {}）",
        ERR_RUNTIME_NULL_POINTER => "空指针解引用",
        ERR_RUNTIME_ASSERTION_FAILED => "断言失败: {}",
        ERR_RUNTIME_INVALID_OPERATION => "无效操作: {}",
        
        // 并发错误
        ERR_CONCURRENT_CHANNEL_CLOSED => "无法向已关闭的 Channel 发送数据",
        ERR_CONCURRENT_DEADLOCK => "检测到潜在的死锁",
        ERR_CONCURRENT_SEND_FAILED => "向 Channel 发送数据失败",
        ERR_CONCURRENT_RECV_FAILED => "从 Channel 接收数据失败",
        ERR_CONCURRENT_MUTEX_POISONED => "互斥锁已中毒（持有锁的线程发生恐慌）",
        
        // GC 消息
        MSG_GC_STARTED => "GC 开始（代: {}）",
        MSG_GC_COMPLETED => "GC 完成，耗时 {} 毫秒",
        MSG_GC_FREED => "GC 释放了 {} 个对象（{} 字节）",
        
        // CLI 消息
        MSG_CLI_USAGE => "用法: {} <命令> [选项] <文件>",
        MSG_CLI_VERSION => "{} 版本 {}",
        MSG_CLI_COMPILING => "正在编译 {}...",
        MSG_CLI_RUNNING => "正在运行 {}...",
        MSG_CLI_DONE => "完成。",
        MSG_CLI_ERROR => "错误: {}",
        MSG_CLI_FILE_NOT_FOUND => "文件未找到: {}",
        MSG_CLI_INVALID_EXTENSION => "无效的文件扩展名: '{}'。请使用 '.{}' 文件",
        MSG_CLI_CANNOT_READ_FILE => "无法读取文件 {}: {}",
        MSG_CLI_PARSE_FAILED => "解析 {} 失败:\n{}",
        MSG_CLI_SYNTAX_ERROR => "[语法错误]",
        MSG_CLI_IMPORT_ERROR => "[导入错误]",
        MSG_CLI_TYPE_ERROR => "[类型检查错误]",
        MSG_CLI_COMPILE_ERROR => "[编译错误]",
        MSG_CLI_RUNTIME_ERROR => "[运行时错误]",
        MSG_CLI_HELP => "Q 语言 - 一个现代化、生产级的编程语言",
        MSG_CLI_COMMANDS => "命令:\n  run <文件>     运行 Q 源文件\n  build <文件>   编译 Q 源文件\n  repl           启动交互式 REPL\n  help           显示此帮助信息",
        
        // 提示
        HINT_DID_YOU_MEAN => "你是不是想说 '{}'？",
        HINT_CHECK_SPELLING => "检查拼写或确保该项已定义",
        HINT_MISSING_IMPORT => "可能需要先导入这个模块",
        HINT_TYPE_ANNOTATION => "考虑在这里添加类型注解",
        HINT_USE_NULL_CHECK => "使用 '?.' 进行安全访问或 '!' 进行非空断言",
        
        // 未知消息键
        _ => "未知的消息键",
    }
}
