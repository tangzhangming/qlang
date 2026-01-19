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
mod runtime;
mod typechecker;

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use config::{LANG_NAME, VERSION, SOURCE_EXTENSION, PROJECT_FILE};

/// 清理路径显示格式（移除 Windows 的 \\?\ 前缀）
fn display_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    // Windows canonicalize 返回 \\?\C:\... 格式，需要清理
    if s.starts_with(r"\\?\") {
        s[4..].to_string()
    } else {
        s.to_string()
    }
}
use i18n::{Locale, format_message, messages};
use lexer::Scanner;
use parser::{Parser, Program, Stmt};
use compiler::Compiler;
use vm::VM;
use typechecker::{TypeChecker, Monomorphizer, CompileContext};
use package::{ProjectConfig, find_project_root, compute_expected_package, PackageResolver, ImportKind};

/// 解析单个源文件
fn parse_source(source: &str, locale: Locale) -> Result<Program, String> {
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
    parser.parse().map_err(|errors| {
        errors
            .iter()
            .map(|e| format!("[{}:{}] {}", e.span.line, e.span.column, e.message))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

/// 加载依赖文件并合并 AST
fn load_dependencies(
    main_program: &Program,
    main_file: &Path,
    project: Option<&ProjectConfig>,
    locale: Locale,
) -> Result<Vec<Stmt>, String> {
    let mut all_statements: Vec<Stmt> = Vec::new();
    let mut loaded_files: HashSet<PathBuf> = HashSet::new();
    
    // 标记主文件已加载
    if let Ok(abs_path) = fs::canonicalize(main_file) {
        loaded_files.insert(abs_path);
    }
    
    // 创建包解析器
    let resolver = PackageResolver::new(project.cloned());
    
    // 处理主程序的 imports
    for import in &main_program.imports {
        match resolver.resolve(import) {
            Ok(resolved) => {
                match resolved.kind {
                    ImportKind::StdBuiltin => {
                        // 内置标准库，不需要加载源文件
                        // 类型和函数由 VM 内置提供
                    }
                    ImportKind::StdSource => {
                        // Q 语言标准库源文件
                        if let Some(source_path) = &resolved.source_path {
                            if source_path.exists() {
                                load_source_file(
                                    source_path,
                                    &mut all_statements,
                                    &mut loaded_files,
                                    project,
                                    locale,
                                    &resolver,
                                )?;
                            }
                        }
                    }
                    ImportKind::Project => {
                        // 项目内部包
                        if let Some(source_path) = &resolved.source_path {
                            // 检查是文件还是目录
                            if source_path.is_file() {
                                load_source_file(
                                    source_path,
                                    &mut all_statements,
                                    &mut loaded_files,
                                    project,
                                    locale,
                                    &resolver,
                                )?;
                            } else if source_path.is_dir() {
                                // 加载目录下所有 .q 文件
                                load_directory(
                                    source_path,
                                    &mut all_statements,
                                    &mut loaded_files,
                                    project,
                                    locale,
                                    &resolver,
                                )?;
                            } else {
                                // 尝试作为包路径解析
                                // import com.test.demo.models.User -> models/User.q 或 models.q 中的 User
                                let parent = source_path.parent().unwrap_or(source_path);
                                if parent.is_dir() {
                                    // 尝试加载目录下的所有文件
                                    load_directory(
                                        parent,
                                        &mut all_statements,
                                        &mut loaded_files,
                                        project,
                                        locale,
                                        &resolver,
                                    )?;
                                }
                            }
                        }
                    }
                    ImportKind::External => {
                        // 外部依赖，暂不支持
                        return Err(format!("外部依赖暂不支持: {}", import.path));
                    }
                }
            }
            Err(e) => {
                // 导入解析失败，尝试智能查找
                // 例如: import com.test.demo.models.User
                // 可能是 models/function.q 中的 User 类
                if let Some(proj) = project {
                    if let Some(found_path) = find_import_source(import, proj) {
                        load_source_file(
                            &found_path,
                            &mut all_statements,
                            &mut loaded_files,
                            project,
                            locale,
                            &resolver,
                        )?;
                    } else {
                        return Err(e);
                    }
                } else {
                    return Err(e);
                }
            }
        }
    }
    
    Ok(all_statements)
}

/// 智能查找导入源文件
fn find_import_source(import: &parser::ast::ImportDecl, project: &ProjectConfig) -> Option<PathBuf> {
    use parser::ast::ImportTarget;
    
    // 获取导入路径的各部分
    let full_path = match &import.target {
        ImportTarget::Single(name) => format!("{}.{}", import.path, name),
        _ => import.path.clone(),
    };
    
    // 去除项目包前缀
    let relative_path = full_path.strip_prefix(&project.package)
        .and_then(|s| s.strip_prefix('.'))
        .unwrap_or(&full_path);
    
    // 将点分隔的路径转换为目录结构
    let parts: Vec<&str> = relative_path.split('.').collect();
    if parts.is_empty() {
        return None;
    }
    
    let src_dir = project.root_dir.join(&project.src_dir);
    
    // 尝试不同的文件位置
    // 1. 直接作为文件: models/User.q
    // 2. 作为目录下的文件: models/function.q (包含 User)
    
    // 策略 1: 尝试 parent_path/*.q 加载整个包
    if parts.len() >= 1 {
        let dir_path: PathBuf = parts[..parts.len()-1].iter().collect();
        let full_dir = src_dir.join(&dir_path);
        if full_dir.is_dir() {
            // 找到目录下的 .q 文件
            if let Ok(entries) = fs::read_dir(&full_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == SOURCE_EXTENSION).unwrap_or(false) {
                        return Some(path);
                    }
                }
            }
        }
    }
    
    None
}

/// 加载单个源文件
fn load_source_file(
    path: &Path,
    all_statements: &mut Vec<Stmt>,
    loaded_files: &mut HashSet<PathBuf>,
    project: Option<&ProjectConfig>,
    locale: Locale,
    resolver: &PackageResolver,
) -> Result<(), String> {
    let abs_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    
    // 检查是否已加载
    if loaded_files.contains(&abs_path) {
        return Ok(());
    }
    loaded_files.insert(abs_path.clone());
    
    // 读取文件
    let source = fs::read_to_string(path)
        .map_err(|e| format!("无法读取文件 {}: {}", display_path(path), e))?;
    
    // 解析
    let program = parse_source(&source, locale)
        .map_err(|e| format!("解析 {} 失败:\n{}", display_path(path), e))?;
    
    // 递归加载依赖
    for import in &program.imports {
        if let Ok(resolved) = resolver.resolve(import) {
            if let Some(source_path) = &resolved.source_path {
                if resolved.kind == ImportKind::Project || resolved.kind == ImportKind::StdSource {
                    if source_path.is_file() {
                        load_source_file(source_path, all_statements, loaded_files, project, locale, resolver)?;
                    } else if source_path.is_dir() {
                        load_directory(source_path, all_statements, loaded_files, project, locale, resolver)?;
                    } else if let Some(parent) = source_path.parent() {
                        if parent.is_dir() {
                            load_directory(parent, all_statements, loaded_files, project, locale, resolver)?;
                        }
                    }
                }
            }
        }
    }
    
    // 添加语句（排除 package 和 import，只要类型和函数定义）
    for stmt in program.statements {
        match &stmt {
            Stmt::Package { .. } | Stmt::Import { .. } => {
                // 跳过 package 和 import 声明
            }
            _ => {
                all_statements.push(stmt);
            }
        }
    }
    
    Ok(())
}

/// 加载目录下所有源文件
fn load_directory(
    dir: &Path,
    all_statements: &mut Vec<Stmt>,
    loaded_files: &mut HashSet<PathBuf>,
    project: Option<&ProjectConfig>,
    locale: Locale,
    resolver: &PackageResolver,
) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("无法读取目录 {:?}: {}", dir, e))?;
    
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().map(|e| e == SOURCE_EXTENSION).unwrap_or(false) {
            load_source_file(&path, all_statements, loaded_files, project, locale, resolver)?;
        }
    }
    
    Ok(())
}

