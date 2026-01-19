//! 代码生成器
//! 
//! 将 AST 编译为字节码

use std::sync::Arc;
use parking_lot::Mutex;
use crate::parser::{Expr, Stmt, Program, BinOp, UnaryOp};
use crate::vm::{Value, value::Function};
use crate::i18n::Locale;
use crate::lexer::Span;
use crate::types::Type;
use super::bytecode::{Chunk, OpCode};
use super::symbol::SymbolTable;

/// 编译错误
#[derive(Debug, Clone)]
pub struct CompileError {
    pub message: String,
    pub span: Span,
}

impl CompileError {
    fn new(message: String, span: Span) -> Self {
        Self { message, span }
    }
}

/// 尾调用信息（用于尾调用优化）
struct TailCallInfo {
    /// 被调用的函数表达式
    callee: Expr,
    /// 调用参数
    args: Vec<Expr>,
}

/// 字节码编译器
/// 循环信息（用于 break/continue）
struct LoopInfo {
    /// 循环起始位置
    start: usize,
    /// break 跳转位置列表（用于回填）
    breaks: Vec<usize>,
    /// 循环标签（可选）
    label: Option<String>,
}

pub struct Compiler {
    /// 当前字节码块
    chunk: Chunk,
    /// 错误列表
    errors: Vec<CompileError>,
    /// 当前语言
    #[allow(dead_code)]
    locale: Locale,
    /// 符号表
    symbols: SymbolTable,
    /// 循环起始位置栈（用于 break/continue）
    loop_starts: Vec<usize>,
    /// break 跳转位置栈（用于回填）
    break_jumps: Vec<Vec<usize>>,
    /// 类型别名表
    type_aliases: std::collections::HashMap<String, Type>,
    /// 循环信息栈（支持带标签的 break/continue）
    loop_stack: Vec<LoopInfo>,
}

/// 简单的静态类型（用于优化）
#[derive(Debug, Clone, Copy, PartialEq)]
enum StaticType {
    Int,
    Float,
    Bool,
    String,
    Unknown,
}

impl Compiler {
    /// 创建新的编译器
    pub fn new(locale: Locale) -> Self {
        Self {
            chunk: Chunk::new(),
            errors: Vec::new(),
            locale,
            symbols: SymbolTable::new(),
            loop_starts: Vec::new(),
            break_jumps: Vec::new(),
            type_aliases: std::collections::HashMap::new(),
            loop_stack: Vec::new(),
        }
    }
    
