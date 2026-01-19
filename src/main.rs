//! Q 语言编译器和虚拟机
//! 
//! 主入口点

mod config;
mod i18n;
mod lexer;
mod parser;
mod compiler;
mod vm;
mod types;
mod package;
mod stdlib;

use std::env;
use std::fs;
use std::process;

use config::{LANG_NAME, VERSION, SOURCE_EXTENSION};
use i18n::{Locale, format_message, messages};
use lexer::Scanner;
use parser::Parser;
use compiler::Compiler;
use vm::VM;

/// 运行源代码
fn run(source: &str, locale: Locale) -> Result<(), String> {
    // 词法分析
    let mut scanner = Scanner::new(source);
    let tokens = scanner.scan_tokens();
    
    // 检查词法错误
    for token in &tokens {
        if token.is_error() {
            if let lexer::TokenKind::Error(msg) = &token.kind {
                return Err(format!(
                    "[{}:{}] {}",
                    token.span.line, token.span.column, msg
                ));
            }
        }
    }
    
    // 语法分析
    let mut parser = Parser::new(tokens, locale);
    let program = parser.parse().map_err(|errors| {
        errors
            .iter()
            .map(|e| format!("[{}:{}] {}", e.span.line, e.span.column, e.message))
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    
    // 编译
    let mut compiler = Compiler::new(locale);
    let chunk = compiler.compile(&program).map_err(|errors| {
        errors
            .iter()
            .map(|e| format!("[{}:{}] {}", e.span.line, e.span.column, e.message))
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    
    // 执行
    let mut vm = VM::new(chunk, locale);
    vm.run().map_err(|e| format!("[line {}] {}", e.line, e.message))?;
    
    Ok(())
}

/// 运行文件
fn run_file(path: &str, locale: Locale) {
    // 检查文件后缀
    let expected_ext = format!(".{}", SOURCE_EXTENSION);
    if !path.ends_with(&expected_ext) {
        let msg = format_message(
            messages::MSG_CLI_INVALID_EXTENSION, 
            locale, 
            &[path, SOURCE_EXTENSION]
        );
        eprintln!("{}", msg);
        process::exit(1);
    }
    
    let source = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => {
            let msg = format_message(messages::MSG_CLI_FILE_NOT_FOUND, locale, &[path]);
            eprintln!("{}", msg);
            process::exit(1);
        }
    };
    
    if let Err(e) = run(&source, locale) {
        eprintln!("{}", format_message(messages::MSG_CLI_ERROR, locale, &[&e]));
        process::exit(1);
    }
}

/// REPL 交互模式
fn repl(locale: Locale) {
    use std::io::{self, Write};
    
    println!("{} {} REPL", LANG_NAME, VERSION);
    println!("Type 'exit' to quit.\n");
    
    loop {
        print!("> ");
        io::stdout().flush().unwrap();
        
        let mut line = String::new();
        if io::stdin().read_line(&mut line).is_err() {
            break;
        }
        
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "exit" || line == "quit" {
            break;
        }
        
        if let Err(e) = run(line, locale) {
            eprintln!("{}", e);
        }
    }
}

/// 打印帮助信息
fn print_help(locale: Locale) {
    let usage = format_message(messages::MSG_CLI_USAGE, locale, &[LANG_NAME]);
    println!("{}", usage);
    println!();
    println!("Commands:");
    println!("  run <file>     Run a source file");
    println!("  repl           Start interactive mode");
    println!("  help           Show this help message");
    println!("  version        Show version information");
    println!();
    println!("Options:");
    println!("  --lang <en|zh> Set language (default: en)");
}

/// 打印版本信息
fn print_version(locale: Locale) {
    let msg = format_message(messages::MSG_CLI_VERSION, locale, &[LANG_NAME, VERSION]);
    println!("{}", msg);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    
    // 默认语言
    let mut locale = Locale::En;
    
    // 解析语言选项
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--lang" && i + 1 < args.len() {
            locale = match args[i + 1].as_str() {
                "zh" | "cn" | "chinese" => Locale::Zh,
                _ => Locale::En,
            };
            i += 2;
        } else {
            break;
        }
    }
    
    // 剩余参数
    let remaining: Vec<&str> = args[i..].iter().map(|s| s.as_str()).collect();
    
    match remaining.as_slice() {
        [] | ["repl"] => repl(locale),
        ["help"] | ["--help"] | ["-h"] => print_help(locale),
        ["version"] | ["--version"] | ["-v"] => print_version(locale),
        ["run", path] => run_file(path, locale),
        [path] if path.ends_with(&format!(".{}", SOURCE_EXTENSION)) => {
            run_file(path, locale)
        }
        _ => {
            print_help(locale);
            process::exit(1);
        }
    }
}