/// 运行源代码（独立文件模式，用于 REPL）
fn run(source: &str, locale: Locale) -> Result<(), String> {
    // REPL 模式下不检查 main 函数和顶级代码限制
    run_with_context(source, locale, CompileContext::default(), false, None, None)
}

/// 运行源代码（带上下文）
fn run_with_context(
    source: &str, 
    locale: Locale, 
    context: CompileContext, 
    type_check: bool,
    extra_statements: Option<Vec<Stmt>>,
    main_file: Option<&Path>,
) -> Result<(), String> {
    // 解析主程序
    let mut program = parse_source(source, locale)
        .map_err(|e| format!("[语法错误/Syntax Error]\n{}", e))?;
    
    // 如果有额外的语句（来自依赖），添加到程序开头
    if let Some(mut extra) = extra_statements {
        // 将依赖的语句放在主程序语句之前
        extra.append(&mut program.statements);
        program.statements = extra;
    }
    
    // 类型检查（可选）
    if type_check {
        let mut type_checker = TypeChecker::with_context(context);
        type_checker.check_program(&program).map_err(|errors| {
            let error_list = errors
                .iter()
                .map(|e| format!("  [{}:{}] {}", e.span.line, e.span.column, e))
                .collect::<Vec<_>>()
                .join("\n");
            format!("[类型检查错误/Type Error]\n{}", error_list)
        })?;
        
        // 收集泛型定义用于单态化
        let mut monomorphizer = Monomorphizer::new();
        monomorphizer.collect_definitions(&program);
        
        // 处理所有待单态化的请求
        monomorphizer.process_all();
    }
    
    // 编译
    let mut compiler = Compiler::new(locale);
    let chunk = compiler.compile(&program).map_err(|errors| {
        let error_list = errors
            .iter()
            .map(|e| format!("  [{}:{}] {}", e.span.line, e.span.column, e.message))
            .collect::<Vec<_>>()
            .join("\n");
        format!("[编译错误/Compile Error]\n{}", error_list)
    })?;
    
    // 执行（从 main 函数开始）
    let chunk_arc = std::sync::Arc::new(chunk);
    let mut vm = VM::new(chunk_arc, locale);
    vm.run().map_err(|e| format!("[运行时错误/Runtime Error]\n  [line {}] {}", e.line, e.message))?;
    
    Ok(())
}