    /// 推断表达式的静态类型（用于优化）
    fn infer_type(&self, expr: &Expr) -> StaticType {
        match expr {
            Expr::Integer { .. } => StaticType::Int,
            Expr::Float { .. } => StaticType::Float,
            Expr::Bool { .. } => StaticType::Bool,
            Expr::String { .. } | Expr::StringInterpolation { .. } => StaticType::String,
            Expr::Identifier { name, .. } => {
                // 检查符号表中的类型
                if let Some(symbol) = self.symbols.resolve(name) {
                    match &symbol.ty {
                        Type::Int | Type::I8 | Type::I16 | Type::I32 | Type::I64 => StaticType::Int,
                        Type::F32 | Type::F64 => StaticType::Float,
                        Type::Bool => StaticType::Bool,
                        Type::String => StaticType::String,
                        _ => StaticType::Unknown,
                    }
                } else {
                    StaticType::Unknown
                }
            }
            Expr::Binary { left, op, right, .. } => {
                let left_type = self.infer_type(left);
                let right_type = self.infer_type(right);
                
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::Pow => {
                        if left_type == StaticType::Int && right_type == StaticType::Int {
                            StaticType::Int
                        } else if left_type == StaticType::Float || right_type == StaticType::Float {
                            StaticType::Float
                        } else {
                            StaticType::Unknown
                        }
                    }
                    BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::Eq | BinOp::Ne | BinOp::And | BinOp::Or => {
                        StaticType::Bool
                    }
                    BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
                        if left_type == StaticType::Int && right_type == StaticType::Int {
                            StaticType::Int
                        } else {
                            StaticType::Unknown
                        }
                    }
                }
            }
            Expr::Unary { op, operand, .. } => {
                match op {
                    UnaryOp::Neg => {
                        let inner_type = self.infer_type(operand);
                        if inner_type == StaticType::Int || inner_type == StaticType::Float {
                            inner_type
                        } else {
                            StaticType::Unknown
                        }
                    }
                    UnaryOp::Not => StaticType::Bool,
                    UnaryOp::BitNot => {
                        if self.infer_type(operand) == StaticType::Int {
                            StaticType::Int
                        } else {
                            StaticType::Unknown
                        }
                    }
                }
            }
            Expr::Grouping { expr, .. } => self.infer_type(expr),
            Expr::Call { .. } => StaticType::Unknown, // 函数返回类型未知
            _ => StaticType::Unknown,
        }
    }

    /// 是否可用无类型检查的整数指令
    fn is_fast_int_type(&self, ty: &Type) -> bool {
        match ty {
            Type::Alias { actual_type, .. } => actual_type.is_integer(),
            _ => ty.is_integer(),
        }
    }

    /// 编译程序
    pub fn compile(&mut self, program: &Program) -> Result<Chunk, Vec<CompileError>> {
        // 第一遍：预注册所有函数名（使前向引用成为可能）
        // 这允许 main 函数调用在它之后定义的函数
        for stmt in &program.statements {
            if let Stmt::FnDef { name, .. } = stmt {
                // 预留常量池位置
                let func_index = self.chunk.constants.len() as u16;
                self.chunk.constants.push(Value::null());
                // 预注册函数名
                self.chunk.register_named_function(name.clone(), func_index);
            }
        }
        
        // 第二遍：实际编译所有语句
        for stmt in &program.statements {
            self.compile_stmt(stmt);
        }
        
        // 如果有 main 函数，生成调用 main 函数的代码
        if let Some(main_index) = self.chunk.get_named_function("main") {
            // 从常量池加载 main 函数
            self.chunk.write_op(OpCode::Const, 0);
            self.chunk.write_u16(main_index, 0);
            // 调用 main 函数（无参数）
            self.chunk.write_op(OpCode::Call, 0);
            self.chunk.write(0, 0);
            // 弹出返回值（main 函数返回 void）
            self.chunk.write_op(OpCode::Pop, 0);
        }
        
        // 添加 HALT 指令
        self.chunk.write_op(OpCode::Halt, 0);
        
        if self.errors.is_empty() {
            Ok(std::mem::take(&mut self.chunk))
        } else {
            Err(self.errors.clone())
        }
    }

    /// 编译语句
    fn compile_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Expression { expr, .. } => {
                self.compile_expr(expr);
                // 表达式语句的结果被丢弃
                self.chunk.write_op(OpCode::Pop, expr.span().line);
            }
            Stmt::Print { expr, newline, span } => {
                self.compile_expr(expr);
                if *newline {
                    self.chunk.write_op(OpCode::PrintLn, span.line);
                } else {
                    self.chunk.write_op(OpCode::Print, span.line);
                }
            }
            Stmt::VarDecl { name, type_ann, initializer, span } => {
                // 编译初始化表达式
                if let Some(init) = initializer {
                    self.compile_expr(init);
                } else {
                    // 无初始值，压入 null
                    self.chunk.write_constant(Value::null(), span.line);
                }
                
                // 推断类型
                let ty = if let Some(ann) = type_ann {
                    ann.ty.clone()
                } else {
                    Type::Infer // TODO: 实际类型推导
                };
                
                // 定义变量
                match self.symbols.define(name.clone(), ty, false) {
                    Ok(_slot) => {
                        // 变量值已在栈上，不需要额外操作
                    }
                    Err(msg) => {
                        self.errors.push(CompileError::new(msg, *span));
                    }
                }
            }
            Stmt::ConstDecl { name, type_ann, initializer, span } => {
                // 编译初始化表达式
                self.compile_expr(initializer);
                
                // 推断类型
                let ty = if let Some(ann) = type_ann {
                    ann.ty.clone()
                } else {
                    Type::Infer // TODO: 实际类型推导
                };
                
                // 定义常量
                match self.symbols.define(name.clone(), ty, true) {
                    Ok(_slot) => {
                        // 常量值已在栈上，不需要额外操作
                    }
                    Err(msg) => {
                        self.errors.push(CompileError::new(msg, *span));
                    }
                }
            }
            Stmt::Block { statements, span: _ } => {
                self.symbols.begin_scope();
                for s in statements {
                    self.compile_stmt(s);
                }
                let pop_count = self.symbols.end_scope();
                // 弹出作用域内的局部变量
                for _ in 0..pop_count {
                    self.chunk.write_op(OpCode::Pop, 0);
                }
            }
            Stmt::If { condition, then_branch, else_branch, span } => {
                // 尝试使用超级指令优化：检查是否是 `local <= int_const` 形式
                // 编译条件并跳转
                self.compile_expr(condition);
                let then_jump = self.chunk.write_jump_if_false_pop(span.line);
                
                // 编译 then 分支
                self.compile_stmt(then_branch);
                
                if let Some(else_branch) = else_branch {
                    // 跳过 else 分支
                    let else_jump = self.chunk.write_jump(OpCode::Jump, span.line);
                    
                    // 回填 then_jump
                    self.chunk.patch_jump(then_jump);
                    
                    // 编译 else 分支
                    self.compile_stmt(else_branch);
                    
                    // 回填 else_jump
                    self.chunk.patch_jump(else_jump);
                } else {
                    // 回填 then_jump
                    self.chunk.patch_jump(then_jump);
                    
                    // 条件值已在 JumpIfFalsePop 中弹出（或由超级指令处理）
                }
            }
            Stmt::ForLoop { label, initializer, condition, increment, body, span } => {
                // C 风格 for 循环: for init; cond; post { body }
                // 编译为:
                //   init
                //   loop_start:
                //   if !cond goto loop_end
                //   body
                //   post
                //   goto loop_start
                //   loop_end:
                
                // 为整个 for 循环创建一个作用域（初始化变量在循环结束后不可访问）
                self.symbols.begin_scope();
                
                // 1. 编译初始化部分
                if let Some(init) = initializer {
                    self.compile_stmt(init);
                }
                
                // 2. 记录循环起始位置
                let loop_start = self.chunk.current_offset();
                
                // 推入循环信息（支持带标签的 break/continue）
                self.loop_stack.push(LoopInfo {
                    start: loop_start,
                    breaks: Vec::new(),
                    label: label.clone(),
                });
                
                // 3. 编译条件检查
                let exit_jump = if let Some(cond) = condition {
                    self.compile_expr(cond);
                    let jump = self.chunk.write_jump_if_false_pop(span.line);
                    Some(jump)
                } else {
                    None
                };
                
                // 4. 编译循环体
                self.compile_stmt(body);
                
                // 5. 编译递增部分
                if let Some(incr) = increment {
                    self.compile_expr(incr);
                    self.chunk.write_op(OpCode::Pop, span.line); // 丢弃递增表达式的值
                }
                
                // 6. 跳回循环开始
                self.chunk.write_loop(loop_start, span.line);
                
                // 7. 回填退出跳转
                if let Some(exit) = exit_jump {
                    self.chunk.patch_jump(exit);
                }
                
                // 8. 回填所有 break 跳转
                let loop_info = self.loop_stack.pop().unwrap();
                for break_jump in loop_info.breaks {
                    self.chunk.patch_jump(break_jump);
                }
                
                // 9. 结束 for 循环作用域
                let pop_count = self.symbols.end_scope();
                for _ in 0..pop_count {
                    self.chunk.write_op(OpCode::Pop, span.line);
                }
            }
            Stmt::ForIn { label, variables, iterable, body, span } => {
                // TODO: 实现标签跳转
                let _ = label;
                // for-in 循环编译：
                // for item in collection { body }
                // 
                // 栈布局（每个变量占一个槽位）：
                // [..., iterator, loop_var]
                //
                // 编译为：
                // 1. 编译 collection
                // 2. IterInit (创建迭代器，替换栈顶)
                // 3. 定义迭代器变量（栈上已有迭代器）
                // 4. 压入循环变量的初始值（null）
                // 5. 定义循环变量
                // 6. loop_start:
                // 7.   GetLocal iterator
                // 8.   IterNext -> 栈: [..., iter, loop_var, iter_copy, value, has_next]
                // 9.   JumpIfFalse exit
                // 10.  Pop (弹出 has_next)
                // 11.  SetLocal loop_var (更新循环变量)
                // 12.  Pop (弹出 value)
                // 13.  Pop (弹出 iter_copy)
                // 14.  body
                // 15.  Loop loop_start
                // 16. exit:
                // 17.   Pop (弹出 has_next)
                // 18.   Pop (弹出 value/null)
                // 19.   Pop (弹出 iter_copy)
                // 20. end_scope (弹出 iterator 和 loop_var)
                
                // 开始 for-in 作用域
                self.symbols.begin_scope();
                
                // 编译可迭代对象
                self.compile_expr(iterable);
                
                // 创建迭代器（替换栈顶的 collection）
                self.chunk.write_op(OpCode::IterInit, span.line);
                
                // 定义迭代器变量（值已在栈顶）
                let iter_slot = match self.symbols.define(
                    format!("__iter_{}__", span.line),
                    crate::types::Type::Unknown,
                    false,
                ) {
                    Ok(slot) => slot,
                    Err(msg) => {
                        self.errors.push(CompileError::new(msg, *span));
                        return;
                    }
                };
                
                // 定义循环变量（先压入 null 作为初始值）
                let loop_var_slot = if variables.len() >= 1 {
                    self.chunk.write_constant(Value::null(), span.line);
                    match self.symbols.define(
                        variables[0].clone(),
                        crate::types::Type::Unknown,
                        false,
                    ) {
                        Ok(slot) => slot,
                        Err(msg) => {
                            self.errors.push(CompileError::new(msg, *span));
                            return;
                        }
                    }
                } else {
                    let msg = "For-in loop requires at least one variable".to_string();
                    self.errors.push(CompileError::new(msg, *span));
                    return;
                };
                
                // 记录循环起始位置
                let loop_start = self.chunk.current_offset();
                
                // 推入循环信息（支持带标签的 break/continue）
                self.loop_stack.push(LoopInfo {
                    start: loop_start,
                    breaks: Vec::new(),
                    label: label.clone(),
                });
                
                // 获取迭代器（复制到栈顶用于 IterNext）
                self.chunk.write_get_local(iter_slot, span.line);
                self.chunk.write_op(OpCode::IterNext, span.line);
                // 栈: [..., iter, loop_var, iter_copy, value, has_next]
                
                // 检查是否有更多元素
                let exit_jump = self.chunk.write_jump(OpCode::JumpIfFalse, span.line);
                self.chunk.write_op(OpCode::Pop, span.line); // 弹出 has_next
                // 栈: [..., iter, loop_var, iter_copy, value]
                
                // 更新循环变量
                self.chunk.write_set_local(loop_var_slot, span.line);
                self.chunk.write_op(OpCode::Pop, span.line); // 弹出 value
                self.chunk.write_op(OpCode::Pop, span.line); // 弹出 iter_copy
                // 栈: [..., iter, loop_var]
                
                // 编译循环体
                self.compile_stmt(body);
                
                // 跳回循环开始
                self.chunk.write_loop(loop_start, span.line);
                
                // 回填退出跳转
                self.chunk.patch_jump(exit_jump);
                // 退出时栈: [..., iter, loop_var, iter_copy, null, false]
                self.chunk.write_op(OpCode::Pop, span.line); // 弹出 has_next (false)
                self.chunk.write_op(OpCode::Pop, span.line); // 弹出 value (null)
                self.chunk.write_op(OpCode::Pop, span.line); // 弹出 iter_copy
                
                // 处理 break 跳转
                let loop_info = self.loop_stack.pop().unwrap();
                for break_jump in loop_info.breaks {
                        self.chunk.patch_jump(break_jump);
                    }
                
                // 结束 for-in 作用域（弹出 iterator 和 loop_var）
                let pop_count = self.symbols.end_scope();
                for _ in 0..pop_count {
                    self.chunk.write_op(OpCode::Pop, span.line);
                }
            }
            Stmt::While { label, condition, body, span } => {
                // 记录循环起始位置
                let loop_start = self.chunk.current_offset();
                
                // 推入循环信息（支持带标签的 break/continue）
                self.loop_stack.push(LoopInfo {
                    start: loop_start,
                    breaks: Vec::new(),
                    label: label.clone(),
                });
                
                // 编译条件（如果有）
                let exit_jump = if let Some(cond) = condition {
                    self.compile_expr(cond);
                    let jump = self.chunk.write_jump_if_false_pop(span.line);
                    Some(jump)
                } else {
                    None
                };
                
                // 编译循环体
                self.compile_stmt(body);
                
                // 跳回循环开始
                self.chunk.write_loop(loop_start, span.line);
                
                // 回填退出跳转
                if let Some(exit) = exit_jump {
                    self.chunk.patch_jump(exit);
                    self.chunk.write_op(OpCode::Pop, span.line);
                }
                
                // 回填所有 break 跳转
                let loop_info = self.loop_stack.pop().unwrap();
                for break_jump in loop_info.breaks {
                    self.chunk.patch_jump(break_jump);
                }
            }
            Stmt::Break { label, span } => {
                if self.loop_stack.is_empty() && self.loop_starts.is_empty() {
                    let msg = "'break' outside of loop".to_string();
                    self.errors.push(CompileError::new(msg, *span));
                } else if let Some(target_label) = label {
                    // 带标签的 break - 查找匹配的循环
                    let idx = self.loop_stack.iter().rposition(|info| {
                        info.label.as_ref() == Some(target_label)
                    });
                    if let Some(idx) = idx {
                        let jump = self.chunk.write_jump(OpCode::Jump, span.line);
                        self.loop_stack[idx].breaks.push(jump);
                } else {
                        let msg = format!("Cannot find loop with label '{}'", target_label);
                        self.errors.push(CompileError::new(msg, *span));
                    }
                } else {
                    // 无标签的 break - 跳出最近的循环
                    let jump = self.chunk.write_jump(OpCode::Jump, span.line);
                    if let Some(info) = self.loop_stack.last_mut() {
                        info.breaks.push(jump);
                    } else if let Some(breaks) = self.break_jumps.last_mut() {
                        breaks.push(jump);
                    }
                }
            }
            Stmt::Continue { label, span } => {
                if self.loop_stack.is_empty() && self.loop_starts.is_empty() {
                    let msg = "'continue' outside of loop".to_string();
                    self.errors.push(CompileError::new(msg, *span));
                } else if let Some(target_label) = label {
                    // 带标签的 continue - 查找匹配的循环
                    let info = self.loop_stack.iter().rev().find(|info| {
                        info.label.as_ref() == Some(target_label)
                    });
                    if let Some(info) = info {
                        self.chunk.write_loop(info.start, span.line);
                    } else {
                        let msg = format!("Cannot find loop with label '{}'", target_label);
                        self.errors.push(CompileError::new(msg, *span));
                    }
                } else {
                    // 无标签的 continue - 回到最近的循环开始
                    if let Some(info) = self.loop_stack.last() {
                        self.chunk.write_loop(info.start, span.line);
                } else {
                    let loop_start = *self.loop_starts.last().unwrap();
                    self.chunk.write_loop(loop_start, span.line);
                    }
                }
            }
            Stmt::Return { value, span } => {
                // 尾调用优化：如果返回值是函数调用，使用 TailCall 指令
                if let Some(expr) = value {
                    if let Some(tail_call_info) = self.try_extract_tail_call(expr) {
                        // 这是一个尾调用，使用 TailCall 指令
                        // 1. 编译函数表达式
                        self.compile_expr(&tail_call_info.callee);
                        
                        // 2. 编译参数
                        for arg in &tail_call_info.args {
                            self.compile_expr(arg);
                        }
                        
                        // 3. 写入 TailCall 指令
                        self.chunk.write_op(OpCode::TailCall, span.line);
                        self.chunk.write(tail_call_info.args.len() as u8, span.line);
                    } else if let Expr::Identifier { name, .. } = expr {
                        // 超级指令优化：返回局部变量
                        if let Some(slot) = self.symbols.resolve_slot(name) {
                            if slot <= 255 {
                                self.chunk.write_return_local(slot as u8, span.line);
                            } else {
                                self.compile_expr(expr);
                                self.chunk.write_op(OpCode::Return, span.line);
                            }
                        } else {
                            self.compile_expr(expr);
                            self.chunk.write_op(OpCode::Return, span.line);
                        }
                    } else if let Expr::Integer { value: int_val, .. } = expr {
                        // 超级指令优化：返回小整数常量
                        if *int_val >= i8::MIN as i64 && *int_val <= i8::MAX as i64 {
                            self.chunk.write_return_int(*int_val as i8, span.line);
                        } else {
                            self.compile_expr(expr);
                            self.chunk.write_op(OpCode::Return, span.line);
                        }
                    } else {
                        // 普通返回
                        self.compile_expr(expr);
                        self.chunk.write_op(OpCode::Return, span.line);
                    }
                } else {
                    // 无返回值时返回 null
                    self.chunk.write_constant(Value::null(), span.line);
                    self.chunk.write_op(OpCode::Return, span.line);
                }
            }
            Stmt::Match { expr, arms, span } => {
                use crate::parser::ast::MatchPattern;
                
                // match 语句编译：
                // 1. 计算被匹配的表达式，存入临时变量
                // 2. 对每个分支：
                //    a. 获取临时变量值
                //    b. 检查模式是否匹配
                //    c. 如果不匹配，跳转到下一个分支
                //    d. 如果匹配，执行分支体，然后跳转到结束
                // 3. 弹出临时变量
                
                // 开始 match 作用域
                self.symbols.begin_scope();
                
                // 编译被匹配的表达式并存入临时变量
                self.compile_expr(expr);
                let match_slot = match self.symbols.define(
                    format!("__match_{}__", span.line),
                    crate::types::Type::Unknown,
                    false,
                ) {
                    Ok(slot) => slot,
                    Err(msg) => {
                        self.errors.push(CompileError::new(msg, *span));
                        return;
                    }
                };
                // match_value 已在栈上正确位置
                
                let mut end_jumps = Vec::new();
                
                for (_idx, arm) in arms.iter().enumerate() {
                    let _is_last = _idx == arms.len() - 1;
                    
                    let next_arm_jump = match &arm.pattern {
                        MatchPattern::Literal(lit_expr) => {
                            // 获取 match_value
                            self.chunk.write_get_local(match_slot, span.line);
                            // 编译模式字面量
                            self.compile_expr(lit_expr);
                            // 比较
                            self.chunk.write_op(OpCode::Eq, span.line);
                            // 栈: [..., is_equal]
                            
                            let jump = self.chunk.write_jump(OpCode::JumpIfFalse, span.line);
                            self.chunk.write_op(OpCode::Pop, span.line); // 弹出 true
                            Some(jump)
                        }
                        MatchPattern::Wildcard => {
                            // 通配符总是匹配
                            None
                        }
                        MatchPattern::Variable(var_name) => {
                            // 变量绑定：将 match_value 绑定到变量
                            // 获取 match_value 并定义为新变量
                            self.chunk.write_get_local(match_slot, span.line);
                            match self.symbols.define(
                                var_name.clone(),
                                crate::types::Type::Unknown,
                                false,
                            ) {
                                Ok(_) => {}
                                Err(msg) => {
                                    self.errors.push(CompileError::new(msg, *span));
                                }
                            }
                            // 总是匹配
                            None
                        }
                        MatchPattern::Or(_) => {
                            let msg = "Or pattern not yet implemented".to_string();
                            self.errors.push(CompileError::new(msg, *span));
                            continue;
                        }
                        MatchPattern::Range { start, end, inclusive } => {
                            // 范围模式：match_value >= start && match_value < end（或 <= end）
                            // 需要两个条件都满足才执行分支体
                            
                            // 获取 match_value 并与 start 比较
                            self.chunk.write_get_local(match_slot, span.line);
                            self.compile_expr(start);
                            self.chunk.write_op(OpCode::Ge, span.line); // match >= start
                            
                            // 如果 match < start，跳到下一个分支
                            let fail_jump1 = self.chunk.write_jump(OpCode::JumpIfFalse, span.line);
                            self.chunk.write_op(OpCode::Pop, span.line); // 弹出 true (match >= start)
                            
                            // 获取 match_value 并与 end 比较
                            self.chunk.write_get_local(match_slot, span.line);
                            self.compile_expr(end);
                            if *inclusive {
                                self.chunk.write_op(OpCode::Le, span.line); // match <= end
                            } else {
                                self.chunk.write_op(OpCode::Lt, span.line); // match < end
                            }
                            
                            // 如果 match >= end (或 > end)，跳到下一个分支
                            let fail_jump2 = self.chunk.write_jump(OpCode::JumpIfFalse, span.line);
                            self.chunk.write_op(OpCode::Pop, span.line); // 弹出 true (match < end)
                            
                            // 执行分支体
                            self.compile_stmt(&arm.body);
                            
                            // 跳转到 match 结束
                            let end_jump = self.chunk.write_jump(OpCode::Jump, span.line);
                            end_jumps.push(end_jump);
                            
                            // 回填两个失败跳转：都跳到这里弹出 false 然后继续下一个分支
                            self.chunk.patch_jump(fail_jump1);
                            self.chunk.write_op(OpCode::Pop, span.line); // 弹出 false (match < start)
                            // fail_jump2 跳到的地方
                            self.chunk.patch_jump(fail_jump2);
                            self.chunk.write_op(OpCode::Pop, span.line); // 弹出 false (match >= end)
                            
                            // 范围模式已经处理了分支体和跳转，继续下一个分支
                            continue;
                        }
                        MatchPattern::Type { .. } => {
                            let msg = "Type pattern not yet implemented".to_string();
                            self.errors.push(CompileError::new(msg, *span));
                            continue;
                        }
                    };
                    
                    // 执行分支体
                    self.compile_stmt(&arm.body);
                    
                    // 跳转到 match 结束（所有分支都需要，包括最后一个）
                    let end_jump = self.chunk.write_jump(OpCode::Jump, span.line);
                    end_jumps.push(end_jump);
                    
                    // 回填跳转到下一个分支（跳转到这里意味着匹配失败）
                    if let Some(jump) = next_arm_jump {
                        self.chunk.patch_jump(jump);
                        self.chunk.write_op(OpCode::Pop, span.line); // 弹出 false
                    }
                }
                
                // 回填所有结束跳转
                for end_jump in end_jumps {
                    self.chunk.patch_jump(end_jump);
                }
                
                // 结束 match 作用域（弹出 match_value 临时变量）
                let pop_count = self.symbols.end_scope();
                for _ in 0..pop_count {
                    self.chunk.write_op(OpCode::Pop, span.line);
                }
            }
            Stmt::StructDef { name, type_params: _, where_clauses: _, interfaces, fields: _, methods, span } => {
                // 注册 struct 类型
                self.chunk.register_type(name.clone());
                
                // 收集已定义的方法名
                let defined_methods: std::collections::HashSet<String> = methods.iter()
                    .map(|m| m.name.clone())
                    .collect();
                
                // 检查接口实现
                for interface_name in interfaces {
                    if let Some(interface_info) = self.chunk.get_interface(interface_name).cloned() {
                        // 检查 struct 是否实现了接口的所有方法
                        for interface_method in &interface_info.methods {
                            if !defined_methods.contains(&interface_method.name) {
                                let msg = format!(
                                    "Struct '{}' does not implement method '{}' required by interface '{}'",
                                    name, interface_method.name, interface_name
                                );
                                self.errors.push(CompileError::new(msg, *span));
                            }
                        }
                    } else {
                        let msg = format!("Unknown interface '{}'", interface_name);
                        self.errors.push(CompileError::new(msg, *span));
                    }
                }
                
                // 编译每个方法
                for method in methods {
                    self.compile_struct_method(name, method, *span);
                }
            }
            Stmt::ClassDef { name, type_params: _, where_clauses: _, is_abstract, parent, interfaces, traits, fields, methods, span } => {
                // 注册 class 类型（包括是否抽象）
                self.chunk.register_class_with_abstract(name.clone(), parent.clone(), *is_abstract);
                
                // 收集类中已定义的方法名（用于避免覆盖）
                let defined_methods: std::collections::HashSet<String> = methods.iter()
                    .map(|m| m.name.clone())
                    .collect();
                
                // 检查接口实现
                for interface_name in interfaces {
                    if let Some(interface_info) = self.chunk.get_interface(interface_name).cloned() {
                        // 检查类是否实现了接口的所有方法
                        for interface_method in &interface_info.methods {
                            if !defined_methods.contains(&interface_method.name) {
                                let msg = format!(
                                    "Class '{}' does not implement method '{}' required by interface '{}'",
                                    name, interface_method.name, interface_name
                                );
                                self.errors.push(CompileError::new(msg, *span));
                            }
                        }
                    } else {
                        let msg = format!("Unknown interface '{}'", interface_name);
                        self.errors.push(CompileError::new(msg, *span));
                    }
                }
                
                // 处理 traits：检查并将 trait 的默认方法复制到 class 中
                for trait_name in traits {
                    if let Some(trait_info) = self.chunk.get_trait(trait_name).cloned() {
                        for trait_method in &trait_info.methods {
                            if !defined_methods.contains(&trait_method.name) {
                                // 如果类没有定义该方法
                                if let Some(func_index) = trait_method.default_impl {
                                    // trait 有默认实现，使用默认实现
                                    self.chunk.register_method(name, trait_method.name.clone(), func_index);
                                } else {
                                    // trait 没有默认实现，且类没有实现该方法，报错
                                    let msg = format!(
                                        "Class '{}' does not implement method '{}' required by trait '{}'",
                                        name, trait_method.name, trait_name
                                    );
                                    self.errors.push(CompileError::new(msg, *span));
                                }
                            }
                        }
                    } else {
                        let msg = format!("Unknown trait '{}'", trait_name);
                        self.errors.push(CompileError::new(msg, *span));
                    }
                }
                
                // 注册字段
                for field in fields {
                    if !field.is_static {
                        self.chunk.register_field(name, field.name.clone());
                    } else {
                        // 静态字段：编译初始值（如果有）并注册
                        if let Some(init) = &field.initializer {
                            // 先跳过初始化代码
                            let jump_over = self.chunk.write_jump(OpCode::Jump, span.line);
                            let value_start = self.chunk.current_offset();
                            
                            self.compile_expr(init);
                            self.chunk.write_op(OpCode::Return, span.line);
                            
                            self.chunk.patch_jump(jump_over);
                            
                            // 创建一个"函数"来计算初始值
                            let init_func = crate::vm::value::Function {
                                name: Some(format!("{}::static_{}", name, field.name)),
                                arity: 0,
                                required_params: 0,
                                defaults: Vec::new(),
                                has_variadic: false,
                                chunk_index: value_start,
                                local_count: 0,
                                upvalues: Vec::new(),
                            };
                            let func_index = self.chunk.add_constant(Value::function(Arc::new(init_func)));
                            // 使用不同的注册方法取决于是否是常量
                            if field.is_const {
                                self.chunk.register_static_const(name, field.name.clone(), func_index);
                            } else {
                                self.chunk.register_static_field(name, field.name.clone(), func_index);
                            }
                        } else {
                            // 没有初始值，使用 null（常量字段在解析器中已强制要求初始值）
                            let null_index = self.chunk.add_constant(Value::null());
                            self.chunk.register_static_field(name, field.name.clone(), null_index);
                        }
                    }
                }
                
                // 注册构造函数参数属性提升的字段
                // 查找 init 方法，并注册 is_field=true 的参数为字段
                for method in methods {
                    if method.name == "init" {
                        for param in &method.params {
                            if param.is_field {
                                self.chunk.register_field(name, param.name.clone());
                            }
                        }
                        break; // 只有一个 init 方法
                    }
                }
                
                // 编译每个方法
                for method in methods {
                    self.compile_class_method(name, method, parent.as_deref(), *span);
                }
            }
            Stmt::InterfaceDef { name, type_params: _, super_interfaces: _, methods, span: _ } => {
                // 收集接口的方法签名
                let method_infos: Vec<_> = methods.iter().map(|m| {
                    crate::compiler::bytecode::InterfaceMethodInfo {
                        name: m.name.clone(),
                        arity: m.params.len(),
                    }
                }).collect();
                
                // 注册 interface
                self.chunk.register_interface(name.clone(), method_infos);
            }
            Stmt::TraitDef { name, type_params: _, where_clauses: _, super_traits: _, methods, span: _ } => {
                // 收集 trait 的方法信息
                let mut method_infos = Vec::new();
                
                for method in methods {
                    // 如果有默认实现，编译它
                    let default_impl = if let Some(body) = &method.default_body {
                        // 跳过方法体的跳转
                        let jump_over = self.chunk.write_jump(OpCode::Jump, method.span.line);
                        let func_start = self.chunk.current_offset();
                        
                        // 保存符号表状态
                        let saved_state = self.symbols.save_state();
                        let saved_scope_depth = self.symbols.scope_depth();
                        self.symbols.reset_for_function();
                        
                        // 定义 self 参数（trait 方法的第一个参数）
                        if let Err(msg) = self.symbols.define("self".to_string(), Type::Unknown, false) {
                            self.errors.push(CompileError::new(msg, method.span));
                        }
                        
                        // 定义其他参数
                        for param in &method.params {
                            if let Err(msg) = self.symbols.define(param.name.clone(), param.type_ann.ty.clone(), false) {
                                self.errors.push(CompileError::new(msg, param.span));
                            }
                        }
                        
                        // 编译方法体
                        self.compile_function_body(body);
                        
                        // 添加隐式返回
                        self.chunk.write_constant(Value::null(), method.span.line);
                        self.chunk.write_op(OpCode::Return, method.span.line);
                        
                        // 计算局部变量数量
                        let local_count = self.symbols.local_count();
                        
                        // 恢复符号表
                        self.symbols.restore_state_full(saved_state, saved_scope_depth);
                        
                        // 回填跳转
                        self.chunk.patch_jump(jump_over);
                        
                        // 创建函数对象
                        let func = crate::vm::value::Function {
                            name: Some(format!("{}::{}", name, method.name)),
                            arity: method.params.len() + 1, // +1 for self
                            required_params: method.params.len() + 1,
                            defaults: Vec::new(),
                            has_variadic: false,
                            chunk_index: func_start,
                            local_count,
                            upvalues: Vec::new(),
                        };
                        
                        Some(self.chunk.add_constant(Value::function(Arc::new(func))))
                    } else {
                        None
                    };
                    
                    method_infos.push(crate::compiler::bytecode::TraitMethodInfo {
                        name: method.name.clone(),
                        arity: method.params.len() + 1, // +1 for self
                        default_impl,
                    });
                }
                
                // 注册 trait
                self.chunk.register_trait(name.clone(), method_infos);
            }
            Stmt::EnumDef { name, variants, span: _ } => {
                // 收集 enum 变体信息，编译每个变体的值表达式
                let mut variant_infos = Vec::new();
                for v in variants {
                    // 如果有关联值，编译表达式并存入常量池
                    let value_index = if let Some(ref value_expr) = v.value {
                        // 编译表达式获取常量值
                        // 枚举值通常是常量表达式（整数字面量等）
                        match self.expr_to_value(value_expr) {
                            Ok(val) => Some(self.chunk.add_constant(val)),
                            Err(err) => {
                                // 非常量表达式，报错
                                self.errors.push(CompileError::new(
                                    format!("Enum variant value must be a constant expression: {}", err),
                                    v.span,
                                ));
                                None
                            }
                        }
                    } else {
                        None
                    };
                    
                    variant_infos.push(crate::compiler::bytecode::EnumVariantInfo {
                        name: v.name.clone(),
                        fields: v.fields.iter().map(|(name, _)| name.clone()).collect(),
                        value_index,
                    });
                }
                
                // 注册 enum
                self.chunk.register_enum(name.clone(), variant_infos);
            }
            Stmt::TypeAlias { name, target_type, span: _ } => {
                // 注册类型别名到符号表
                self.type_aliases.insert(name.clone(), target_type.ty.clone());
            }
            Stmt::TryCatch { try_block, catch_param, catch_block, finally_block, span } => {
                // 记录 try 块开始时的槽位，用于确保 catch 参数位置正确
                let try_start_slot = self.symbols.current_slot();
                
                // 设置异常处理器
                let setup_try = self.chunk.write_jump(OpCode::SetupTry, span.line);
                
                // 编译 try 块
                self.compile_stmt(try_block);
                
                // try 块正常结束，需要清理可能产生的局部变量，跳过 catch 块
                // 弹出 try 块中可能产生的临时值
                let try_end_slot = self.symbols.current_slot();
                for _ in try_start_slot..try_end_slot {
                    self.chunk.write_op(OpCode::Pop, span.line);
                }
                let skip_catch = self.chunk.write_jump(OpCode::Jump, span.line);
                
                // catch 块起始位置
                self.chunk.patch_jump(setup_try);
                
                // 开始 catch 作用域
                self.symbols.begin_scope();
                
                // 定义 catch 参数（如果有）
                // 注意：VM 在抛出异常时会将栈恢复到 try 开始时的深度，然后推入异常值
                // 所以异常值会在 try_start_slot 位置
                if let Some(param_name) = catch_param {
                    // 设置符号表槽位与 VM 栈位置匹配
                    self.symbols.set_current_slot(try_start_slot);
                    if let Err(msg) = self.symbols.define(param_name.clone(), crate::types::Type::Unknown, false) {
                        self.errors.push(CompileError::new(msg, *span));
                    }
                } else {
                    // 没有 catch 参数，弹出异常值
                    self.chunk.write_op(OpCode::Pop, span.line);
                }
                
                // 编译 catch 块
                self.compile_stmt(catch_block);
                
                // 结束 catch 作用域
                self.symbols.end_scope();
                
                // 如果有 catch 参数，弹出异常值
                if catch_param.is_some() {
                    self.chunk.write_op(OpCode::Pop, span.line);
                }
                
                // 恢复符号表槽位
                self.symbols.set_current_slot(try_start_slot);
                
                // 跳过 catch 的跳转目标
                self.chunk.patch_jump(skip_catch);
                
                // 编译 finally 块（如果有）
                if let Some(finally) = finally_block {
                    self.compile_stmt(finally);
                }
            }
            Stmt::Throw { value, span } => {
                // 编译要抛出的值
                self.compile_expr(value);
                // 生成 Throw 操作码
                self.chunk.write_op(OpCode::Throw, span.line);
            }
            Stmt::FnDef { name, type_params: _, where_clauses: _, params, return_type: _, body, visibility: _, span } => {
                // 编译命名函数定义（支持递归和前向引用）
                
                // 1. 检查是否已经预注册了这个函数（在 compile 第一遍中）
                let func_index = if let Some(idx) = self.chunk.get_named_function(name) {
                    // 使用已预留的索引
                    idx
                } else {
                    // 未预注册，创建新的（兼容直接调用 compile_stmt 的情况）
                    let idx = self.chunk.constants.len() as u16;
                    self.chunk.constants.push(Value::null());
                    self.chunk.register_named_function(name.clone(), idx);
                    idx
                };
                
                // 3. 写一个跳转指令跳过函数体
                let jump_over = self.chunk.write_jump(OpCode::Jump, span.line);
                
                // 4. 记录函数体起始位置
                let func_start = self.chunk.current_offset();
                
                // 5. 保存符号表状态，为函数创建独立作用域
                let saved_state = self.symbols.save_state();
                let saved_scope_depth = self.symbols.scope_depth();
                self.symbols.reset_for_function();
                
                let arity = params.len();
                let mut required_params = 0;
                let mut defaults = Vec::new();
                let mut has_default = false;
                let mut has_variadic = false;
                
                // 6. 定义参数
                for (idx, param) in params.iter().enumerate() {
                    let param_type = param.type_ann.ty.clone();
                    
                    if let Err(msg) = self.symbols.define(param.name.clone(), param_type, false) {
                        self.errors.push(CompileError::new(msg, param.span));
                    }
                    
                    // 检查可变参数
                    if param.variadic {
                        has_variadic = true;
                        if idx != params.len() - 1 {
                            self.errors.push(CompileError::new(
                                "Variadic parameter must be the last parameter".to_string(),
                                param.span,
                            ));
                        }
                        if param.default.is_some() {
                            self.errors.push(CompileError::new(
                                "Variadic parameter cannot have default value".to_string(),
                                param.span,
                            ));
                        }
                        continue;
                    }
                    
                    if let Some(default_expr) = &param.default {
                        has_default = true;
                        match self.expr_to_value(default_expr) {
                            Ok(value) => defaults.push(value),
                            Err(msg) => {
                                self.errors.push(CompileError::new(msg, param.span));
                                defaults.push(Value::null());
                            }
                        }
                    } else {
                        if has_default {
                            self.errors.push(CompileError::new(
                                "Required parameter cannot follow optional parameter".to_string(),
                                param.span,
                            ));
                        }
                        required_params += 1;
                    }
                }
                
                // 7. 编译函数体
                self.compile_function_body(body);
                
                // 8. 添加隐式返回
                let needs_return = self.chunk.code.is_empty() 
                    || self.chunk.code.last() != Some(&(OpCode::Return as u8));
                
                if needs_return {
                    self.chunk.write_constant(Value::null(), span.line);
                    self.chunk.write_op(OpCode::Return, span.line);
                }
                
                // 9. 计算局部变量数量
                let local_count = self.symbols.local_count();
                
                // 10. 恢复符号表
                self.symbols.restore_state_full(saved_state, saved_scope_depth);
                
                // 11. 回填跳转
                self.chunk.patch_jump(jump_over);
                
                // 12. 创建函数对象并替换常量池中的占位值
                let func = crate::vm::value::Function {
                    name: Some(name.clone()),
                    arity,
                    required_params,
                    defaults,
                    has_variadic,
                    chunk_index: func_start,
                    local_count,
                    upvalues: Vec::new(),
                };
                self.chunk.constants[func_index as usize] = Value::function(Arc::new(func));
                
                // 13. 收集参数名列表并注册到命名函数信息中（用于命名参数重排）
                let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                self.chunk.register_named_function_info(name.clone(), func_index, param_names);
                
                // 注意：不再在顶级作用域定义函数变量
                // 函数调用通过 named_functions 查找，而不是通过符号表
            }
            
            // Package 和 Import 语句在编译阶段不生成字节码
            // 它们是元数据，由包管理系统在编译前处理
            Stmt::Package { .. } => {
                // 包声明不生成字节码
            }
            Stmt::Import { .. } => {
                // 导入声明不生成字节码（由包加载器处理）
            }
        }
    }

    /// 编译函数体
    /// 如果函数体是一个 Block，直接编译其内部语句，避免额外的作用域管理
    fn compile_function_body(&mut self, body: &Stmt) {
        match body {
            Stmt::Block { statements, .. } => {
                // 直接编译块内的语句，不调用 begin_scope/end_scope
                // 因为函数体的局部变量应该在函数返回时清理
                for stmt in statements {
                    self.compile_stmt(stmt);
                }
            }
            _ => {
                // 如果不是块语句，直接编译
                self.compile_stmt(body);
            }
        }
    }
    
    /// 编译 struct 方法
    fn compile_struct_method(&mut self, struct_name: &str, method: &crate::parser::ast::StructMethod, _span: Span) {
        use crate::parser::ast::StructMethod;
        
        let StructMethod { name, params, return_type: _, body, visibility: _, span: method_span } = method;
        
        // 1. 写一个跳转指令跳过方法体
        let jump_over = self.chunk.write_jump(OpCode::Jump, method_span.line);
        
        // 2. 记录方法体起始位置
        let func_start = self.chunk.current_offset();
        
        // 3. 保存符号表状态
        let saved_state = self.symbols.save_state();
        let saved_scope_depth = self.symbols.scope_depth();
        self.symbols.reset_for_function();
        
        // 4. 定义 this 参数（隐式第一个参数）
        if let Err(msg) = self.symbols.define("this".to_string(), Type::Unknown, false) {
            self.errors.push(CompileError::new(msg, *method_span));
        }
        
        // 5. 定义其他参数
        let arity = params.len() + 1; // +1 for this
        let mut required_params = 1; // this is required
        let mut defaults = Vec::new();
        let mut has_default = false;
        let mut has_variadic = false;
        
        for (idx, param) in params.iter().enumerate() {
            let param_type = param.type_ann.ty.clone();
            
            if let Err(msg) = self.symbols.define(param.name.clone(), param_type, false) {
                self.errors.push(CompileError::new(msg, param.span));
            }
            
            // 处理可变参数
            if param.variadic {
                has_variadic = true;
                if idx != params.len() - 1 {
                    self.errors.push(CompileError::new(
                        "Variadic parameter must be the last parameter".to_string(),
                        param.span,
                    ));
                }
                if param.default.is_some() {
                    self.errors.push(CompileError::new(
                        "Variadic parameter cannot have default value".to_string(),
                        param.span,
                    ));
                }
                continue;
            }
            
            if let Some(default_expr) = &param.default {
                has_default = true;
                match self.expr_to_value(default_expr) {
                    Ok(value) => defaults.push(value),
                    Err(msg) => {
                        self.errors.push(CompileError::new(msg, param.span));
                        defaults.push(Value::null());
                    }
                }
            } else {
                if has_default {
                    self.errors.push(CompileError::new(
                        "Required parameter cannot follow optional parameter".to_string(),
                        param.span,
                    ));
                }
                required_params += 1;
            }
        }
        
        // 6. 编译方法体
        self.compile_function_body(body);
        
        // 7. 添加隐式返回
        let needs_return = self.chunk.code.is_empty() 
            || self.chunk.code.last() != Some(&(OpCode::Return as u8));
        
        if needs_return {
            self.chunk.write_constant(Value::null(), method_span.line);
            self.chunk.write_op(OpCode::Return, method_span.line);
        }
        
        // 8. 计算局部变量数量
        let local_count = self.symbols.local_count();
        
        // 9. 恢复符号表
        self.symbols.restore_state_full(saved_state, saved_scope_depth);
        
        // 10. 回填跳转
        self.chunk.patch_jump(jump_over);
        
        // 11. 创建函数对象
        let func = crate::vm::value::Function {
            name: Some(format!("{}::{}", struct_name, name)),
            arity,
            required_params,
            defaults,
            has_variadic,
            chunk_index: func_start,
            local_count,
            upvalues: Vec::new(),
        };
        
        // 12. 添加到常量池并注册方法
        let func_index = self.chunk.add_constant(Value::function(Arc::new(func)));
        self.chunk.register_method(struct_name, name.clone(), func_index);
    }
    
    /// 编译 class 方法
    fn compile_class_method(&mut self, class_name: &str, method: &crate::parser::ast::ClassMethod, parent: Option<&str>, _span: Span) {
        use crate::parser::ast::ClassMethod;
        
        let ClassMethod { name, params, return_type: _, body, visibility: _, is_static, is_override, is_abstract, span: method_span } = method;
        
        // override 检查：如果标记了 override，父类必须有同名方法
        if *is_override {
            if let Some(parent_name) = parent {
                // 检查父类是否有该方法
                let parent_has_method = self.chunk.get_method(parent_name, name).is_some();
                if !parent_has_method {
                    let msg = format!(
                        "Method '{}' is marked as override but parent class '{}' has no such method",
                        name, parent_name
                    );
                    self.errors.push(CompileError::new(msg, *method_span));
                    return;
                }
            } else {
                let msg = format!(
                    "Method '{}' is marked as override but class '{}' has no parent class",
                    name, class_name
                );
                self.errors.push(CompileError::new(msg, *method_span));
                return;
            }
        }
        
        // 抽象方法没有方法体，只注册签名
        if *is_abstract {
            self.chunk.register_abstract_method(class_name, name.clone());
            return;
        }
        
        // 方法体必须存在（非抽象方法）
        let body = match body {
            Some(b) => b,
            None => {
                self.errors.push(CompileError::new(
                    "Non-abstract method must have a body".to_string(),
                    *method_span,
                ));
                return;
            }
        };
        
        // 1. 写一个跳转指令跳过方法体
        let jump_over = self.chunk.write_jump(OpCode::Jump, method_span.line);
        
        // 2. 记录方法体起始位置
        let func_start = self.chunk.current_offset();
        
        // 3. 保存符号表状态
        let saved_state = self.symbols.save_state();
        let saved_scope_depth = self.symbols.scope_depth();
        self.symbols.reset_for_function();
        
        let mut arity = params.len();
        let mut required_params = 0;
        let mut defaults = Vec::new();
        let mut has_default = false;
        let mut has_variadic = false;
        
        // 4. 对于非静态方法，定义 this 参数（隐式第一个参数）
        if !*is_static {
            if let Err(msg) = self.symbols.define("this".to_string(), Type::Unknown, false) {
                self.errors.push(CompileError::new(msg, *method_span));
            }
            arity += 1;
            required_params += 1;
        }
        
        // 5. 定义其他参数
        for (idx, param) in params.iter().enumerate() {
            let param_type = param.type_ann.ty.clone();
            
            if let Err(msg) = self.symbols.define(param.name.clone(), param_type, false) {
                self.errors.push(CompileError::new(msg, param.span));
            }
            
            // 处理可变参数
            if param.variadic {
                has_variadic = true;
                if idx != params.len() - 1 {
                    self.errors.push(CompileError::new(
                        "Variadic parameter must be the last parameter".to_string(),
                        param.span,
                    ));
                }
                if param.default.is_some() {
                    self.errors.push(CompileError::new(
                        "Variadic parameter cannot have default value".to_string(),
                        param.span,
                    ));
                }
                continue;
            }
            
            if let Some(default_expr) = &param.default {
                has_default = true;
                match self.expr_to_value(default_expr) {
                    Ok(value) => defaults.push(value),
                    Err(msg) => {
                        self.errors.push(CompileError::new(msg, param.span));
                        defaults.push(Value::null());
                    }
                }
            } else {
                if has_default {
                    self.errors.push(CompileError::new(
                        "Required parameter cannot follow optional parameter".to_string(),
                        param.span,
                    ));
                }
                required_params += 1;
            }
        }
        
        // 5.5 对于 init 方法，生成构造函数参数属性提升的字段赋值
        if name == "init" && !*is_static {
            for (param_idx, param) in params.iter().enumerate() {
                if param.is_field {
                    // 生成 this.field = param 的字节码
                    // 1. 加载 this（槽 0）
                    self.chunk.write_get_local(0, method_span.line);
                    // 2. 加载参数值（槽 1 + param_idx，因为 this 在槽 0）
                    self.chunk.write_get_local(1 + param_idx, method_span.line);
                    // 3. SetField
                    let field_name_index = self.chunk.add_constant(Value::string(param.name.clone()));
                    self.chunk.write_op(OpCode::SetField, method_span.line);
                    self.chunk.write_u16(field_name_index, method_span.line);
                    // SetField 会将栈顶的值弹出，保留对象在栈上
                    // 我们需要弹出 this
                    self.chunk.write_op(OpCode::Pop, method_span.line);
                }
            }
        }
        
        // 6. 编译方法体
        self.compile_function_body(body);
        
        // 7. 添加隐式返回
        let needs_return = self.chunk.code.is_empty() 
            || self.chunk.code.last() != Some(&(OpCode::Return as u8));
        
        if needs_return {
            // init 方法返回 this，其他方法返回 null
            if name == "init" {
                // 返回 this（局部变量槽 0）
                self.chunk.write_get_local(0, method_span.line);
            } else {
                self.chunk.write_constant(Value::null(), method_span.line);
            }
            self.chunk.write_op(OpCode::Return, method_span.line);
        }
        
        // 8. 计算局部变量数量
        let local_count = self.symbols.local_count();
        
        // 9. 恢复符号表
        self.symbols.restore_state_full(saved_state, saved_scope_depth);
        
        // 10. 回填跳转
        self.chunk.patch_jump(jump_over);
        
        // 11. 创建函数对象
        let func = crate::vm::value::Function {
            name: Some(format!("{}::{}", class_name, name)),
            arity,
            required_params,
            defaults,
            has_variadic,
            chunk_index: func_start,
            local_count,
            upvalues: Vec::new(),
        };
        
        // 12. 添加到常量池并注册方法（静态或实例）
        let func_index = self.chunk.add_constant(Value::function(Arc::new(func)));
        if *is_static {
            self.chunk.register_static_method(class_name, name.clone(), func_index);
        } else {
            self.chunk.register_method(class_name, name.clone(), func_index);
        }
    }

    /// 编译表达式
    fn compile_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Integer { value, span } => {
                // 优化：小整数使用 ConstInt8 指令
                if *value >= -128 && *value <= 127 {
                    self.chunk.write_const_int8(*value as i8, span.line);
                } else {
                    self.chunk.write_constant(Value::int(*value), span.line);
                }
            }
            Expr::Float { value, span } => {
                self.chunk.write_constant(Value::float(*value), span.line);
            }
            Expr::String { value, span } => {
                self.chunk.write_constant(Value::string(value.clone()), span.line);
            }
            Expr::StringInterpolation { parts, span } => {
                // 编译字符串插值为一系列字符串连接操作
                use crate::parser::ast::StringInterpPart;
                
                let mut first = true;
                for part in parts {
                    match part {
                        StringInterpPart::Literal(s) => {
                            if !s.is_empty() {
                                self.chunk.write_constant(Value::string(s.clone()), span.line);
                                if !first {
                                    // 将当前字符串与之前的结果连接
                                    self.chunk.write_op(OpCode::Add, span.line);
                                }
                                first = false;
                            }
                        }
                        StringInterpPart::Expr(expr) => {
                            self.compile_expr(expr);
                            // 将表达式结果转换为字符串
                            self.chunk.write_op(OpCode::ToString, span.line);
                            if !first {
                                // 将转换后的字符串与之前的结果连接
                                self.chunk.write_op(OpCode::Add, span.line);
                            }
                            first = false;
                        }
                    }
                }
                
                // 如果没有任何部分，返回空字符串
                if first {
                    self.chunk.write_constant(Value::string(String::new()), span.line);
                }
            }
            Expr::Bool { value, span } => {
                self.chunk.write_constant(Value::bool(*value), span.line);
            }
            Expr::Char { value, span } => {
                self.chunk.write_constant(Value::char(*value), span.line);
            }
            Expr::Null { span } => {
                self.chunk.write_constant(Value::null(), span.line);
            }
            Expr::Identifier { name, span } => {
                // 查找变量
                if let Some(slot) = self.symbols.resolve_slot(name) {
                    if let Some(symbol) = self.symbols.resolve(name) {
                        if self.is_fast_int_type(&symbol.ty) {
                            self.chunk.write_get_local_int(slot as u16, span.line);
                        } else {
                            self.chunk.write_get_local(slot, span.line);
                        }
                    } else {
                        self.chunk.write_get_local(slot, span.line);
                    }
                } else if let Some(func_index) = self.chunk.get_named_function(name) {
                    // 如果是命名函数，从常量池加载
                    self.chunk.write_op(OpCode::Const, span.line);
                    self.chunk.write_u16(func_index, span.line);
                } else {
                    let msg = format!("Undefined variable: {}", name);
                    self.errors.push(CompileError::new(msg, *span));
                }
            }
            Expr::Binary { left, op, right, span } => {
                // 逻辑运算符需要短路求值，单独处理
                match op {
                    BinOp::And => {
                        // 短路求值: 如果左侧为假，跳过右侧，结果为左侧（假）
                        self.compile_expr(left);
                        
                        // 如果为假则跳转到结束（保留左侧值作为结果）
                        let jump_if_false = self.chunk.write_jump(OpCode::JumpIfFalse, span.line);
                        
                        // 弹出左侧值（因为为真，需要继续计算右侧）
                        self.chunk.write_op(OpCode::Pop, span.line);
                        
                        // 编译右侧
                        self.compile_expr(right);
                        
                        // 回填跳转
                        self.chunk.patch_jump(jump_if_false);
                        return;
                    }
                    BinOp::Or => {
                        // 短路求值: 如果左侧为真，跳过右侧，结果为左侧（真）
                        self.compile_expr(left);
                        
                        // 如果为真则跳转到结束（保留左侧值作为结果）
                        let jump_if_true = self.chunk.write_jump(OpCode::JumpIfTrue, span.line);
                        
                        // 弹出左侧值（因为为假，需要继续计算右侧）
                        self.chunk.write_op(OpCode::Pop, span.line);
                        
                        // 编译右侧
                        self.compile_expr(right);
                        
                        // 回填跳转
                        self.chunk.patch_jump(jump_if_true);
                        return;
                    }
                    _ => {}
                }
                
                // 其他二元运算符：先编译两边，再执行运算
                // 优化：检查两边是否都是整数类型
                let left_type = self.infer_type(left);
                let right_type = self.infer_type(right);
                let both_int = left_type == StaticType::Int && right_type == StaticType::Int;

                // 融合指令优化：局部整数变量与小整数常量
                if both_int {
                    let try_emit_local_const = |compiler: &mut Compiler,
                                                local_name: &str,
                                                const_value: i64| {
                        if const_value < -128 || const_value > 127 {
                            return false;
                        }
                        if let Some(symbol) = compiler.symbols.resolve(local_name) {
                            if compiler.is_fast_int_type(&symbol.ty) {
                                if let Some(slot) = compiler.symbols.resolve_slot(local_name) {
                                    let v = const_value as i8;
                                    match op {
                                        BinOp::Add => {
                                            compiler.chunk.write_get_local_add_int(slot as u16, v, span.line);
                                            return true;
                                        }
                                        BinOp::Sub => {
                                            compiler.chunk.write_get_local_sub_int(slot as u16, v, span.line);
                                            return true;
                                        }
                                        BinOp::Le => {
                                            compiler.chunk.write_op(OpCode::GetLocalLeInt, span.line);
                                            compiler.chunk.write_u16(slot as u16, span.line);
                                            compiler.chunk.write(v as u8, span.line);
                                            return true;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        false
                    };

                    if let (Expr::Identifier { name, .. }, Expr::Integer { value, .. }) =
                        (left.as_ref(), right.as_ref())
                    {
                        if try_emit_local_const(self, name, *value) {
                            return;
                        }
                    }

                    // 加法是可交换的，支持常量在左侧的情况
                    if let (Expr::Integer { value, .. }, Expr::Identifier { name, .. }) =
                        (left.as_ref(), right.as_ref())
                    {
                        if *op == BinOp::Add {
                            if try_emit_local_const(self, name, *value) {
                                return;
                            }
                        }
                    }
                    
                    // 超级指令优化：两个局部变量相加/相减（整数类型）
                    if both_int {
                        if let (Expr::Identifier { name: name1, .. }, Expr::Identifier { name: name2, .. }) =
                            (left.as_ref(), right.as_ref())
                        {
                            if let (Some(slot1), Some(slot2)) = 
                                (self.symbols.resolve_slot(name1), self.symbols.resolve_slot(name2))
                            {
                                // 仅当槽位在 u8 范围内时使用超级指令
                                if slot1 <= 255 && slot2 <= 255 {
                                    match op {
                                        BinOp::Add => {
                                            self.chunk.write_add_locals(slot1 as u8, slot2 as u8, span.line);
                                            return;
                                        }
                                        BinOp::Sub => {
                                            self.chunk.write_sub_locals(slot1 as u8, slot2 as u8, span.line);
                                            return;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
                
                self.compile_expr(left);
                self.compile_expr(right);
                
                let opcode = if both_int {
                    // 使用专用整数指令（更快）
                    match op {
                        BinOp::Add => OpCode::AddInt,
                        BinOp::Sub => OpCode::SubInt,
                        BinOp::Mul => OpCode::MulInt,
                        BinOp::Div => OpCode::DivInt,
                        BinOp::Lt => OpCode::LtInt,
                        BinOp::Le => OpCode::LeInt,
                        BinOp::Gt => OpCode::GtInt,
                        BinOp::Ge => OpCode::GeInt,
                        BinOp::Eq => OpCode::EqInt,
                        BinOp::Ne => OpCode::NeInt,
                        // 其他操作没有专用整数版本，使用通用版本
                        BinOp::Mod => OpCode::Mod,
                        BinOp::Pow => OpCode::Pow,
                        BinOp::BitAnd => OpCode::BitAnd,
                        BinOp::BitOr => OpCode::BitOr,
                        BinOp::BitXor => OpCode::BitXor,
                        BinOp::Shl => OpCode::Shl,
                        BinOp::Shr => OpCode::Shr,
                        BinOp::And | BinOp::Or => unreachable!(),
                    }
                } else {
                    // 使用通用指令
                    match op {
                    BinOp::Add => OpCode::Add,
                    BinOp::Sub => OpCode::Sub,
                    BinOp::Mul => OpCode::Mul,
                    BinOp::Div => OpCode::Div,
                    BinOp::Mod => OpCode::Mod,
                    BinOp::Pow => OpCode::Pow,
                    BinOp::Eq => OpCode::Eq,
                    BinOp::Ne => OpCode::Ne,
                    BinOp::Lt => OpCode::Lt,
                    BinOp::Le => OpCode::Le,
                    BinOp::Gt => OpCode::Gt,
                    BinOp::Ge => OpCode::Ge,
                    BinOp::BitAnd => OpCode::BitAnd,
                    BinOp::BitOr => OpCode::BitOr,
                    BinOp::BitXor => OpCode::BitXor,
                    BinOp::Shl => OpCode::Shl,
                    BinOp::Shr => OpCode::Shr,
                    BinOp::And | BinOp::Or => unreachable!(), // 已在上面处理
                    }
                };
                
                self.chunk.write_op(opcode, span.line);
            }
            Expr::Unary { op, operand, span } => {
                self.compile_expr(operand);
                
                let opcode = match op {
                    UnaryOp::Neg => OpCode::Neg,
                    UnaryOp::Not => OpCode::Not,
                    UnaryOp::BitNot => OpCode::BitNot,
                };
                
                self.chunk.write_op(opcode, span.line);
            }
            Expr::Grouping { expr, .. } => {
                self.compile_expr(expr);
            }
            Expr::Call { callee, args, span } => {
                // 提取参数值（命名参数将在后面处理）
                let has_named_args = args.iter().any(|(name, _)| name.is_some());
                
                // 检查是否是内置函数（内置函数不支持命名参数）
                if let Expr::Identifier { name, .. } = callee.as_ref() {
                    if has_named_args {
                        // 内置函数不支持命名参数，但仍需检查
                        match name.as_str() {
                            "print" | "println" | "typeof" | "typeinfo" | "sizeof" | "panic" | "time" => {
                                let msg = "Built-in functions do not support named arguments".to_string();
                                self.errors.push(CompileError::new(msg, *span));
                                return;
                            }
                            _ => {}
                        }
                    }
                    
                    match name.as_str() {
                        "print" if args.len() == 1 => {
                            self.compile_expr(&args[0].1);
                            self.chunk.write_op(OpCode::Print, span.line);
                            // 内置函数需要返回值，以便作为表达式使用
                            self.chunk.write_constant(Value::null(), span.line);
                            return;
                        }
                        "println" if args.len() == 1 => {
                            self.compile_expr(&args[0].1);
                            self.chunk.write_op(OpCode::PrintLn, span.line);
                            // 内置函数需要返回值，以便作为表达式使用
                            self.chunk.write_constant(Value::null(), span.line);
                            return;
                        }
                        "typeof" if args.len() == 1 => {
                            self.compile_expr(&args[0].1);
                            self.chunk.write_op(OpCode::TypeOf, span.line);
                            return;
                        }
                        "typeinfo" if args.len() == 1 => {
                            // 获取完整的运行时类型信息对象
                            self.compile_expr(&args[0].1);
                            self.chunk.write_op(OpCode::TypeInfo, span.line);
                            return;
                        }
                        "sizeof" if args.len() == 1 => {
                            self.compile_expr(&args[0].1);
                            self.chunk.write_op(OpCode::SizeOf, span.line);
                            return;
                        }
                        "panic" if args.len() == 1 => {
                            self.compile_expr(&args[0].1);
                            self.chunk.write_op(OpCode::Panic, span.line);
                            return;
                        }
                        // [deprecated] time() 函数可能在未来版本移除
                        "time" if args.is_empty() => {
                            self.chunk.write_op(OpCode::Time, span.line);
                            return;
                        }
                        _ => {}
                    }
                }
                
                // 检查是否是静态成员调用 (ClassName::method(args))
                if let Expr::StaticMember { class_name, member, span: member_span } = callee.as_ref() {
                    // 检查是否是枚举的内置方法
                    let is_enum_builtin = if self.chunk.get_enum(class_name).is_some() {
                        member == "fromValue" || member == "values"
                    } else {
                        false
                    };
                    
                    if is_enum_builtin {
                        // 枚举内置方法，生成 InvokeStatic 调用
                        let class_name_index = self.chunk.add_constant(Value::string(class_name.clone()));
                        let method_name_index = self.chunk.add_constant(Value::string(member.clone()));
                        
                        // 先编译所有参数
                        for (_, arg) in args {
                            self.compile_expr(arg);
                        }
                        
                        // 生成 InvokeStatic 指令
                        self.chunk.write_op(OpCode::InvokeStatic, span.line);
                        self.chunk.write_u16(class_name_index, span.line);
                        self.chunk.write_u16(method_name_index, span.line);
                        self.chunk.write(args.len() as u8, span.line);
                        return;
                    }
                    
                    // 检查是否是静态方法
                    let has_static_method = self.chunk.get_static_method(class_name, member).is_some();
                    
                    if has_static_method {
                        let func_index = self.chunk.get_static_method(class_name, member).unwrap();
                        
                        // 检查参数数量
                        if args.len() > u8::MAX as usize {
                            let msg = "Too many arguments".to_string();
                            self.errors.push(CompileError::new(msg, *span));
                            return;
                        }
                        
                        // 生成调用静态方法的指令
                        // 1. 先从常量池加载函数
                        self.chunk.write_op(OpCode::Const, span.line);
                        self.chunk.write_u16(func_index, span.line);
                        
                        // 2. 然后编译所有参数
                        for (_, arg) in args {
                            self.compile_expr(arg);
                        }
                        
                        // 3. 发出调用指令
                        self.chunk.write_op(OpCode::Call, span.line);
                        self.chunk.write(args.len() as u8, span.line);
                        return;
                    } else {
                        let msg = format!("Type '{}' has no static method '{}'", class_name, member);
                        self.errors.push(CompileError::new(msg, *member_span));
                        return;
                    }
                }
                
                // 检查是否是方法调用 (obj.method(args))
                if let Expr::Member { object, member, span: member_span } = callee.as_ref() {
                    // 检查是否是 super 调用 (super.method(args))
                    if matches!(object.as_ref(), Expr::Super { .. }) {
                        // 编译 this（super 方法需要 this 作为 receiver）
                        if let Some(slot) = self.symbols.resolve_slot("this") {
                            self.chunk.write_get_local(slot, span.line);
                        } else {
                            let msg = "'super' can only be used inside a class method".to_string();
                            self.errors.push(CompileError::new(msg, *span));
                            return;
                        }
                        
                        // 编译所有参数
                        for (_, arg) in args {
                            self.compile_expr(arg);
                        }
                        
                        // 检查参数数量
                        if args.len() > u8::MAX as usize {
                            let msg = "Too many arguments".to_string();
                            self.errors.push(CompileError::new(msg, *span));
                            return;
                        }
                        
                        // 将方法名添加到常量池
                        let method_name_index = self.chunk.add_constant(Value::string(member.clone()));
                        
                        // 生成 InvokeSuper 指令
                        self.chunk.write_op(OpCode::InvokeSuper, span.line);
                        self.chunk.write_u16(method_name_index, span.line);
                        self.chunk.write(args.len() as u8, span.line);
                        return;
                    }
                    
                    // 检查是否是静态方法调用 (ClassName.method(args))
                    if let Expr::Identifier { name: class_name, .. } = object.as_ref() {
                        // 检查是否是已注册的类名
                        if self.chunk.get_type(class_name).is_some() {
                            // 静态方法调用
                            if let Some(func_index) = self.chunk.get_static_method(class_name, member) {
                                // 检查参数数量
                                if args.len() > u8::MAX as usize {
                                    let msg = "Too many arguments".to_string();
                                    self.errors.push(CompileError::new(msg, *span));
                                    return;
                                }
                                
                                // 生成调用静态方法的指令
                                // 栈布局: [func, arg1, arg2, ...] -> Call -> [result]
                                // 1. 先从常量池加载函数
                                self.chunk.write_op(OpCode::Const, span.line);
                                self.chunk.write_u16(func_index, span.line);
                                
                                // 2. 然后编译所有参数
                                for (_, arg) in args {
                                    self.compile_expr(arg);
                                }
                                
                                // 3. 发出调用指令
                                self.chunk.write_op(OpCode::Call, span.line);
                                self.chunk.write(args.len() as u8, span.line);
                                return;
                            } else {
                                let msg = format!("Type '{}' has no static method '{}'", class_name, member);
                                self.errors.push(CompileError::new(msg, *member_span));
                                return;
                            }
                        }
                    }
                    
                    // 实例方法调用
                    // 编译对象表达式（receiver）
                    self.compile_expr(object);
                    
                    // 编译所有参数
                    for (_, arg) in args {
                        self.compile_expr(arg);
                    }
                    
                    // 检查参数数量
                    if args.len() > u8::MAX as usize {
                        let msg = "Too many arguments".to_string();
                        self.errors.push(CompileError::new(msg, *span));
                        return;
                    }
                    
                    // 将方法名添加到常量池
                    let method_name_index = self.chunk.add_constant(Value::string(member.clone()));
                    
                    // 生成 InvokeMethod 指令
                    self.chunk.write_op(OpCode::InvokeMethod, span.line);
                    self.chunk.write_u16(method_name_index, span.line);
                    self.chunk.write(args.len() as u8, span.line);
                    return;
                }
                
                // 检查是否是安全方法调用 (obj?.method(args))
                if let Expr::SafeMember { object, member, span: member_span } = callee.as_ref() {
                    // 编译对象表达式
                    self.compile_expr(object);
                    
                    // 编译所有参数
                    for (_, arg) in args {
                        self.compile_expr(arg);
                    }
                    
                    // 检查参数数量
                    if args.len() > u8::MAX as usize {
                        let msg = "Too many arguments".to_string();
                        self.errors.push(CompileError::new(msg, *span));
                        return;
                    }
                    
                    // 将方法名添加到常量池
                    let method_name_index = self.chunk.add_constant(Value::string(member.clone()));
                    
                    // 生成 SafeInvokeMethod 指令（如果对象为 null 则返回 null）
                    self.chunk.write_op(OpCode::SafeInvokeMethod, member_span.line);
                    self.chunk.write_u16(method_name_index, member_span.line);
                    self.chunk.write(args.len() as u8, member_span.line);
                    return;
                }
                
                // 检查是否是非空断言方法调用 (obj!.method(args))
                if let Expr::NonNullMember { object, member, span: member_span } = callee.as_ref() {
                    // 编译对象表达式
                    self.compile_expr(object);
                    
                    // 编译所有参数
                    for (_, arg) in args {
                        self.compile_expr(arg);
                    }
                    
                    // 检查参数数量
                    if args.len() > u8::MAX as usize {
                        let msg = "Too many arguments".to_string();
                        self.errors.push(CompileError::new(msg, *span));
                        return;
                    }
                    
                    // 将方法名添加到常量池
                    let method_name_index = self.chunk.add_constant(Value::string(member.clone()));
                    
                    // 生成 NonNullInvokeMethod 指令（如果对象为 null 则 panic）
                    self.chunk.write_op(OpCode::NonNullInvokeMethod, member_span.line);
                    self.chunk.write_u16(method_name_index, member_span.line);
                    self.chunk.write(args.len() as u8, member_span.line);
                    return;
                }
                
                // 用户定义函数调用
                // 1. 编译被调用的表达式（将函数值压栈）
                self.compile_expr(callee);
                
                // 2. 编译所有参数（依次压栈）
                // 如果有命名参数，需要根据函数定义重新排列参数顺序
                if has_named_args {
                    // 命名参数调用：需要根据函数定义重排参数
                    // 尝试获取函数的参数名列表
                    let param_names = if let Expr::Identifier { name, .. } = callee.as_ref() {
                        self.symbols.resolve(name).and_then(|s| s.param_names.clone())
                    } else {
                        None
                    };
                    
                    if let Some(param_names) = param_names {
                        // 有参数名信息，进行重排
                        // 1. 分离位置参数和命名参数
                        let mut positional_args: Vec<&Expr> = Vec::new();
                        let mut named_args: std::collections::HashMap<&str, &Expr> = std::collections::HashMap::new();
                        
                        for (name, arg) in args {
                            if let Some(n) = name {
                                named_args.insert(n.as_str(), arg);
                            } else {
                                positional_args.push(arg);
                            }
                        }
                        
                        // 2. 按照函数定义的参数顺序编译参数
                        for (idx, param_name) in param_names.iter().enumerate() {
                            if idx < positional_args.len() {
                                // 使用位置参数
                                self.compile_expr(positional_args[idx]);
                            } else if let Some(arg) = named_args.get(param_name.as_str()) {
                                // 使用命名参数
                                self.compile_expr(arg);
                            } else {
                                // 参数缺失，报错（或者依赖默认参数处理）
                                let msg = format!("Missing argument for parameter '{}'", param_name);
                                self.errors.push(CompileError::new(msg, *span));
                                // 压入 null 占位
                                self.chunk.write_constant(Value::null(), span.line);
                            }
                        }
                    } else {
                        // 没有参数名信息，按原顺序编译（可能产生错误结果）
                        for (_, arg) in args {
                            self.compile_expr(arg);
                        }
                    }
                } else {
                    // 位置参数：按顺序压栈
                    for (_, arg) in args {
                        self.compile_expr(arg);
                    }
                }
                
                // 3. 生成 Call 指令
                if args.len() > u8::MAX as usize {
                    let msg = "Too many arguments".to_string();
                    self.errors.push(CompileError::new(msg, *span));
                    return;
                }
                self.chunk.write_call(args.len() as u8, span.line);
            }
            Expr::Assign { target, op, value, span } => {
                use crate::parser::ast::AssignOp;
                
                match target.as_ref() {
                    Expr::Identifier { name, .. } => {
                    // 检查是否是常量
                    if let Some(true) = self.symbols.is_const(name) {
                        let msg = format!("Cannot assign to constant '{}'", name);
                        self.errors.push(CompileError::new(msg, *span));
                        return;
                    }
                    
                    // 获取变量槽位
                    if let Some(slot) = self.symbols.resolve_slot(name) {
                        match op {
                            AssignOp::Assign => {
                                // 简单赋值：编译右侧值
                                self.compile_expr(value);
                            }
                            _ => {
                                // 复合赋值：先获取当前值，再编译右侧，最后执行运算
                                let mut fused_done = false;
                                if let Some(symbol) = self.symbols.resolve(name) {
                                    if self.is_fast_int_type(&symbol.ty) {
                                        if let Expr::Integer { value: rhs, .. } = value.as_ref() {
                                            if *rhs >= -128 && *rhs <= 127 {
                                                let v = *rhs as i8;
                                                match op {
                                                    AssignOp::AddAssign => {
                                                        self.chunk.write_get_local_add_int(slot as u16, v, span.line);
                                                        fused_done = true;
                                                    }
                                                    AssignOp::SubAssign => {
                                                        self.chunk.write_get_local_sub_int(slot as u16, v, span.line);
                                                        fused_done = true;
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                }

                                if !fused_done {
                                    self.chunk.write_get_local(slot, span.line);
                                    self.compile_expr(value);
                                    
                                    // 执行对应的二元运算
                                    let bin_op = match op {
                                        AssignOp::AddAssign => OpCode::Add,
                                        AssignOp::SubAssign => OpCode::Sub,
                                        AssignOp::MulAssign => OpCode::Mul,
                                        AssignOp::DivAssign => OpCode::Div,
                                        AssignOp::ModAssign => OpCode::Mod,
                                        AssignOp::BitAndAssign => OpCode::BitAnd,
                                        AssignOp::BitOrAssign => OpCode::BitOr,
                                        AssignOp::BitXorAssign => OpCode::BitXor,
                                        AssignOp::ShlAssign => OpCode::Shl,
                                        AssignOp::ShrAssign => OpCode::Shr,
                                        AssignOp::Assign => unreachable!(),
                                    };
                                    self.chunk.write_op(bin_op, span.line);
                                }
                            }
                        }
                        
                        // 存入变量
                        self.chunk.write_set_local(slot, span.line);
                    } else {
                        let msg = format!("Undefined variable: {}", name);
                        self.errors.push(CompileError::new(msg, *span));
                    }
                    }
                    Expr::Member { object, member, span: member_span } => {
                        // 成员赋值: obj.field = value
                        // 栈布局: [..., obj, value] -> SetField -> [..., obj]
                        
                        // 先编译对象
                        self.compile_expr(object);
                        
                        match op {
                            AssignOp::Assign => {
                                // 简单赋值
                                self.compile_expr(value);
                            }
                            _ => {
                                // 复合赋值: obj.field op= value
                                // 需要先获取当前值
                                // 复制对象引用
                                self.chunk.write_op(OpCode::Dup, span.line);
                                // 获取字段值
                                let field_index = self.chunk.add_constant(Value::string(member.clone()));
                                self.chunk.write_op(OpCode::GetField, span.line);
                                self.chunk.write_u16(field_index, span.line);
                                // 编译右侧值
                                self.compile_expr(value);
                                // 执行运算
                                let bin_op = match op {
                                    AssignOp::AddAssign => OpCode::Add,
                                    AssignOp::SubAssign => OpCode::Sub,
                                    AssignOp::MulAssign => OpCode::Mul,
                                    AssignOp::DivAssign => OpCode::Div,
                                    AssignOp::ModAssign => OpCode::Mod,
                                    AssignOp::BitAndAssign => OpCode::BitAnd,
                                    AssignOp::BitOrAssign => OpCode::BitOr,
                                    AssignOp::BitXorAssign => OpCode::BitXor,
                                    AssignOp::ShlAssign => OpCode::Shl,
                                    AssignOp::ShrAssign => OpCode::Shr,
                                    AssignOp::Assign => unreachable!(),
                                };
                                self.chunk.write_op(bin_op, span.line);
                            }
                        }
                        
                        // 设置字段
                        let field_index = self.chunk.add_constant(Value::string(member.clone()));
                        self.chunk.write_op(OpCode::SetField, member_span.line);
                        self.chunk.write_u16(field_index, member_span.line);
                    }
                    Expr::Index { object, index, span: index_span } => {
                        // 索引赋值: arr[i] = value
                        self.compile_expr(object);
                        self.compile_expr(index);
                        
                        match op {
                            AssignOp::Assign => {
                                self.compile_expr(value);
                            }
                            _ => {
                                // 复合赋值需要先获取当前值
                                // 这里需要复制 object 和 index
                                // 暂不支持复合索引赋值，报错
                                let msg = "Compound index assignment not yet supported".to_string();
                                self.errors.push(CompileError::new(msg, *span));
                                return;
                            }
                        }
                        
                        // 设置索引
                        self.chunk.write_op(OpCode::SetIndex, index_span.line);
                    }
                    _ => {
                    let msg = "Invalid assignment target".to_string();
                    self.errors.push(CompileError::new(msg, *span));
                    }
                }
            }
            // 注意：Index, Member, SafeMember, NonNullMember, NullCoalesce, PostIncrement, 
            // PostDecrement, Cast, TypeCheck 在下方的新代码块中处理
            _ if false => {
                // 这个分支永远不会执行，只是用于保持编译通过
                unreachable!();
            }
            Expr::Range { start, end, inclusive, span } => {
                // 编译起始和结束值
                if let Some(start_expr) = start {
                    self.compile_expr(start_expr);
                } else {
                    // 如果没有起始值，使用0
                    self.chunk.write_constant(Value::int(0), span.line);
                }
                if let Some(end_expr) = end {
                    self.compile_expr(end_expr);
                } else {
                    // 如果没有结束值，使用i64::MAX（表示无限）
                    self.chunk.write_constant(Value::int(i64::MAX), span.line);
                }
                // 创建范围
                if *inclusive {
                    self.chunk.write_op(OpCode::NewRangeInclusive, span.line);
                } else {
                    self.chunk.write_op(OpCode::NewRange, span.line);
                }
            }
            Expr::IfExpr { span, .. } => {
                let msg = "If expression not yet implemented".to_string();
                self.errors.push(CompileError::new(msg, *span));
            }
            Expr::Array { elements, span } => {
                // 编译所有元素
                for elem in elements {
                    self.compile_expr(elem);
                }
                // 创建数组
                if elements.len() > u16::MAX as usize {
                    let msg = "Array too large".to_string();
                    self.errors.push(CompileError::new(msg, *span));
                    return;
                }
                self.chunk.write_op(OpCode::NewArray, span.line);
                self.chunk.write_u16(elements.len() as u16, span.line);
            }
            Expr::MapLiteral { entries, span } => {
                // 编译每个键值对
                for (key, value) in entries {
                    self.compile_expr(key);
                    self.compile_expr(value);
                }
                
                // 生成 NewMap 指令
                self.chunk.write_op(OpCode::NewMap, span.line);
                self.chunk.write_u16(entries.len() as u16, span.line);
            }
            Expr::Closure { params, return_type: _, body, span } => {
                // 1. 先写一个跳转指令跳过函数体
                let jump_over = self.chunk.write_jump(OpCode::Jump, span.line);
                
                // 2. 记录函数体起始位置
                let func_start = self.chunk.current_offset();
                
                // 3. 保存当前符号表状态，为函数创建独立的作用域
                let saved_state = self.symbols.save_state();
                let saved_scope_depth = self.symbols.scope_depth();
                self.symbols.reset_for_function();
                
                let arity = params.len();
                
                // 计算必需参数数量、收集默认值、检查可变参数
                let mut required_params = 0;
                let mut defaults = Vec::new();
                let mut has_default = false;
                let mut has_variadic = false;
                
                for (idx, param) in params.iter().enumerate() {
                    let param_type = param.type_ann.ty.clone();
                    
                    if let Err(msg) = self.symbols.define(param.name.clone(), param_type, false) {
                        self.errors.push(CompileError::new(msg, param.span));
                    }
                    
                    // 检查可变参数
                    if param.variadic {
                        has_variadic = true;
                        // 可变参数必须是最后一个
                        if idx != params.len() - 1 {
                            self.errors.push(CompileError::new(
                                "Variadic parameter must be the last parameter".to_string(),
                                param.span,
                            ));
                        }
                        // 可变参数不能有默认值
                        if param.default.is_some() {
                            self.errors.push(CompileError::new(
                                "Variadic parameter cannot have default value".to_string(),
                                param.span,
                            ));
                        }
                        continue; // 可变参数不计入 required_params
                    }
                    
                    if let Some(default_expr) = &param.default {
                        has_default = true;
                        // 尝试将默认表达式转换为常量值
                        match self.expr_to_value(default_expr) {
                            Ok(value) => defaults.push(value),
                            Err(msg) => {
                                self.errors.push(CompileError::new(msg, param.span));
                                defaults.push(Value::null()); // 占位
                            }
                        }
                    } else {
                        if has_default {
                            // 有默认值的参数之后不能有无默认值的参数（可变参数除外）
                            self.errors.push(CompileError::new(
                                "Required parameter cannot follow optional parameter".to_string(),
                                param.span,
                            ));
                        }
                        required_params += 1;
                    }
                }
                
                // 4. 编译函数体
                // 如果函数体是一个 Block，直接编译其内部语句，不增加额外作用域
                // 因为函数体的局部变量应该在函数返回时清理，而不是在块结束时
                self.compile_function_body(body);
                
                // 5. 如果函数体没有显式返回，添加隐式返回 null
                // 检查最后一条指令是否是 Return
                let needs_return = self.chunk.code.is_empty() 
                    || self.chunk.code.last() != Some(&(OpCode::Return as u8));
                
                if needs_return {
                    self.chunk.write_constant(Value::null(), span.line);
                    self.chunk.write_op(OpCode::Return, span.line);
                }
                
                // 计算局部变量数量（包括参数）
                let local_count = self.symbols.local_count();
                
                // 6. 恢复符号表状态
                self.symbols.restore_state_full(saved_state, saved_scope_depth);
                
                // 7. 回填跳转指令
                self.chunk.patch_jump(jump_over);
                
                // 8. 创建 Function 对象并存入常量池
                let func = Function {
                    name: None, // 闭包没有名字
                    arity,
                    required_params,
                    defaults,
                    has_variadic,
                    chunk_index: func_start,
                    local_count,
                    upvalues: Vec::new(), // TODO: 实际填充捕获的 upvalues
                };
                self.chunk.write_constant(Value::function(Arc::new(func)), span.line);
            }
            Expr::StructLiteral { name, fields, span } => {
                // 编译 struct 字面量
                // 1. 将类型名称添加到常量池
                let type_name_index = self.chunk.add_constant(Value::string(name.clone()));
                
                // 2. 编译每个字段：先压入字段名，再压入字段值
                for (field_name, field_value) in fields {
                    // 字段名
                    self.chunk.write_constant(Value::string(field_name.clone()), span.line);
                    // 字段值
                    self.compile_expr(field_value);
                }
                
                // 3. 生成 NewStruct 指令
                self.chunk.write_op(OpCode::NewStruct, span.line);
                self.chunk.write(fields.len() as u8, span.line); // 字段数量
                self.chunk.write_u16(type_name_index as u16, span.line); // 类型名称索引
            }
            Expr::Member { object, member, span } => {
                // 编译成员访问表达式 obj.field
                // 1. 编译对象表达式
                self.compile_expr(object);
                
                // 2. 将字段名添加到常量池
                let field_name_index = self.chunk.add_constant(Value::string(member.clone()));
                
                // 3. 生成 GetField 指令
                self.chunk.write_op(OpCode::GetField, span.line);
                self.chunk.write_u16(field_name_index as u16, span.line);
            }
            Expr::SafeMember { object, member, span } => {
                // 安全成员访问 obj?.field
                // 编译对象
                self.compile_expr(object);
                // 使用 SafeGetField 操作码（如果对象为 null 则返回 null）
                let field_name_index = self.chunk.add_constant(Value::string(member.clone()));
                self.chunk.write_op(OpCode::SafeGetField, span.line);
                self.chunk.write_u16(field_name_index, span.line);
            }
            Expr::NonNullMember { object, member, span } => {
                // 非空断言成员访问 obj!.field
                // 编译对象
                self.compile_expr(object);
                // 使用 NonNullGetField 操作码（如果对象为 null 则 panic）
                let field_name_index = self.chunk.add_constant(Value::string(member.clone()));
                self.chunk.write_op(OpCode::NonNullGetField, span.line);
                self.chunk.write_u16(field_name_index, span.line);
            }
            Expr::NullCoalesce { left, right, span } => {
                // 空值合并 a ?? b
                // 编译左侧表达式
                self.compile_expr(left);
                // 复制栈顶用于判断: [left, left_copy]
                self.chunk.write_op(OpCode::Dup, span.line);
                // JumpIfNull: 如果为 null 则跳转到 compute_right
                let jump_if_null = self.chunk.write_jump(OpCode::JumpIfNull, span.line);
                // 不为 null，弹出复制的值（保留原值）: [left]
                self.chunk.write_op(OpCode::Pop, span.line);
                // 跳过右侧计算
                let skip_right = self.chunk.write_jump(OpCode::Jump, span.line);
                // compute_right: [left, left_copy (null)]
                self.chunk.patch_jump(jump_if_null);
                // 弹出 left_copy (null): [left]
                self.chunk.write_op(OpCode::Pop, span.line);
                // 弹出原始 left (null): []
                self.chunk.write_op(OpCode::Pop, span.line);
                // 计算右侧: [right]
                self.compile_expr(right);
                // end:
                self.chunk.patch_jump(skip_right);
            }
            Expr::Index { object, index, span } => {
                // 编译数组索引访问 arr[i]
                self.compile_expr(object);
                self.compile_expr(index);
                self.chunk.write_op(OpCode::GetIndex, span.line);
            }
            Expr::PostIncrement { span, .. } => {
                let msg = "Post increment not yet implemented".to_string();
                self.errors.push(CompileError::new(msg, *span));
            }
            Expr::PostDecrement { span, .. } => {
                let msg = "Post decrement not yet implemented".to_string();
                self.errors.push(CompileError::new(msg, *span));
            }
            Expr::Cast { expr, target_type, force, span } => {
                // 编译要转换的表达式
                self.compile_expr(expr);
                
                // 获取目标类型名称
                let type_name = target_type.ty.to_string();
                let type_name_index = self.chunk.add_constant(Value::string(type_name));
                
                // 根据是否强制转换选择操作码
                let opcode = if *force {
                    OpCode::CastForce
                } else {
                    OpCode::CastSafe
                };
                
                self.chunk.write_op(opcode, span.line);
                self.chunk.write_u16(type_name_index, span.line);
            }
            Expr::TypeCheck { expr, check_type, span } => {
                // 编译要检查的表达式
                self.compile_expr(expr);
                
                // 获取类型名称
                let type_name = check_type.ty.to_string();
                let type_name_index = self.chunk.add_constant(Value::string(type_name));
                
                self.chunk.write_op(OpCode::TypeCheck, span.line);
                self.chunk.write_u16(type_name_index, span.line);
            }
            Expr::New { class_name, args, span } => {
                // 编译参数
                for arg in args {
                    self.compile_expr(arg);
                }
                
                // 生成 NewClass 指令
                let class_name_index = self.chunk.add_constant(Value::string(class_name.clone()));
                self.chunk.write_op(OpCode::NewClass, span.line);
                self.chunk.write_u16(class_name_index, span.line);
                self.chunk.write(args.len() as u8, span.line);
            }
            Expr::This { span } => {
                // this 被编译为局部变量 "this"（在方法中是第一个局部变量）
                if let Some(slot) = self.symbols.resolve_slot("this") {
                    self.chunk.write_get_local(slot, span.line);
                } else {
                    let msg = "'this' can only be used inside a method".to_string();
                self.errors.push(CompileError::new(msg, *span));
                }
            }
            Expr::Super { span } => {
                // super 不能单独使用，必须配合成员访问（super.method()）
                let msg = "'super' must be used with member access (e.g., super.method())".to_string();
                self.errors.push(CompileError::new(msg, *span));
            }
            Expr::Default { type_name, span } => {
                // default 初始化：创建类型的默认实例
                // 查找类型信息
                if let Some(_type_info) = self.chunk.get_type(type_name) {
                    // 创建新实例（调用无参构造函数或使用默认值）
                    let type_name_idx = self.chunk.add_constant(Value::string(type_name.clone()));
                    self.chunk.write_op(OpCode::NewClass, span.line);
                    self.chunk.write_u16(type_name_idx, span.line);
                    self.chunk.write(0, span.line);  // 0 个参数，使用默认值
                } else {
                    let msg = format!("Unknown type '{}' for default initialization", type_name);
                    self.errors.push(CompileError::new(msg, *span));
                }
            }
            Expr::StaticMember { class_name, member, span } => {
                // 静态成员访问: ClassName::CONST 或 ClassName::field 或 EnumName::Variant
                
                // 先检查是否是枚举变体访问
                if let Some(enum_info) = self.chunk.get_enum(class_name) {
                    let has_variant = enum_info.variants.iter().any(|v| v.name == *member);
                    if has_variant {
                        // 枚举变体访问
                        let enum_name_index = self.chunk.add_constant(Value::string(class_name.clone()));
                        let variant_name_index = self.chunk.add_constant(Value::string(member.clone()));
                        self.chunk.write_op(OpCode::GetStatic, span.line);
                        self.chunk.write_u16(enum_name_index, span.line);
                        self.chunk.write_u16(variant_name_index, span.line);
                        return;
                    } else {
                        let msg = format!("Enum '{}' has no variant '{}'", class_name, member);
                        self.errors.push(CompileError::new(msg, *span));
                        return;
                    }
                }
                
                // 检查类型信息是否存在，并获取是否有该静态字段
                let has_static_member = if let Some(type_info) = self.chunk.get_type(class_name) {
                    type_info.static_fields.contains_key(member)
                } else {
                    let msg = format!("Unknown class or enum '{}'", class_name);
                    self.errors.push(CompileError::new(msg, *span));
                    return;
                };
                
                if has_static_member {
                    // 静态字段访问
                    let class_name_index = self.chunk.add_constant(Value::string(class_name.clone()));
                    let field_name_index = self.chunk.add_constant(Value::string(member.clone()));
                    self.chunk.write_op(OpCode::GetStatic, span.line);
                    self.chunk.write_u16(class_name_index, span.line);
                    self.chunk.write_u16(field_name_index, span.line);
                } else {
                    let msg = format!("Class '{}' has no static member '{}'", class_name, member);
                    self.errors.push(CompileError::new(msg, *span));
                }
            }
            
            // go 表达式：启动协程
            Expr::Go { call, span } => {
                // 编译被调用的表达式（必须是一个 Call 表达式）
                if let Expr::Call { callee, args, .. } = call.as_ref() {
                    // 编译闭包/函数
                    self.compile_expr(callee);
                    
                    // 编译参数
                    for (_, arg) in args {
                        self.compile_expr(arg);
                    }
                    
                    // 生成 GoSpawn 指令
                    self.chunk.write_op(OpCode::GoSpawn, span.line);
                    self.chunk.write(args.len() as u8, span.line);
                    
                    // go 表达式返回 null（或者未来可以返回 JoinHandle）
                    self.chunk.write_constant(Value::null(), span.line);
                } else {
                    let msg = "go expression must be followed by a function call".to_string();
                    self.errors.push(CompileError::new(msg, *span));
                }
            }
        }
    }
    
    /// 尝试提取尾调用信息
    /// 如果表达式是一个简单的函数调用（不是方法调用），返回调用信息
    fn try_extract_tail_call(&self, expr: &Expr) -> Option<TailCallInfo> {
        match expr {
            Expr::Call { callee, args, .. } => {
                // 检查是否是简单的标识符调用（用户自定义函数）
                // 排除内置函数和方法调用
                match callee.as_ref() {
                    Expr::Identifier { name, .. } => {
                        // 排除内置函数
                        match name.as_str() {
                            "print" | "println" | "typeof" | "typeinfo" | "sizeof" | "panic" | "time" => None,
                            _ => Some(TailCallInfo {
                                callee: callee.as_ref().clone(),
                                args: args.iter().map(|(_, e)| e.clone()).collect(),
                            })
                        }
                    }
                    // 对于更复杂的 callee（如闭包表达式），也可以进行尾调用优化
                    Expr::Closure { .. } => Some(TailCallInfo {
                        callee: callee.as_ref().clone(),
                        args: args.iter().map(|(_, e)| e.clone()).collect(),
                    }),
                    _ => None, // 其他情况（方法调用、静态成员调用等）暂不优化
                }
            }
            _ => None,
        }
    }
    
    /// 尝试将常量表达式转换为运行时值
    /// 仅支持字面量（数字、字符串、布尔值、null）
    fn expr_to_value(&self, expr: &Expr) -> Result<Value, String> {
        match expr {
            Expr::Integer { value, .. } => Ok(Value::int(*value)),
            Expr::Float { value, .. } => Ok(Value::float(*value)),
            Expr::String { value, .. } => Ok(Value::string(value.clone())),
            Expr::Bool { value, .. } => Ok(Value::bool(*value)),
            Expr::Null { .. } => Ok(Value::null()),
            Expr::Char { value, .. } => Ok(Value::char(*value)),
            Expr::Unary { op, operand, .. } => {
                // 支持负数字面量，如 -1, -3.14
                let val = self.expr_to_value(operand)?;
                match op {
                    UnaryOp::Neg => {
                        if let Some(n) = val.as_int() {
                            Ok(Value::int(-n))
                        } else if let Some(f) = val.as_float() {
                            Ok(Value::float(-f))
                        } else {
                            Err("Cannot negate non-numeric default value".to_string())
                        }
                    }
                    UnaryOp::Not => {
                        Ok(Value::bool(!val.is_truthy()))
                    }
                    UnaryOp::BitNot => {
                        if let Some(n) = val.as_int() {
                            Ok(Value::int(!n))
                        } else {
                            Err("Cannot bitwise NOT non-integer default value".to_string())
                        }
                    }
                }
            }
            _ => Err("Default parameter value must be a constant expression".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Scanner;
    use crate::parser::Parser;
    use crate::compiler::OpCode;

    fn compile(source: &str) -> Result<Chunk, Vec<CompileError>> {
        let mut scanner = Scanner::new(source);
        let tokens = scanner.scan_tokens();
        let mut parser = Parser::new(tokens, Locale::En);
        let program = parser.parse().unwrap();
        let mut compiler = Compiler::new(Locale::En);
        compiler.compile(&program)
    }

    #[test]
    fn test_compile_integer() {
        let chunk = compile("123").unwrap();
        // 小整数 (-128 到 127) 使用 ConstInt8 指令，不存储在常量池
        assert_eq!(chunk.constants.len(), 0);
        // 检查生成了 ConstInt8 指令
        assert!(chunk.code.contains(&(OpCode::ConstInt8 as u8)));
    }

    #[test]
    fn test_compile_binary() {
        let chunk = compile("1 + 2").unwrap();
        // 小整数使用 ConstInt8 指令，不存储在常量池
        assert_eq!(chunk.constants.len(), 0);
        // 检查生成了 ConstInt8 指令
        assert!(chunk.code.contains(&(OpCode::ConstInt8 as u8)));
    }

    #[test]
    fn test_compile_print() {
        let chunk = compile("print(42)").unwrap();
        // 小整数使用 ConstInt8 指令，不存储在常量池
        assert_eq!(chunk.constants.len(), 0);
        // 检查生成了 ConstInt8 指令
        assert!(chunk.code.contains(&(OpCode::ConstInt8 as u8)));
    }
    
    #[test]
    fn test_compile_large_integer() {
        let chunk = compile("1000").unwrap();
        // 大整数存储在常量池
        assert_eq!(chunk.constants.len(), 1);
        assert_eq!(chunk.constants[0].as_int(), Some(1000));
    }
}