/// 构建编译上下文
fn build_compile_context(file_path: &Path) -> CompileContext {
    build_compile_context_with_project(file_path).0
}

/// 构建编译上下文（同时返回项目配置）
fn build_compile_context_with_project(file_path: &Path) -> (CompileContext, Option<ProjectConfig>) {
    // 获取文件的绝对路径
    let abs_path = fs::canonicalize(file_path).unwrap_or_else(|_| file_path.to_path_buf());
    
    // 尝试查找 project.toml
    if let Some(project_root) = find_project_root(&abs_path) {
        let project_file = project_root.join(PROJECT_FILE);
        if let Ok(project) = ProjectConfig::load(&project_file) {
            // 计算期望包名
            let expected_package = compute_expected_package(&project, &abs_path);
            
            let context = CompileContext {
                is_entry_file: true,
                expected_package,
                standalone_mode: false,
            };
            return (context, Some(project));
        }
    }
    
    // 独立文件模式
    let context = CompileContext {
        is_entry_file: true,
        expected_package: None,
        standalone_mode: true,
    };
    (context, None)
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
    
    // 构建编译上下文
    let file_path = Path::new(path);
    let (context, project) = build_compile_context_with_project(file_path);
    
    // 先解析主程序以获取 imports
    let main_program = match parse_source(&source, locale) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[语法错误/Syntax Error] {}\n{}", path, e);
            process::exit(1);
        }
    };
    
    // 加载所有依赖
    let extra_statements = match load_dependencies(&main_program, file_path, project.as_ref(), locale) {
        Ok(stmts) => Some(stmts),
        Err(e) => {
            eprintln!("[导入错误/Import Error]\n  {}", e);
            process::exit(1);
        }
    };
    
    if let Err(e) = run_with_context(&source, locale, context, true, extra_statements, Some(file_path)) {
        eprintln!("{}", e);
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
