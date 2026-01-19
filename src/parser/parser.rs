//! 语法解析器
//! 
//! 使用递归下降法将 Token 流解析为 AST

use crate::lexer::{Token, TokenKind, Span};
use crate::i18n::{Locale, format_message, messages};
use super::ast::{Expr, Stmt, Program, BinOp, UnaryOp, AssignOp, TypeAnnotation, FnParam, ImportDecl, ImportTarget};
use crate::types::Type;

/// 运算符优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)]
enum Precedence {
    None,
    Or,         // ||
    And,        // &&
    Equality,   // == !=
    Comparison, // < > <= >=
    Term,       // + -
    Factor,     // * / %
    Power,      // **
    Unary,      // ! -
    Call,       // () []
    Primary,
}

/// 解析错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// 意外的 token
    UnexpectedToken,
    /// 期望的 token 未找到
    ExpectedToken,
    /// 意外的文件结束
    UnexpectedEof,
    /// 无效的语法
    InvalidSyntax,
    /// 无效的表达式
    InvalidExpression,
    /// 重复定义
    DuplicateDefinition,
    /// 无效的赋值目标
    InvalidAssignmentTarget,
    /// 无效的类型
    InvalidType,
}

/// 解析错误
#[derive(Debug, Clone)]
pub struct ParseError {
    /// 错误消息
    pub message: String,
    /// 错误位置
    pub span: Span,
    /// 错误类型
    pub kind: ParseErrorKind,
    /// 期望的内容（如果有）
    pub expected: Option<String>,
    /// 实际得到的内容
    pub found: Option<String>,
    /// 修复建议
    pub hint: Option<String>,
}

impl ParseError {
    /// 创建基本错误
    fn new(message: String, span: Span) -> Self {
        Self { 
            message, 
            span,
            kind: ParseErrorKind::InvalidSyntax,
            expected: None,
            found: None,
            hint: None,
        }
    }
    
    /// 创建带类型的错误
    fn with_kind(message: String, span: Span, kind: ParseErrorKind) -> Self {
        Self {
            message,
            span,
            kind,
            expected: None,
            found: None,
            hint: None,
        }
    }
    
    /// 创建期望 token 错误
    fn expected_token(expected: &str, found: &str, span: Span) -> Self {
        Self {
            message: format!("Expected '{}', but found '{}'", expected, found),
            span,
            kind: ParseErrorKind::ExpectedToken,
            expected: Some(expected.to_string()),
            found: Some(found.to_string()),
            hint: None,
        }
    }
    
    /// 创建意外 token 错误
    fn unexpected_token(found: &str, context: &str, span: Span) -> Self {
        Self {
            message: format!("Unexpected '{}' {}", found, context),
            span,
            kind: ParseErrorKind::UnexpectedToken,
            expected: None,
            found: Some(found.to_string()),
            hint: None,
        }
    }
    
    /// 创建意外 EOF 错误
    fn unexpected_eof(context: &str, span: Span) -> Self {
        Self {
            message: format!("Unexpected end of file {}", context),
            span,
            kind: ParseErrorKind::UnexpectedEof,
            expected: None,
            found: Some("EOF".to_string()),
            hint: None,
        }
    }
    
    /// 添加修复建议
    fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
    
    /// 格式化完整错误消息
    pub fn format(&self) -> String {
        let mut msg = self.message.clone();
        if let Some(hint) = &self.hint {
            msg.push_str(&format!("\n  Hint: {}", hint));
        }
        msg
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format())
    }
}

/// 语法解析器
pub struct Parser {
    /// Token 列表
    tokens: Vec<Token>,
    /// 当前位置
    current: usize,
    /// 错误列表
    errors: Vec<ParseError>,
    /// 当前语言
    locale: Locale,
    /// 恐慌模式（遇到错误后进入，用于错误恢复）
    panic_mode: bool,
}

impl Parser {
    /// 创建新的解析器
    pub fn new(tokens: Vec<Token>, locale: Locale) -> Self {
        Self {
            tokens,
            current: 0,
            errors: Vec::new(),
            locale,
            panic_mode: false,
        }
    }

    /// 解析程序
    pub fn parse(&mut self) -> Result<Program, Vec<ParseError>> {
        let mut package: Option<String> = None;
        let mut imports: Vec<ImportDecl> = Vec::new();
        let mut statements = Vec::new();
        
        // 跳过开头的空行
        while self.check(&TokenKind::Newline) {
            self.advance();
        }
        
        // 解析可选的包声明（必须在文件开头）
        if self.check(&TokenKind::Package) {
            match self.parse_package_declaration() {
                Ok(pkg) => package = Some(pkg),
                Err(e) => self.errors.push(e),
            }
        }
        
        // 解析所有 import 声明
        loop {
            // 跳过空行
            while self.check(&TokenKind::Newline) {
                self.advance();
            }
            
            if self.is_at_end() {
                break;
            }
            
            // 检查是否是 import
            if self.check(&TokenKind::Import) {
                match self.parse_import_declaration() {
                    Ok(import) => imports.push(import),
                    Err(e) => {
                        self.errors.push(e);
                        self.synchronize();
                    }
                }
            } else {
                break;
            }
        }
        
        // 解析其余语句
        while !self.is_at_end() {
            // 跳过空行
            while self.check(&TokenKind::Newline) {
                self.advance();
            }
            
            if self.is_at_end() {
                break;
            }
            
            match self.parse_statement() {
                Ok(stmt) => statements.push(stmt),
                Err(e) => {
                    self.errors.push(e);
                    self.synchronize();
                }
            }
        }
        
        if self.errors.is_empty() {
            Ok(Program::with_package_and_imports(package, imports, statements))
        } else {
            Err(self.errors.clone())
        }
    }
    
    /// 解析包声明
    /// package com.example.demo
    fn parse_package_declaration(&mut self) -> Result<String, ParseError> {
        self.advance(); // 消费 'package'
        
        // 解析包路径
        let path = self.parse_dotted_name()?;
        
        // 可选的换行或分号
        if self.check(&TokenKind::Newline) || self.check(&TokenKind::Semicolon) {
            self.advance();
        }
        
        Ok(path)
    }
    
    /// 解析导入声明
    /// import com.example.services.*
    /// import com.example.models.UserModel
    /// import com.example.models.{User, Product}
    fn parse_import_declaration(&mut self) -> Result<ImportDecl, ParseError> {
        self.advance(); // 消费 'import'
        
        // 解析导入路径（点分隔）
        let mut parts = Vec::new();
        parts.push(self.expect_identifier()?);
        
        while self.check(&TokenKind::Dot) {
            self.advance(); // 消费 '.'
            
            // 检查是否是通配符 *
            if self.check(&TokenKind::Star) {
                self.advance();
                // 可选的换行或分号
                if self.check(&TokenKind::Newline) || self.check(&TokenKind::Semicolon) {
                    self.advance();
                }
                return Ok(ImportDecl {
                    path: parts.join("."),
                    target: ImportTarget::All,
                });
            }
            
            // 检查是否是多成员导入 { A, B, C }
            if self.check(&TokenKind::LeftBrace) {
                self.advance(); // 消费 '{'
                let mut members = Vec::new();
                
                // 跳过换行
                while self.check(&TokenKind::Newline) {
                    self.advance();
                }
                
                if !self.check(&TokenKind::RightBrace) {
                    members.push(self.expect_identifier()?);
                    
                    while self.check(&TokenKind::Comma) {
                        self.advance();
                        // 跳过换行
                        while self.check(&TokenKind::Newline) {
                            self.advance();
                        }
                        if self.check(&TokenKind::RightBrace) {
                            break; // 允许末尾逗号
                        }
                        members.push(self.expect_identifier()?);
                    }
                }
                
                // 跳过换行
                while self.check(&TokenKind::Newline) {
                    self.advance();
                }
                
                self.expect(&TokenKind::RightBrace)?;
                
                // 可选的换行或分号
                if self.check(&TokenKind::Newline) || self.check(&TokenKind::Semicolon) {
                    self.advance();
                }
                
                return Ok(ImportDecl {
                    path: parts.join("."),
                    target: ImportTarget::Multiple(members),
                });
            }
            
            // 普通标识符
            parts.push(self.expect_identifier()?);
        }
        
        // 单个成员导入：最后一个部分是成员名
        if parts.len() < 2 {
            return Err(ParseError::new(
                "Import path must have at least two parts".to_string(),
                self.current_span(),
            ));
        }
        
        let member = parts.pop().unwrap();
        let path = parts.join(".");
        
        // 可选的换行或分号
        if self.check(&TokenKind::Newline) || self.check(&TokenKind::Semicolon) {
            self.advance();
        }
        
        Ok(ImportDecl {
            path,
            target: ImportTarget::Single(member),
        })
    }
    
    /// 解析点分隔的名称，如 com.example.demo
    fn parse_dotted_name(&mut self) -> Result<String, ParseError> {
        let mut parts = Vec::new();
        parts.push(self.expect_identifier()?);
        
        while self.check(&TokenKind::Dot) {
            self.advance();
            parts.push(self.expect_identifier()?);
        }
        
        Ok(parts.join("."))
    }

    /// 解析语句
    fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
        // 检查是否是 print/println 语句
        if self.check_identifier("print") {
            return self.parse_print_statement(false);
        }
        if self.check_identifier("println") {
            return self.parse_print_statement(true);
        }
        
        // 检查变量声明
        if self.check(&TokenKind::Var) {
            return self.parse_var_declaration();
        }
        
        // 检查常量声明
        if self.check(&TokenKind::Const) {
            return self.parse_const_declaration();
        }
        
        // 检查块语句
        if self.check(&TokenKind::LeftBrace) {
            return self.parse_block();
        }
        
        // 检查 if 语句
        if self.check(&TokenKind::If) {
            return self.parse_if_statement();
        }
        
        // 检查带标签的循环（label: for ...）
        if let TokenKind::Identifier(name) = &self.current_token().kind.clone() {
            // 检查是否是 label: for 语法
            if self.peek_token().map(|t| &t.kind) == Some(&TokenKind::Colon) {
                let label = name.clone();
                self.advance(); // 消费标识符
                self.advance(); // 消费冒号
                if self.check(&TokenKind::For) {
                    return self.parse_for_statement_with_label(Some(label));
                }
                // 不是 for，报错
                let msg = "Label must be followed by 'for'".to_string();
                return Err(ParseError::new(msg, self.current_span()));
            }
        }
        
        // 检查 for 循环
        if self.check(&TokenKind::For) {
            return self.parse_for_statement_with_label(None);
        }
        
        // 检查 break
        if self.check(&TokenKind::Break) {
            return self.parse_break_statement();
        }
        
        // 检查 continue
        if self.check(&TokenKind::Continue) {
            return self.parse_continue_statement();
        }
        
        // 检查 return
        if self.check(&TokenKind::Return) {
            return self.parse_return_statement();
        }
        
        // 检查 struct 定义
        if self.check(&TokenKind::Struct) {
            return self.parse_struct_definition();
        }
        
        // 检查 abstract class 定义
        if self.check(&TokenKind::Abstract) {
            self.advance(); // 消费 'abstract'
            if self.check(&TokenKind::Class) {
                return self.parse_class_definition(true);
            } else {
                let msg = "Expected 'class' after 'abstract'".to_string();
                return Err(ParseError::new(msg, self.current_span()));
            }
        }
        
        // 检查 class 定义
        if self.check(&TokenKind::Class) {
            return self.parse_class_definition(false);
        }
        
        // 检查 interface 定义
        if self.check(&TokenKind::Interface) {
            return self.parse_interface_definition();
        }
        
        // 检查 trait 定义
        if self.check(&TokenKind::Trait) {
            return self.parse_trait_definition();
        }
        
        // 检查 enum 定义
        if self.check(&TokenKind::Enum) {
            return self.parse_enum_definition();
        }
        
        // 检查 type 别名
        if self.check(&TokenKind::Type) {
            return self.parse_type_alias();
        }
        
        // 检查可见性修饰符 + 函数定义
        let visibility = if self.check(&TokenKind::Public) || self.check(&TokenKind::Internal) 
                           || self.check(&TokenKind::Private) || self.check(&TokenKind::Protected) {
            let vis = self.parse_visibility();
            // 可见性后面必须跟 func
            if !self.check(&TokenKind::Func) {
                return Err(ParseError::new(
                    format!("Visibility modifier must be followed by 'func' keyword"),
                    self.current_span(),
                ));
            }
            vis
        } else if self.check(&TokenKind::Func) {
            super::ast::Visibility::default()
        } else {
            // 不是函数定义，继续其他检查
            super::ast::Visibility::default()
        };
        
        // 检查命名函数定义 func name(params) return_type { }
        if self.check(&TokenKind::Func) {
            return self.parse_named_function_with_visibility(visibility);
        }
        
        // 检查 match 语句
        if self.check(&TokenKind::Match) {
            return self.parse_match_statement();
        }
        
        // 检查 try 语句
        if self.check(&TokenKind::Try) {
            return self.parse_try_statement();
        }
        
        // 检查 throw 语句
        if self.check(&TokenKind::Throw) {
            return self.parse_throw_statement();
        }
        
        // 否则是表达式语句
        self.parse_expression_statement()
    }

    /// 解析 print/println 语句
    fn parse_print_statement(&mut self, newline: bool) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'print' 或 'println'
        
        self.expect(&TokenKind::LeftParen)?;
        let expr = self.parse_expression()?;
        self.expect(&TokenKind::RightParen)?;
        
        // 可选的换行或分号
        if self.check(&TokenKind::Newline) || self.check(&TokenKind::Semicolon) {
            self.advance();
        }
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::Print { expr, newline, span })
    }
    
    /// 解析变量声明
    fn parse_var_declaration(&mut self) -> Result<Stmt, ParseError> {
        let stmt = self.parse_var_declaration_no_terminator()?;
        
        // 可选的换行或分号
        if self.check(&TokenKind::Newline) || self.check(&TokenKind::Semicolon) {
            self.advance();
        }
        
        Ok(stmt)
    }
    
    /// 解析变量声明（不消费终结符，用于 for 循环初始化）
    fn parse_var_declaration_no_terminator(&mut self) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'var'
        
        // 变量名
        let name = self.expect_identifier()?;
        
        // 可选的类型注解
        let type_ann = if self.check(&TokenKind::Colon) {
            self.advance();
            Some(self.parse_type_annotation()?)
        } else {
            None
        };
        
        // 可选的初始化表达式
        let initializer = if self.check(&TokenKind::Equal) {
            self.advance();
            // 检查是否是 default 初始化
            if self.check(&TokenKind::Default) {
                let default_span = self.current_span();
                self.advance();
                // default 初始化需要类型注解
                if let Some(ref ta) = type_ann {
                    // 从类型注解中获取类型名
                    let type_name = match &ta.ty {
                        crate::types::Type::Class(name) => name.clone(),
                        _ => {
                            let msg = "'default' can only be used with class/struct types".to_string();
                            return Err(ParseError::new(msg, default_span));
                        }
                    };
                    Some(Expr::Default { 
                        type_name, 
                        span: default_span 
                    })
                } else {
                    let msg = "'default' initialization requires type annotation".to_string();
                    return Err(ParseError::new(msg, default_span));
                }
            } else {
                Some(self.parse_expression()?)
            }
        } else {
            None
        };
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::VarDecl { name, type_ann, initializer, span })
    }
    
    /// 解析常量声明
    fn parse_const_declaration(&mut self) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'const'
        
        // 常量名
        let name = self.expect_identifier()?;
        
        // 可选的类型注解
        let type_ann = if self.check(&TokenKind::Colon) {
            self.advance();
            Some(self.parse_type_annotation()?)
        } else {
            None
        };
        
        // 必须有初始化表达式
        self.expect(&TokenKind::Equal)?;
        let initializer = self.parse_expression()?;
        
        // 可选的换行或分号
        if self.check(&TokenKind::Newline) || self.check(&TokenKind::Semicolon) {
            self.advance();
        }
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::ConstDecl { name, type_ann, initializer, span })
    }
    
    /// 解析块语句
    fn parse_block(&mut self) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.expect(&TokenKind::LeftBrace)?;
        
        let mut statements = Vec::new();
        
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            // 跳过空行
            while self.check(&TokenKind::Newline) {
                self.advance();
            }
            
            if self.check(&TokenKind::RightBrace) {
                break;
            }
            
            statements.push(self.parse_statement()?);
        }
        
        self.expect(&TokenKind::RightBrace)?;
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::Block { statements, span })
    }
    
    /// 解析 if 语句
    fn parse_if_statement(&mut self) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'if'
        
        // 条件表达式
        let condition = self.parse_expression()?;
        
        // then 分支（必须是块）
        let then_branch = Box::new(self.parse_block()?);
        
        // else 分支（可选）
        let else_branch = if self.check(&TokenKind::Else) {
            self.advance();
            if self.check(&TokenKind::If) {
                // else if
                Some(Box::new(self.parse_if_statement()?))
            } else {
                // else
                Some(Box::new(self.parse_block()?))
            }
        } else {
            None
        };
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::If { condition, then_branch, else_branch, span })
    }
    
    /// 解析 for 语句
    fn parse_for_statement_with_label(&mut self, label: Option<String>) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'for'
        
        // 检查是否是无限循环 for {}
        if self.check(&TokenKind::LeftBrace) {
            let body = Box::new(self.parse_block()?);
            let end_span = self.previous_span();
            let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
            return Ok(Stmt::While { label, condition: None, body, span });
        }
        
        // 尝试解析 C 风格 for 循环 (for init; cond; post {})
        // 首先，我们需要检查是否存在分号来判断是否是 C 风格
        // 但这需要 lookahead，所以我们尝试解析然后检查
        
        // 保存当前位置用于回溯
        let saved_pos = self.current;
        
        // 尝试解析初始化部分（可能是 var 声明或表达式）
        let initializer = if self.check(&TokenKind::Semicolon) {
            None
        } else if self.check(&TokenKind::Var) {
            // 在 for 循环中解析变量声明，但不消费分号
            Some(Box::new(self.parse_var_declaration_no_terminator()?))
        } else {
            // 尝试解析表达式
            let expr = self.parse_expression()?;
            
            // 如果后面是分号，说明是 C 风格 for 循环的初始化部分
            if self.check(&TokenKind::Semicolon) {
                Some(Box::new(Stmt::Expression { 
                    expr: expr.clone(), 
                    span: expr.span() 
                }))
            } else if self.check(&TokenKind::LeftBrace) {
                // 这是条件循环 for condition {}
                let body = Box::new(self.parse_block()?);
                let end_span = self.previous_span();
                let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
                return Ok(Stmt::While { label: label.clone(), condition: Some(expr), body, span });
            } else if self.check(&TokenKind::In) {
                // 这是 for-in 循环，暂时回溯
                self.current = saved_pos;
                return self.parse_for_in_statement(start_span, label.clone());
            } else {
                // 意外的 token
                let msg = format!("Expected '{{' or ';' after for condition");
                return Err(ParseError::new(msg, self.current_span()));
            }
        };
        
        // 期望分号（结束初始化部分）
        self.expect(&TokenKind::Semicolon)?;
        
        // 解析条件部分（可选）
        let condition = if self.check(&TokenKind::Semicolon) {
            None
        } else {
            Some(self.parse_expression()?)
        };
        
        // 期望分号（结束条件部分）
        self.expect(&TokenKind::Semicolon)?;
        
        // 解析递增部分（可选）
        let increment = if self.check(&TokenKind::LeftBrace) {
            None
        } else {
            Some(self.parse_expression()?)
        };
        
        // 解析循环体
        let body = Box::new(self.parse_block()?);
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::ForLoop { label, initializer, condition, increment, body, span })
    }
    
    /// 解析 for-in 循环
    fn parse_for_in_statement(&mut self, start_span: Span, label: Option<String>) -> Result<Stmt, ParseError> {
        // 解析变量名（可能有多个，如 for i, v in array）
        let mut variables = Vec::new();
        let first_var = self.expect_identifier()?;
        variables.push(first_var);
        
        while self.check(&TokenKind::Comma) {
            self.advance();
            let var = self.expect_identifier()?;
            variables.push(var);
        }
        
        // 期望 'in' 关键字
        self.expect(&TokenKind::In)?;
        
        // 解析可迭代表达式
        let iterable = self.parse_expression()?;
        
        // 解析循环体
        let body = Box::new(self.parse_block()?);
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::ForIn { label, variables, iterable, body, span })
    }
    
    /// 解析 struct 定义
    fn parse_struct_definition(&mut self) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'struct'
        
        // 结构体名称
        let name = self.expect_identifier()?;
        
        // 解析可选的泛型类型参数 <T, K, V>
        let type_params = self.parse_type_params()?;
        
        // 解析可选的 implements 子句
        let mut interfaces = Vec::new();
        if self.check(&TokenKind::Implements) {
            self.advance();
            interfaces.push(self.expect_identifier()?);
            while self.check(&TokenKind::Comma) {
                self.advance();
                interfaces.push(self.expect_identifier()?);
            }
        }
        
        // 期望 '{'
        self.expect(&TokenKind::LeftBrace)?;
        
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        
        // 解析字段和方法
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            // 跳过空行
            while self.check(&TokenKind::Newline) {
                self.advance();
            }
            
            if self.check(&TokenKind::RightBrace) {
                break;
            }
            
            // 检查可见性修饰符
            let visibility = self.parse_visibility();
            
            // 检查是否是方法（func 关键字）
            if self.check(&TokenKind::Func) {
                let method = self.parse_struct_method(visibility)?;
                methods.push(method);
            } else {
                // 解析字段
                let field = self.parse_struct_field(visibility)?;
                fields.push(field);
            }
        }
        
        // 期望 '}'
        self.expect(&TokenKind::RightBrace)?;
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::StructDef { name, type_params, where_clauses: Vec::new(), interfaces, fields, methods, span })
    }
    
    /// 解析可见性修饰符（Kotlin 风格，默认为 public）
    fn parse_visibility(&mut self) -> super::ast::Visibility {
        use super::ast::Visibility;
        
        if self.check(&TokenKind::Public) {
            self.advance();
            Visibility::Public
        } else if self.check(&TokenKind::Internal) {
            self.advance();
            Visibility::Internal
        } else if self.check(&TokenKind::Private) {
            self.advance();
            Visibility::Private
        } else if self.check(&TokenKind::Protected) {
            self.advance();
            Visibility::Protected
        } else {
            // 默认为 public（Kotlin 风格）
            Visibility::default()
        }
    }
    
    /// 解析泛型类型参数列表 <T, K: Comparable<K>, V = string>
    fn parse_type_params(&mut self) -> Result<Vec<super::ast::TypeParam>, ParseError> {
        use super::ast::TypeParam;
        use crate::types::TypeBound;
        
        let mut params = Vec::new();
        
        // 检查是否有 '<'
        if !self.check(&TokenKind::Less) {
            return Ok(params);
        }
        
        self.advance(); // 消费 '<'
        
        // 跳过空行
        while self.check(&TokenKind::Newline) {
            self.advance();
        }
        
        // 解析第一个类型参数
        if !self.check(&TokenKind::Greater) {
            let param = self.parse_single_type_param()?;
            params.push(param);
            
            // 解析更多参数
            while self.check(&TokenKind::Comma) {
                self.advance();
                
                // 跳过空行
                while self.check(&TokenKind::Newline) {
                    self.advance();
                }
                
                if self.check(&TokenKind::Greater) {
                    break; // 允许末尾逗号
                }
                
                let param = self.parse_single_type_param()?;
                params.push(param);
            }
        }
        
        // 期望 '>'
        self.expect(&TokenKind::Greater)?;
        
        Ok(params)
    }
    
    /// 解析单个类型参数（支持约束和默认类型）
    /// 格式: T, T: Bound, T: Bound1 + Bound2, T = DefaultType
    fn parse_single_type_param(&mut self) -> Result<super::ast::TypeParam, ParseError> {
        use crate::types::TypeBound;
        
        let param_span = self.current_span();
        let param_name = self.expect_identifier()?;
        
        // 解析约束 T: Bound1 + Bound2
        let bounds = if self.check(&TokenKind::Colon) {
            self.advance();
            self.parse_type_bounds()?
        } else {
            Vec::new()
        };
        
        // 解析默认类型 T = DefaultType
        let default_type = if self.check(&TokenKind::Equal) {
            self.advance();
            Some(self.parse_type_annotation()?)
        } else {
            None
        };
        
        Ok(super::ast::TypeParam {
            name: param_name,
            bounds,
            default_type,
            span: param_span,
        })
    }
    
    /// 解析类型约束列表（用 + 连接）
    /// 格式: Comparable<T> + Printable + Clone
    fn parse_type_bounds(&mut self) -> Result<Vec<crate::types::TypeBound>, ParseError> {
        use crate::types::TypeBound;
        
        let mut bounds = Vec::new();
        
        // 解析第一个约束
        bounds.push(self.parse_single_type_bound()?);
        
        // 解析更多约束（用 + 连接）
        while self.check(&TokenKind::Plus) {
            self.advance();
            bounds.push(self.parse_single_type_bound()?);
        }
        
        Ok(bounds)
    }
    
    /// 解析单个类型约束
    /// 格式: Comparable 或 Comparable<T>
    fn parse_single_type_bound(&mut self) -> Result<crate::types::TypeBound, ParseError> {
        use crate::types::TypeBound;
        
        let trait_name = self.expect_identifier()?;
        
        // 检查是否有泛型参数
        let type_args = if self.check(&TokenKind::Less) {
            self.advance(); // 消费 '<'
            let args = self.parse_type_args()?;
            self.expect(&TokenKind::Greater)?;
            args
        } else {
            Vec::new()
        };
        
        Ok(TypeBound {
            trait_name,
            type_args,
        })
    }
    
    /// 解析 where 子句
    /// 格式: where T: Comparable<T>, U: Printable
    fn parse_where_clauses(&mut self) -> Result<Vec<super::ast::WhereClause>, ParseError> {
        use super::ast::WhereClause;
        
        let mut clauses = Vec::new();
        
        // 检查是否有 'where' 关键字
        if !self.check(&TokenKind::Where) {
            return Ok(clauses);
        }
        
        self.advance(); // 消费 'where'
        
        // 跳过空行
        while self.check(&TokenKind::Newline) {
            self.advance();
        }
        
        // 解析第一个约束
        let clause = self.parse_single_where_clause()?;
        clauses.push(clause);
        
        // 解析更多约束（用逗号分隔）
        while self.check(&TokenKind::Comma) {
            self.advance();
            
            // 跳过空行
            while self.check(&TokenKind::Newline) {
                self.advance();
            }
            
            // 检查是否是新语句开始（{ 或其他）
            if self.check(&TokenKind::LeftBrace) || self.check(&TokenKind::LeftParen) {
                break;
            }
            
            let clause = self.parse_single_where_clause()?;
            clauses.push(clause);
        }
        
        Ok(clauses)
    }
    
    /// 解析单个 where 子句约束
    /// 格式: T: Comparable<T> + Printable
    fn parse_single_where_clause(&mut self) -> Result<super::ast::WhereClause, ParseError> {
        let span = self.current_span();
        let type_param = self.expect_identifier()?;
        
        self.expect(&TokenKind::Colon)?;
        
        let bounds = self.parse_type_bounds()?;
        
        Ok(super::ast::WhereClause {
            type_param,
            bounds,
            span,
        })
    }
    
    /// 解析 struct 字段
    fn parse_struct_field(&mut self, visibility: super::ast::Visibility) -> Result<super::ast::StructField, ParseError> {
        let start_span = self.current_span();
        
        // 字段名
        let name = self.expect_identifier()?;
        
        // 期望 ':'
        self.expect(&TokenKind::Colon)?;
        
        // 类型注解
        let type_ann = self.parse_type_annotation()?;
        
        // 可选的换行或分号
        if self.check(&TokenKind::Newline) || self.check(&TokenKind::Semicolon) {
            self.advance();
        }
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(super::ast::StructField { name, type_ann, visibility, span })
    }
    
    /// 解析 struct 方法
    fn parse_struct_method(&mut self, visibility: super::ast::Visibility) -> Result<super::ast::StructMethod, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'func'
        
        // 方法名
        let name = self.expect_identifier()?;
        
        // 参数列表（struct 方法不允许字段修饰符）
        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_fn_params(false)?;
        self.expect(&TokenKind::RightParen)?;
        
        // 返回类型（可选）
        let return_type = if !self.check(&TokenKind::LeftBrace) && !self.check(&TokenKind::Newline) {
            Some(self.parse_type_annotation()?)
        } else {
            None
        };
        
        // 方法体
        let body = Box::new(self.parse_block()?);
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(super::ast::StructMethod { name, params, return_type, body, visibility, span })
    }
    
    /// 解析 class 定义
    fn parse_class_definition(&mut self, is_abstract: bool) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'class'
        
        // 类名
        let name = self.expect_identifier()?;
        
        // 解析可选的泛型类型参数 <T, K, V>
        let type_params = self.parse_type_params()?;
        
        // 可选的父类
        let parent = if self.check(&TokenKind::Extends) {
            self.advance();
            Some(self.expect_identifier()?)
        } else {
            None
        };
        
        // 可选的接口列表
        let mut interfaces = Vec::new();
        if self.check(&TokenKind::Implements) {
            self.advance();
            interfaces.push(self.expect_identifier()?);
            while self.check(&TokenKind::Comma) {
                self.advance();
                interfaces.push(self.expect_identifier()?);
            }
        }
        
        // 期望 '{'
        self.expect(&TokenKind::LeftBrace)?;
        
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        let mut traits = Vec::new();
        
        // 解析字段、方法和 use trait
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            // 跳过空行
            while self.check(&TokenKind::Newline) {
                self.advance();
            }
            
            if self.check(&TokenKind::RightBrace) {
                break;
            }
            
            // 检查 use trait 语法
            if self.check(&TokenKind::Use) {
                self.advance();
                let trait_name = self.expect_identifier()?;
                traits.push(trait_name);
                // 跳过可选的分号或换行
                while self.check(&TokenKind::Semicolon) || self.check(&TokenKind::Newline) {
                    self.advance();
                }
                continue;
            }
            
            // 检查可见性修饰符
            let visibility = self.parse_visibility();
            
            // 检查 static
            let is_static = if self.check(&TokenKind::Static) {
                self.advance();
                true
            } else {
                false
            };
            
            // 检查 const (用于 static const 字段)
            let is_const = if self.check(&TokenKind::Const) {
                if !is_static {
                    let msg = "'const' can only be used with 'static' (use 'static const')".to_string();
                    return Err(ParseError::new(msg, self.current_span()));
                }
                self.advance();
                true
            } else {
                false
            };
            
            // 检查 override
            let is_override = if self.check(&TokenKind::Override) {
                self.advance();
                true
            } else {
                false
            };
            
            // 检查 abstract（仅在抽象类中允许）
            let is_method_abstract = if self.check(&TokenKind::Abstract) {
                if !is_abstract {
                    let msg = "Abstract methods can only be declared in abstract classes".to_string();
                    return Err(ParseError::new(msg, self.current_span()));
                }
                self.advance();
                true
            } else {
                false
            };
            
            // 检查是否是方法（func 关键字，包括构造函数 func init()）
            if self.check(&TokenKind::Func) {
                let method = self.parse_class_method(visibility, is_static, is_override, is_method_abstract)?;
                methods.push(method);
            } else if self.check(&TokenKind::Var) || self.check(&TokenKind::Const) {
                // 解析字段（必须有 var 或 const 关键字）
                let is_const_field = self.check(&TokenKind::Const);
                self.advance(); // 消费 var 或 const
                let field = self.parse_class_field(visibility, is_static, is_const_field || is_const)?;
                fields.push(field);
            } else {
                // 不是 func、var、const，报错
                let msg = format_message(messages::ERR_COMPILE_EXPECTED_VAR_OR_FUNC, self.locale, &[]);
                return Err(ParseError::new(msg, self.current_span()));
            }
        }
        
        // 期望 '}'
        self.expect(&TokenKind::RightBrace)?;
        
        // 检查是否有多个构造函数（禁止构造函数重载）
        let init_methods: Vec<_> = methods.iter().filter(|m| m.name == "init").collect();
        if init_methods.len() > 1 {
            let msg = format_message(messages::ERR_COMPILE_CONSTRUCTOR_OVERLOAD, self.locale, &[]);
            return Err(ParseError::new(msg, init_methods[1].span));
        }
        
        // 检查构造函数可见性（必须是 public）
        if let Some(init_method) = init_methods.first() {
            if init_method.visibility != super::ast::Visibility::Public {
                let msg = format_message(messages::ERR_COMPILE_CONSTRUCTOR_VISIBILITY, self.locale, &[]);
                return Err(ParseError::new(msg, init_method.span));
            }
        }
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::ClassDef { name, type_params, where_clauses: Vec::new(), is_abstract, parent, interfaces, traits, fields, methods, span })
    }
    
    /// 解析 class 字段
    fn parse_class_field(&mut self, visibility: super::ast::Visibility, is_static: bool, is_const: bool) -> Result<super::ast::ClassField, ParseError> {
        let start_span = self.current_span();
        
        // 字段名
        let name = self.expect_identifier()?;
        
        // 可选的类型注解
        let type_ann = if self.check(&TokenKind::Colon) {
            self.advance();
            Some(self.parse_type_annotation()?)
        } else {
            None
        };
        
        // 可选的初始化表达式
        let initializer = if self.check(&TokenKind::Equal) {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };
        
        // const 字段必须有初始值
        if is_const && initializer.is_none() {
            let msg = "Const field must be initialized".to_string();
            return Err(ParseError::new(msg, start_span));
        }
        
        // 可选的换行或分号
        if self.check(&TokenKind::Newline) || self.check(&TokenKind::Semicolon) {
            self.advance();
        }
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(super::ast::ClassField { name, type_ann, initializer, visibility, is_static, is_const, span })
    }
    
    /// 解析 class 方法
    fn parse_class_method(&mut self, visibility: super::ast::Visibility, is_static: bool, is_override: bool, is_abstract: bool) -> Result<super::ast::ClassMethod, ParseError> {
        let start_span = self.current_span();
        
        // 必须有 func 关键字
        if !self.check(&TokenKind::Func) {
            let msg = "Expected 'func' keyword".to_string();
            return Err(ParseError::new(msg, self.current_span()));
        }
        self.advance(); // 消费 'func'
        
        // 方法名（包括 init 构造函数）
        let name = self.expect_identifier()?;
        
        // 抽象方法不能是构造函数
        if is_abstract && name == "init" {
            let msg = "Constructor cannot be abstract".to_string();
            return Err(ParseError::new(msg, self.current_span()));
        }
        
        // 参数列表（init 方法允许字段修饰符 var/val/const）
        let is_init = name == "init";
        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_fn_params(is_init)?;
        self.expect(&TokenKind::RightParen)?;
        
        // 返回类型（可选，init 构造函数没有返回类型）
        let return_type = if name == "init" {
            // 构造函数不能有返回类型
            // 检查用户是否错误地添加了返回类型
            if !self.check(&TokenKind::LeftBrace) && !self.check(&TokenKind::Newline) && !self.check(&TokenKind::Semicolon) {
                let msg = format_message(messages::ERR_COMPILE_CONSTRUCTOR_RETURN, self.locale, &[]);
                return Err(ParseError::new(msg, self.current_span()));
            }
            None
        } else if !self.check(&TokenKind::LeftBrace) && !self.check(&TokenKind::Newline) && !self.check(&TokenKind::Semicolon) {
            Some(self.parse_type_annotation()?)
        } else {
            None
        };
        
        // 方法体（抽象方法没有方法体）
        let body = if is_abstract {
            // 抽象方法以分号或换行结束
            if self.check(&TokenKind::Semicolon) || self.check(&TokenKind::Newline) {
                self.advance();
            }
            None
        } else {
            Some(Box::new(self.parse_block()?))
        };
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(super::ast::ClassMethod { name, params, return_type, body, visibility, is_static, is_override, is_abstract, span })
    }
    
    /// 解析 interface 定义
    fn parse_interface_definition(&mut self) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'interface'
        
        // 接口名称
        let name = self.expect_identifier()?;
        
        // 期望 '{'
        self.expect(&TokenKind::LeftBrace)?;
        
        let mut methods = Vec::new();
        
        // 解析方法签名
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            // 跳过空行
            while self.check(&TokenKind::Newline) {
                self.advance();
            }
            
            if self.check(&TokenKind::RightBrace) {
                break;
            }
            
            // 解析方法签名
            let method = self.parse_interface_method()?;
            methods.push(method);
        }
        
        // 期望 '}'
        self.expect(&TokenKind::RightBrace)?;
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::InterfaceDef { name, type_params: Vec::new(), super_interfaces: Vec::new(), methods, span })
    }
    
    /// 解析 interface 方法签名
    fn parse_interface_method(&mut self) -> Result<super::ast::InterfaceMethod, ParseError> {
        let start_span = self.current_span();
        
        // 期望 'func'
        self.expect(&TokenKind::Func)?;
        
        // 方法名
        let name = self.expect_identifier()?;
        
        // 参数列表（interface 方法不允许字段修饰符）
        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_fn_params(false)?;
        self.expect(&TokenKind::RightParen)?;
        
        // 返回类型（可选）
        let return_type = if !self.check(&TokenKind::Newline) && !self.check(&TokenKind::Semicolon) && !self.check(&TokenKind::RightBrace) {
            Some(self.parse_type_annotation()?)
        } else {
            None
        };
        
        // 可选的换行或分号
        if self.check(&TokenKind::Newline) || self.check(&TokenKind::Semicolon) {
            self.advance();
        }
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(super::ast::InterfaceMethod { name, params, return_type, span })
    }
    
    /// 解析 trait 定义
    fn parse_trait_definition(&mut self) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'trait'
        
        // trait 名称
        let name = self.expect_identifier()?;
        
        // 解析可选的泛型类型参数 <T, K, V>
        let type_params = self.parse_type_params()?;
        
        // 期望 '{'
        self.expect(&TokenKind::LeftBrace)?;
        
        let mut methods = Vec::new();
        
        // 解析方法（可能有默认实现）
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            // 跳过空行
            while self.check(&TokenKind::Newline) {
                self.advance();
            }
            
            if self.check(&TokenKind::RightBrace) {
                break;
            }
            
            let method = self.parse_trait_method()?;
            methods.push(method);
            
            // 跳过可选的分号或换行
            while self.check(&TokenKind::Semicolon) || self.check(&TokenKind::Newline) {
                self.advance();
            }
        }
        
        // 期望 '}'
        self.expect(&TokenKind::RightBrace)?;
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::TraitDef { name, type_params, where_clauses: Vec::new(), super_traits: Vec::new(), methods, span })
    }
    
    /// 解析 trait 方法（可能有默认实现）
    fn parse_trait_method(&mut self) -> Result<super::ast::TraitMethod, ParseError> {
        let start_span = self.current_span();
        
        // 期望 'func'
        self.expect(&TokenKind::Func)?;
        
        // 方法名
        let name = self.expect_identifier()?;
        
        // 参数列表（trait 方法不允许字段修饰符）
        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_fn_params(false)?;
        self.expect(&TokenKind::RightParen)?;
        
        // 返回类型（可选）
        let return_type = if !self.check(&TokenKind::Newline) && !self.check(&TokenKind::Semicolon) 
            && !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::LeftBrace) {
            Some(self.parse_type_annotation()?)
        } else {
            None
        };
        
        // 检查是否有默认实现（方法体）
        let default_body = if self.check(&TokenKind::LeftBrace) {
            Some(Box::new(self.parse_block()?))
        } else {
            None
        };
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(super::ast::TraitMethod { name, params, return_type, default_body, span })
    }
    
    /// 解析 enum 定义
    fn parse_enum_definition(&mut self) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'enum'
        
        // 枚举名称
        let name = self.expect_identifier()?;
        
        // 期望 '{'
        self.expect(&TokenKind::LeftBrace)?;
        
        let mut variants = Vec::new();
        
        // 解析变体
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            // 跳过空行
            while self.check(&TokenKind::Newline) {
                self.advance();
            }
            
            if self.check(&TokenKind::RightBrace) {
                break;
            }
            
            // 解析变体
            let variant = self.parse_enum_variant()?;
            variants.push(variant);
            
            // 逗号分隔（可选）
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }
        
        // 期望 '}'
        self.expect(&TokenKind::RightBrace)?;
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::EnumDef { name, variants, span })
    }
    
    /// 解析 type 别名
    fn parse_type_alias(&mut self) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'type'
        
        // 类型别名名称
        let name = self.expect_identifier()?;
        
        // 期望 '='
        self.expect(&TokenKind::Equal)?;
        
        // 目标类型
        let target_type = self.parse_type_annotation()?;
        
        // 可选的换行或分号
        if self.check(&TokenKind::Newline) || self.check(&TokenKind::Semicolon) {
            self.advance();
        }
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::TypeAlias { name, target_type, span })
    }
    
    /// 解析 enum 变体
    fn parse_enum_variant(&mut self) -> Result<super::ast::EnumVariant, ParseError> {
        let start_span = self.current_span();
        
        // 变体名
        let name = self.expect_identifier()?;
        
        // 可选的值或关联数据
        let mut value = None;
        let mut fields = Vec::new();
        
        if self.check(&TokenKind::Equal) {
            // 显式值
            self.advance();
            value = Some(self.parse_expression()?);
        } else if self.check(&TokenKind::LeftParen) {
            // 关联数据字段
            self.advance();
            while !self.check(&TokenKind::RightParen) {
                let field_name = self.expect_identifier()?;
                self.expect(&TokenKind::Colon)?;
                let field_type = self.parse_type_annotation()?;
                fields.push((field_name, field_type));
                
                if self.check(&TokenKind::Comma) {
                    self.advance();
                }
            }
            self.expect(&TokenKind::RightParen)?;
        }
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(super::ast::EnumVariant { name, value, fields, span })
    }
    
    /// 解析 match 语句
    fn parse_match_statement(&mut self) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'match'
        
        // 解析被匹配的表达式
        let expr = self.parse_expression()?;
        
        // 期望 '{'
        self.expect(&TokenKind::LeftBrace)?;
        
        let mut arms = Vec::new();
        
        // 解析分支
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            // 跳过空行
            while self.check(&TokenKind::Newline) {
                self.advance();
            }
            
            if self.check(&TokenKind::RightBrace) {
                break;
            }
            
            // 解析分支
            let arm = self.parse_match_arm()?;
            arms.push(arm);
            
            // 逗号分隔（可选）
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
            
            // 跳过空行
            while self.check(&TokenKind::Newline) {
                self.advance();
            }
        }
        
        // 期望 '}'
        self.expect(&TokenKind::RightBrace)?;
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::Match { expr, arms, span })
    }
    
    /// 解析 match 分支
    fn parse_match_arm(&mut self) -> Result<super::ast::MatchArm, ParseError> {
        let start_span = self.current_span();
        
        // 解析模式
        let pattern = self.parse_match_pattern()?;
        
        // 可选的守卫条件
        let guard = if self.check(&TokenKind::If) {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };
        
        // 期望 '=>'
        self.expect(&TokenKind::FatArrow)?;
        
        // 解析分支体（可以是块或表达式）
        let body = if self.check(&TokenKind::LeftBrace) {
            Box::new(self.parse_block()?)
        } else {
            let expr = self.parse_expression()?;
            let expr_span = expr.span();
            Box::new(Stmt::Expression { expr, span: expr_span })
        };
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(super::ast::MatchArm { pattern, guard, body, span })
    }
    
    /// 解析 match 模式
    fn parse_match_pattern(&mut self) -> Result<super::ast::MatchPattern, ParseError> {
        
        // 通配符模式 _
        if self.check(&TokenKind::Underscore) {
            self.advance();
            return Ok(super::ast::MatchPattern::Wildcard);
        }
        
        // 检查是否是多值模式（或模式）
        let first = self.parse_single_match_pattern()?;
        
        // 检查是否有 | 连接更多模式
        if self.check(&TokenKind::Pipe) {
            let mut patterns = vec![first];
            while self.check(&TokenKind::Pipe) {
                self.advance();
                patterns.push(self.parse_single_match_pattern()?);
            }
            return Ok(super::ast::MatchPattern::Or(patterns));
        }
        
        Ok(first)
    }
    
    /// 解析单个 match 模式（不包括 | 组合）
    fn parse_single_match_pattern(&mut self) -> Result<super::ast::MatchPattern, ParseError> {
        // 通配符模式 _
        if self.check(&TokenKind::Underscore) {
            self.advance();
            return Ok(super::ast::MatchPattern::Wildcard);
        }
        
        // 字面量模式（数字、字符串、布尔值）
        match &self.current_token().kind {
            TokenKind::Integer(_) | TokenKind::Float(_) | TokenKind::String(_) | TokenKind::RawString(_) 
            | TokenKind::True | TokenKind::False | TokenKind::Null => {
                // 先解析一个基本表达式（不包括中缀运算符）
                let start_expr = self.parse_prefix()?;
                
                // 检查是否是范围模式（在中缀处理之前）
                if self.check(&TokenKind::DotDot) || self.check(&TokenKind::DotDotEqual) {
                    let inclusive = self.check(&TokenKind::DotDotEqual);
                    self.advance();
                    // 解析范围的结束值
                    let end_expr = self.parse_prefix()?;
                    return Ok(super::ast::MatchPattern::Range {
                        start: Box::new(start_expr),
                        end: Box::new(end_expr),
                        inclusive,
                    });
                }
                
                return Ok(super::ast::MatchPattern::Literal(start_expr));
            }
            // 标识符可能是变量绑定
            TokenKind::Identifier(name) => {
                let name = name.clone();
                self.advance();
                
                // 检查是否是类型模式 x:Type
                if self.check(&TokenKind::Colon) {
                    self.advance();
                    let type_ann = self.parse_type_annotation()?;
                    return Ok(super::ast::MatchPattern::Type { name, type_ann });
                }
                
                // 否则是变量绑定
                return Ok(super::ast::MatchPattern::Variable(name));
            }
            _ => {
                // 尝试作为表达式解析
                let expr = self.parse_expression()?;
                return Ok(super::ast::MatchPattern::Literal(expr));
            }
        }
    }
    
    /// 解析 break 语句
    fn parse_break_statement(&mut self) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'break'
        
        // 可选的标签
        let label = if let TokenKind::Identifier(name) = &self.current_token().kind.clone() {
            let name = name.clone();
            self.advance();
            Some(name)
        } else {
            None
        };
        
        // 可选的换行或分号
        if self.check(&TokenKind::Newline) || self.check(&TokenKind::Semicolon) {
            self.advance();
        }
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::Break { label, span })
    }
    
    /// 解析 continue 语句
    fn parse_continue_statement(&mut self) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'continue'
        
        // 可选的标签
        let label = if let TokenKind::Identifier(name) = &self.current_token().kind.clone() {
            let name = name.clone();
            self.advance();
            Some(name)
        } else {
            None
        };
        
        // 可选的换行或分号
        if self.check(&TokenKind::Newline) || self.check(&TokenKind::Semicolon) {
            self.advance();
        }
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::Continue { label, span })
    }
    
    /// 解析 return 语句
    fn parse_return_statement(&mut self) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'return'
        
        // 可选的返回值
        let value = if !self.check(&TokenKind::Newline) 
            && !self.check(&TokenKind::Semicolon) 
            && !self.check(&TokenKind::RightBrace)
            && !self.is_at_end() 
        {
            Some(self.parse_expression()?)
        } else {
            None
        };
        
        // 可选的换行或分号
        if self.check(&TokenKind::Newline) || self.check(&TokenKind::Semicolon) {
            self.advance();
        }
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::Return { value, span })
    }
    
    /// 解析类型注解
    fn parse_type_annotation(&mut self) -> Result<TypeAnnotation, ParseError> {
        let start_span = self.current_span();
        let ty = self.parse_type()?;
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(TypeAnnotation { ty, span })
    }
    
    /// 解析类型
    fn parse_type(&mut self) -> Result<Type, ParseError> {
        let token = self.advance();
        
        // 解析基础类型
        let base_type = match &token.kind {
            TokenKind::Int => Type::Int,
            TokenKind::Uint => Type::Uint,
            TokenKind::I8 => Type::I8,
            TokenKind::I16 => Type::I16,
            TokenKind::I32 => Type::I32,
            TokenKind::I64 => Type::I64,
            TokenKind::U8 => Type::U8,
            TokenKind::U16 => Type::U16,
            TokenKind::U32 => Type::U32,
            TokenKind::U64 => Type::U64,
            TokenKind::F32 => Type::F32,
            TokenKind::F64 => Type::F64,
            TokenKind::Bool => Type::Bool,
            TokenKind::Byte => Type::Byte,
            TokenKind::CharType => Type::Char,
            TokenKind::StringType => Type::String,
            TokenKind::Unknown => Type::Unknown,
            TokenKind::Dynamic => Type::Dynamic,
            TokenKind::Func => {
                // 函数类型: func(int, string) bool
                return self.parse_function_type();
            }
            TokenKind::Identifier(name) => Type::Class(name.clone()),
            _ => {
                let msg = format_message(
                    messages::ERR_COMPILE_EXPECTED_TYPE,
                    self.locale,
                    &[],
                );
                return Err(ParseError::new(msg, token.span));
            }
        };
        
        // 检查泛型类型参数 <T, U>
        let typed = if self.check(&TokenKind::Less) {
            self.advance(); // 消费 '<'
            let type_args = self.parse_type_args()?;
            self.expect(&TokenKind::Greater)?;
            
            Type::Generic {
                base_type: Box::new(base_type),
                type_args,
            }
        } else {
            base_type
        };
        
        // 检查是否是固定数组类型 int[10] 或动态切片 int[]
        let result_type = if self.check(&TokenKind::LeftBracket) {
            self.advance(); // 消费 '['
            
            if self.check(&TokenKind::RightBracket) {
                // int[] - 动态切片
                self.advance(); // 消费 ']'
                Type::Slice { element_type: Box::new(typed) }
            } else {
                // int[10] - 固定数组
                let size = match &self.current_token().kind {
                    TokenKind::Integer(n) => {
                        if *n <= 0 {
                            return Err(ParseError::new(
                                "Array size must be positive".to_string(),
                                self.current_span(),
                            ));
                        }
                        *n as usize
                    }
                    _ => {
                        return Err(ParseError::new(
                            "Expected array size (positive integer)".to_string(),
                            self.current_span(),
                        ));
                    }
                };
                self.advance(); // 消费数字
                self.expect(&TokenKind::RightBracket)?;
                Type::Array { element_type: Box::new(typed), size }
            }
        } else {
            typed
        };
        
        // 检查是否是可空类型
        if self.check(&TokenKind::Question) {
            self.advance();
            Ok(Type::Nullable(Box::new(result_type)))
        } else {
            Ok(result_type)
        }
    }
    
    /// 解析泛型类型参数列表（如 <int, string>）
    fn parse_type_args(&mut self) -> Result<Vec<Type>, ParseError> {
        let mut types = Vec::new();
        
        // 跳过空行
        while self.check(&TokenKind::Newline) {
            self.advance();
        }
        
        // 检查空参数列表
        if self.check(&TokenKind::Greater) {
            return Ok(types);
        }
        
        // 解析第一个类型
        types.push(self.parse_type()?);
        
        // 解析更多类型
        while self.check(&TokenKind::Comma) {
            self.advance();
            
            // 跳过空行
            while self.check(&TokenKind::Newline) {
                self.advance();
            }
            
            if self.check(&TokenKind::Greater) {
                break; // 允许末尾逗号
            }
            
            types.push(self.parse_type()?);
        }
        
        Ok(types)
    }
    
    /// 解析函数类型 func(int, string) bool
    fn parse_function_type(&mut self) -> Result<Type, ParseError> {
        // 期望 '('
        self.expect(&TokenKind::LeftParen)?;
        
        let mut param_types = Vec::new();
        
        // 解析参数类型
        if !self.check(&TokenKind::RightParen) {
            param_types.push(self.parse_type()?);
            
            while self.check(&TokenKind::Comma) {
                self.advance();
                if self.check(&TokenKind::RightParen) {
                    break;
                }
                param_types.push(self.parse_type()?);
            }
        }
        
        self.expect(&TokenKind::RightParen)?;
        
        // 解析返回类型（可选）
        let return_type = if self.check_type_start() {
            self.parse_type()?
        } else {
            Type::Void
        };
        
        Ok(Type::Function {
            param_types: param_types.clone(),
            return_type: Box::new(return_type),
            required_params: param_types.len(), // 类型注解中的函数类型假设所有参数都是必需的
        })
    }
    
    /// 检查当前是否是类型开始
    fn check_type_start(&self) -> bool {
        matches!(
            self.current_token().kind,
            TokenKind::Int | TokenKind::Uint |
            TokenKind::I8 | TokenKind::I16 | TokenKind::I32 | TokenKind::I64 |
            TokenKind::U8 | TokenKind::U16 | TokenKind::U32 | TokenKind::U64 |
            TokenKind::F32 | TokenKind::F64 |
            TokenKind::Bool | TokenKind::Byte | TokenKind::CharType | TokenKind::StringType |
            TokenKind::Unknown | TokenKind::Dynamic | TokenKind::Func |
            TokenKind::Identifier(_)
        )
    }
    
    /// 期望一个标识符
    fn expect_identifier(&mut self) -> Result<String, ParseError> {
        if let TokenKind::Identifier(name) = &self.current_token().kind.clone() {
            let name = name.clone();
            self.advance();
            Ok(name)
        } else {
            let msg = format_message(
                messages::ERR_COMPILE_EXPECTED_IDENTIFIER,
                self.locale,
                &[],
            );
            Err(ParseError::new(msg, self.current_span()))
        }
    }

    /// 解析表达式语句
    fn parse_expression_statement(&mut self) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        let expr = self.parse_expression()?;
        
        // 可选的换行或分号
        if self.check(&TokenKind::Newline) || self.check(&TokenKind::Semicolon) {
            self.advance();
        }
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::Expression { expr, span })
    }

    /// 解析表达式
    fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        self.parse_assignment()
    }
    
    /// 解析赋值表达式
    fn parse_assignment(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_precedence(Precedence::Or)?;
        
        // 检查是否有赋值运算符
        let assign_op = if self.check(&TokenKind::Equal) {
            Some(AssignOp::Assign)
        } else if self.check(&TokenKind::PlusEqual) {
            Some(AssignOp::AddAssign)
        } else if self.check(&TokenKind::MinusEqual) {
            Some(AssignOp::SubAssign)
        } else if self.check(&TokenKind::StarEqual) {
            Some(AssignOp::MulAssign)
        } else if self.check(&TokenKind::SlashEqual) {
            Some(AssignOp::DivAssign)
        } else if self.check(&TokenKind::PercentEqual) {
            Some(AssignOp::ModAssign)
        } else if self.check(&TokenKind::AmpEqual) {
            Some(AssignOp::BitAndAssign)
        } else if self.check(&TokenKind::PipeEqual) {
            Some(AssignOp::BitOrAssign)
        } else if self.check(&TokenKind::CaretEqual) {
            Some(AssignOp::BitXorAssign)
        } else if self.check(&TokenKind::LessLessEqual) {
            Some(AssignOp::ShlAssign)
        } else if self.check(&TokenKind::GreaterGreaterEqual) {
            Some(AssignOp::ShrAssign)
        } else {
            None
        };
        
        if let Some(op) = assign_op {
            let op_token = self.advance();
            let value = self.parse_assignment()?; // 右结合
            
            // 检查左侧是否是有效的赋值目标
            match &expr {
                Expr::Identifier { name, span } => {
                    let end_span = value.span();
                    return Ok(Expr::Assign {
                        target: Box::new(Expr::Identifier { name: name.clone(), span: *span }),
                        op,
                        value: Box::new(value),
                        span: Span::new(span.start, end_span.end, span.line, span.column),
                    });
                }
                Expr::Member { object, member, span } => {
                    // 支持成员访问作为赋值目标 (e.g., this.name = value, obj.field = value)
                    let end_span = value.span();
                    return Ok(Expr::Assign {
                        target: Box::new(Expr::Member { 
                            object: object.clone(), 
                            member: member.clone(), 
                            span: *span 
                        }),
                        op,
                        value: Box::new(value),
                        span: Span::new(span.start, end_span.end, span.line, span.column),
                    });
                }
                Expr::Index { object, index, span } => {
                    // 支持索引访问作为赋值目标 (e.g., arr[0] = value)
                    let end_span = value.span();
                    return Ok(Expr::Assign {
                        target: Box::new(Expr::Index { 
                            object: object.clone(), 
                            index: index.clone(), 
                            span: *span 
                        }),
                        op,
                        value: Box::new(value),
                        span: Span::new(span.start, end_span.end, span.line, span.column),
                    });
                }
                _ => {
                    let msg = "Invalid assignment target".to_string();
                    return Err(ParseError::new(msg, op_token.span));
                }
            }
        }
        
        Ok(expr)
    }

    /// 按优先级解析表达式
    fn parse_precedence(&mut self, precedence: Precedence) -> Result<Expr, ParseError> {
        // 解析前缀表达式
        let mut left = self.parse_prefix()?;
        
        // 循环解析中缀表达式
        while precedence <= self.current_precedence() {
            left = self.parse_infix(left)?;
        }
        
        Ok(left)
    }

    /// 解析前缀表达式
    fn parse_prefix(&mut self) -> Result<Expr, ParseError> {
        let token = self.advance();
        
        match &token.kind {
            // 字面量
            TokenKind::Integer(n) => Ok(Expr::Integer {
                value: *n,
                span: token.span,
            }),
            TokenKind::Float(n) => Ok(Expr::Float {
                value: *n,
                span: token.span,
            }),
            TokenKind::String(s) => {
                // 检查是否包含字符串插值 ${...}
                if s.contains("${") {
                    self.parse_string_interpolation(s.clone(), token.span)
                } else {
                    Ok(Expr::String {
                        value: s.clone(),
                        span: token.span,
                    })
                }
            }
            TokenKind::RawString(s) => Ok(Expr::String {
                value: s.clone(),
                span: token.span,
            }),
            TokenKind::True => Ok(Expr::Bool {
                value: true,
                span: token.span,
            }),
            TokenKind::False => Ok(Expr::Bool {
                value: false,
                span: token.span,
            }),
            TokenKind::Null => Ok(Expr::Null {
                span: token.span,
            }),
            
            // 标识符、函数调用或 struct 字面量
            TokenKind::Identifier(name) => {
                if self.check(&TokenKind::ColonColon) {
                    // 静态访问: ClassName::member 或 ClassName::method()
                    self.parse_static_access(name.clone(), token.span)
                } else if self.check(&TokenKind::LeftParen) {
                    // 函数调用
                    self.parse_call(name.clone(), token.span)
                } else if self.check(&TokenKind::LeftBrace) {
                    // struct 字面量: Point { x: 1, y: 2 }
                    self.parse_struct_literal(name.clone(), token.span)
                } else {
                    Ok(Expr::Identifier {
                        name: name.clone(),
                        span: token.span,
                    })
                }
            }
            
            // 分组表达式
            TokenKind::LeftParen => {
                let start_span = token.span;
                let expr = self.parse_expression()?;
                self.expect(&TokenKind::RightParen)?;
                let end_span = self.previous_span();
                Ok(Expr::Grouping {
                    expr: Box::new(expr),
                    span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                })
            }
            
            // 一元运算符
            TokenKind::Minus => {
                let start_span = token.span;
                let operand = self.parse_precedence(Precedence::Unary)?;
                let end_span = operand.span();
                Ok(Expr::Unary {
                    op: UnaryOp::Neg,
                    operand: Box::new(operand),
                    span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                })
            }
            TokenKind::Bang => {
                let start_span = token.span;
                let operand = self.parse_precedence(Precedence::Unary)?;
                let end_span = operand.span();
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                    span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                })
            }
            TokenKind::Tilde => {
                let start_span = token.span;
                let operand = self.parse_precedence(Precedence::Unary)?;
                let end_span = operand.span();
                Ok(Expr::Unary {
                    op: UnaryOp::BitNot,
                    operand: Box::new(operand),
                    span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                })
            }
            
            // 闭包表达式 func(params) returnType { body }
            TokenKind::Func => {
                self.parse_closure(token.span)
            }
            
            // this 关键字
            TokenKind::This => {
                Ok(Expr::This { span: token.span })
            }
            
            // super 关键字
            TokenKind::Super => {
                Ok(Expr::Super { span: token.span })
            }
            
            // go 关键字：启动协程
            TokenKind::Go => {
                let start_span = token.span;
                // 解析后面的函数调用表达式
                let call = self.parse_expression()?;
                let end_span = call.span();
                Ok(Expr::Go {
                    call: Box::new(call),
                    span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                })
            }
            
            // 数组字面量 [1, 2, 3]
            TokenKind::LeftBracket => {
                let start_span = token.span;
                let mut elements = Vec::new();
                
                if !self.check(&TokenKind::RightBracket) {
                    elements.push(self.parse_expression()?);
                    while self.check(&TokenKind::Comma) {
                        self.advance();
                        if self.check(&TokenKind::RightBracket) {
                            break; // 允许末尾逗号
                        }
                        elements.push(self.parse_expression()?);
                    }
                }
                
                self.expect(&TokenKind::RightBracket)?;
                let end_span = self.previous_span();
                
                Ok(Expr::Array {
                    elements,
                    span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                })
            }
            
            // Map 字面量 { "key": value, ... }
            TokenKind::LeftBrace => {
                let start_span = token.span;
                let mut entries = Vec::new();
                
                // 跳过可能的空行
                while self.check(&TokenKind::Newline) {
                    self.advance();
                }
                
                if !self.check(&TokenKind::RightBrace) {
                    // 解析第一个键值对
                    let key = self.parse_expression()?;
                    self.expect(&TokenKind::Colon)?;
                    let value = self.parse_expression()?;
                    entries.push((key, value));
                    
                    while self.check(&TokenKind::Comma) {
                        self.advance();
                        // 跳过可能的空行
                        while self.check(&TokenKind::Newline) {
                            self.advance();
                        }
                        if self.check(&TokenKind::RightBrace) {
                            break; // 允许末尾逗号
                        }
                        let key = self.parse_expression()?;
                        self.expect(&TokenKind::Colon)?;
                        let value = self.parse_expression()?;
                        entries.push((key, value));
                    }
                }
                
                // 跳过可能的空行
                while self.check(&TokenKind::Newline) {
                    self.advance();
                }
                
                self.expect(&TokenKind::RightBrace)?;
                let end_span = self.previous_span();
                
                Ok(Expr::MapLiteral {
                    entries,
                    span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                })
            }
            
            // typeof/sizeof/make 内置函数（作为关键字处理）
            TokenKind::Typeof | TokenKind::Sizeof | TokenKind::Make => {
                let func_name = match &token.kind {
                    TokenKind::Typeof => "typeof",
                    TokenKind::Sizeof => "sizeof",
                    TokenKind::Make => "make",
                    _ => unreachable!(),
                };
                self.parse_call(func_name.to_string(), token.span)
            }
            
            // new 表达式
            TokenKind::New => {
                let start_span = token.span;
                let class_name = self.expect_identifier()?;
                self.expect(&TokenKind::LeftParen)?;
                
                let mut args = Vec::new();
                if !self.check(&TokenKind::RightParen) {
                    args.push(self.parse_expression()?);
                    while self.check(&TokenKind::Comma) {
                        self.advance();
                        args.push(self.parse_expression()?);
                    }
                }
                self.expect(&TokenKind::RightParen)?;
                
                let end_span = self.previous_span();
                Ok(Expr::New {
                    class_name,
                    args,
                    span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                })
            }
            
            _ => {
                let msg = format_message(
                    messages::ERR_COMPILE_EXPECTED_EXPRESSION,
                    self.locale,
                    &[],
                );
                Err(ParseError::new(msg, token.span))
            }
        }
    }

    /// 解析中缀表达式
    fn parse_infix(&mut self, left: Expr) -> Result<Expr, ParseError> {
        let token = self.advance();
        let start_span = left.span();
        
        let (op, precedence) = match &token.kind {
            TokenKind::Plus => (BinOp::Add, Precedence::Term),
            TokenKind::Minus => (BinOp::Sub, Precedence::Term),
            TokenKind::Star => (BinOp::Mul, Precedence::Factor),
            TokenKind::Slash => (BinOp::Div, Precedence::Factor),
            TokenKind::Percent => (BinOp::Mod, Precedence::Factor),
            TokenKind::StarStar => (BinOp::Pow, Precedence::Power),
            TokenKind::EqualEqual => (BinOp::Eq, Precedence::Equality),
            TokenKind::BangEqual => (BinOp::Ne, Precedence::Equality),
            TokenKind::Less => (BinOp::Lt, Precedence::Comparison),
            TokenKind::LessEqual => (BinOp::Le, Precedence::Comparison),
            TokenKind::Greater => (BinOp::Gt, Precedence::Comparison),
            TokenKind::GreaterEqual => (BinOp::Ge, Precedence::Comparison),
            TokenKind::AmpAmp => (BinOp::And, Precedence::And),
            TokenKind::PipePipe => (BinOp::Or, Precedence::Or),
            
            // 范围表达式 1..10 或 1..=10
            TokenKind::DotDot => {
                let right = self.parse_precedence(Precedence::Term)?;
                let end_span = right.span();
                return Ok(Expr::Range {
                    start: Some(Box::new(left)),
                    end: Some(Box::new(right)),
                    inclusive: false,
                    span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                });
            }
            TokenKind::DotDotEqual => {
                let right = self.parse_precedence(Precedence::Term)?;
                let end_span = right.span();
                return Ok(Expr::Range {
                    start: Some(Box::new(left)),
                    end: Some(Box::new(right)),
                    inclusive: true,
                    span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                });
            }
            
            // 成员访问 obj.field
            TokenKind::Dot => {
                let member_name = self.expect_identifier()?;
                
                // 检查是否是方法调用
                if self.check(&TokenKind::LeftParen) {
                    self.advance();
                    let mut args: Vec<(Option<String>, Expr)> = Vec::new();
                    if !self.check(&TokenKind::RightParen) {
                        args.push((None, self.parse_expression()?));
                        while self.check(&TokenKind::Comma) {
                            self.advance();
                            args.push((None, self.parse_expression()?));
                        }
                    }
                    self.expect(&TokenKind::RightParen)?;
                    
                    let end_span = self.previous_span();
                    // 方法调用是对成员的调用
                    let member_expr = Expr::Member {
                        object: Box::new(left),
                        member: member_name.clone(),
                        span: Span::new(start_span.start, token.span.end, start_span.line, start_span.column),
                    };
                    return Ok(Expr::Call {
                        callee: Box::new(member_expr),
                        args,
                        span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                    });
                }
                
                let end_span = self.previous_span();
                return Ok(Expr::Member {
                    object: Box::new(left),
                    member: member_name,
                    span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                });
            }
            
            // 安全成员访问 obj?.field
            TokenKind::QuestionDot => {
                let member_name = self.expect_identifier()?;
                
                // 检查是否是方法调用
                if self.check(&TokenKind::LeftParen) {
                    self.advance();
                    let mut args: Vec<(Option<String>, Expr)> = Vec::new();
                    if !self.check(&TokenKind::RightParen) {
                        args.push((None, self.parse_expression()?));
                        while self.check(&TokenKind::Comma) {
                            self.advance();
                            args.push((None, self.parse_expression()?));
                        }
                    }
                    self.expect(&TokenKind::RightParen)?;
                    
                    let end_span = self.previous_span();
                    // 安全方法调用
                    let member_expr = Expr::SafeMember {
                        object: Box::new(left),
                        member: member_name.clone(),
                        span: Span::new(start_span.start, token.span.end, start_span.line, start_span.column),
                    };
                    return Ok(Expr::Call {
                        callee: Box::new(member_expr),
                        args,
                        span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                    });
                }
                
                let end_span = self.previous_span();
                return Ok(Expr::SafeMember {
                    object: Box::new(left),
                    member: member_name,
                    span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                });
            }
            
            // 非空断言成员访问 obj!.field
            TokenKind::BangDot => {
                let member_name = self.expect_identifier()?;
                
                // 检查是否是方法调用
                if self.check(&TokenKind::LeftParen) {
                    self.advance();
                    let mut args: Vec<(Option<String>, Expr)> = Vec::new();
                    if !self.check(&TokenKind::RightParen) {
                        args.push((None, self.parse_expression()?));
                        while self.check(&TokenKind::Comma) {
                            self.advance();
                            args.push((None, self.parse_expression()?));
                        }
                    }
                    self.expect(&TokenKind::RightParen)?;
                    
                    let end_span = self.previous_span();
                    // 非空断言方法调用
                    let member_expr = Expr::NonNullMember {
                        object: Box::new(left),
                        member: member_name.clone(),
                        span: Span::new(start_span.start, token.span.end, start_span.line, start_span.column),
                    };
                    return Ok(Expr::Call {
                        callee: Box::new(member_expr),
                        args,
                        span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                    });
                }
                
                let end_span = self.previous_span();
                return Ok(Expr::NonNullMember {
                    object: Box::new(left),
                    member: member_name,
                    span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                });
            }
            
            // 空值合并 a ?? b
            TokenKind::QuestionQuestion => {
                let right = self.parse_precedence(Precedence::Or)?;
                let end_span = right.span();
                return Ok(Expr::NullCoalesce {
                    left: Box::new(left),
                    right: Box::new(right),
                    span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                });
            }
            
            // 函数调用 func(args) - 支持命名参数
            TokenKind::LeftParen => {
                let mut args: Vec<(Option<String>, Expr)> = Vec::new();
                let mut seen_named = false;
                
                if !self.check(&TokenKind::RightParen) {
                    loop {
                        // 检查是否是命名参数
                        let (param_name, value) = if let TokenKind::Identifier(name) = &self.current_token().kind {
                            let name = name.clone();
                            let save_pos = self.current;
                            self.advance();
                            
                            if self.check(&TokenKind::Colon) {
                                self.advance();
                                seen_named = true;
                                let value = self.parse_expression()?;
                                (Some(name), value)
                            } else {
                                self.current = save_pos;
                                if seen_named {
                                    return Err(ParseError::new(
                                        "Positional arguments cannot follow named arguments".to_string(),
                                        self.current_span(),
                                    ));
                                }
                                (None, self.parse_expression()?)
                            }
                        } else {
                            if seen_named {
                                return Err(ParseError::new(
                                    "Positional arguments cannot follow named arguments".to_string(),
                                    self.current_span(),
                                ));
                            }
                            (None, self.parse_expression()?)
                        };
                        
                        args.push((param_name, value));
                        
                        if !self.check(&TokenKind::Comma) {
                            break;
                        }
                        self.advance();
                    }
                }
                self.expect(&TokenKind::RightParen)?;
                
                let end_span = self.previous_span();
                return Ok(Expr::Call {
                    callee: Box::new(left),
                    args,
                    span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                });
            }
            
            // 索引访问 arr[index]
            TokenKind::LeftBracket => {
                let index = self.parse_expression()?;
                self.expect(&TokenKind::RightBracket)?;
                
                let end_span = self.previous_span();
                return Ok(Expr::Index {
                    object: Box::new(left),
                    index: Box::new(index),
                    span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                });
            }
            
            // 类型转换 expr as Type 或 expr as! Type
            TokenKind::As => {
                // 检查是否有 !
                let force = if self.check(&TokenKind::Bang) {
                    self.advance();
                    true
                } else {
                    false
                };
                
                // 解析目标类型
                let target_type = self.parse_type_annotation()?;
                let end_span = self.previous_span();
                
                return Ok(Expr::Cast {
                    expr: Box::new(left),
                    target_type,
                    force,
                    span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                });
            }
            
            // 类型检查 expr is Type
            TokenKind::Is => {
                // 解析检查的类型
                let check_type = self.parse_type_annotation()?;
                let end_span = self.previous_span();
                
                return Ok(Expr::TypeCheck {
                    expr: Box::new(left),
                    check_type,
                    span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
                });
            }
            
            _ => {
                let msg = format_message(
                    messages::ERR_COMPILE_UNEXPECTED_TOKEN,
                    self.locale,
                    &[&token.lexeme],
                );
                return Err(ParseError::new(msg, token.span));
            }
        };
        
        // 右结合的运算符（如幂运算）
        let next_precedence = if op == BinOp::Pow {
            Precedence::Power
        } else {
            // 左结合：下一个优先级
            match precedence {
                Precedence::Or => Precedence::And,
                Precedence::And => Precedence::Equality,
                Precedence::Equality => Precedence::Comparison,
                Precedence::Comparison => Precedence::Term,
                Precedence::Term => Precedence::Factor,
                Precedence::Factor => Precedence::Power,
                Precedence::Power => Precedence::Unary,
                _ => Precedence::None,
            }
        };
        
        let right = self.parse_precedence(next_precedence)?;
        let end_span = right.span();
        
        Ok(Expr::Binary {
            left: Box::new(left),
            op,
            right: Box::new(right),
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    /// 解析静态访问 ClassName::member 或 ClassName::method()
    fn parse_static_access(&mut self, class_name: String, start_span: Span) -> Result<Expr, ParseError> {
        self.advance(); // 消费 '::'
        
        // 获取成员名
        let member = self.expect_identifier()?;
        
        // 检查是否是静态方法调用
        if self.check(&TokenKind::LeftParen) {
            // 静态方法调用: ClassName::method(args)
            self.advance(); // 消费 '('
            
            let mut args: Vec<(Option<String>, Expr)> = Vec::new();
            
            if !self.check(&TokenKind::RightParen) {
                loop {
                    args.push((None, self.parse_expression()?));
                    
                    if !self.check(&TokenKind::Comma) {
                        break;
                    }
                    self.advance(); // 消费 ','
                }
            }
            
            self.expect(&TokenKind::RightParen)?;
            let end_span = self.previous_span();
            
            // 静态方法调用表示为 Call，其 callee 是 StaticMember
            let callee = Box::new(Expr::StaticMember {
                class_name,
                member,
                span: start_span,
            });
            
            Ok(Expr::Call {
                callee,
                args,
                span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
            })
        } else {
            // 静态字段访问: ClassName::CONST
            let end_span = self.previous_span();
            Ok(Expr::StaticMember {
                class_name,
                member,
                span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
            })
        }
    }

    /// 解析函数调用（支持命名参数）
    /// 
    /// 语法: 
    ///   foo(1, 2, 3)                    - 位置参数
    ///   foo(name: "Alice", age: 25)    - 命名参数
    ///   foo(1, name: "Alice")          - 混合（位置参数必须在前）
    fn parse_call(&mut self, callee_name: String, start_span: Span) -> Result<Expr, ParseError> {
        // 创建 callee 表达式
        let callee = Box::new(Expr::Identifier {
            name: callee_name,
            span: start_span,
        });
        
        self.advance(); // 消费 '('
        
        let mut args: Vec<(Option<String>, Expr)> = Vec::new();
        let mut seen_named = false; // 是否已经遇到命名参数
        
        if !self.check(&TokenKind::RightParen) {
            loop {
                // 检查是否是命名参数: name: value
                let (param_name, value) = if let TokenKind::Identifier(name) = &self.current_token().kind {
                    let name = name.clone();
                    let save_pos = self.current;
                    self.advance(); // 消费标识符
                    
                    if self.check(&TokenKind::Colon) {
                        // 是命名参数
                        self.advance(); // 消费 ':'
                        seen_named = true;
                        let value = self.parse_expression()?;
                        (Some(name), value)
                    } else {
                        // 不是命名参数，回退
                        self.current = save_pos;
                        
                        if seen_named {
                            return Err(ParseError::new(
                                "Positional arguments cannot follow named arguments".to_string(),
                                self.current_span(),
                            ));
                        }
                        
                        let value = self.parse_expression()?;
                        (None, value)
                    }
                } else {
                    if seen_named {
                        return Err(ParseError::new(
                            "Positional arguments cannot follow named arguments".to_string(),
                            self.current_span(),
                        ));
                    }
                    
                    let value = self.parse_expression()?;
                    (None, value)
                };
                
                args.push((param_name, value));
                
                if !self.check(&TokenKind::Comma) {
                    break;
                }
                self.advance(); // 消费 ','
            }
        }
        
        self.expect(&TokenKind::RightParen)?;
        let end_span = self.previous_span();
        
        Ok(Expr::Call {
            callee,
            args,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }
    
    /// 解析 struct 字面量: Point { x: 1, y: 2 }
    fn parse_struct_literal(&mut self, name: String, start_span: Span) -> Result<Expr, ParseError> {
        self.advance(); // 消费 '{'
        
        let mut fields = Vec::new();
        
        // 跳过空行
        while self.check(&TokenKind::Newline) {
            self.advance();
        }
        
        if !self.check(&TokenKind::RightBrace) {
            loop {
                // 跳过空行
                while self.check(&TokenKind::Newline) {
                    self.advance();
                }
                
                if self.check(&TokenKind::RightBrace) {
                    break;
                }
                
                // 字段名
                let field_name = self.expect_identifier()?;
                
                // 冒号
                self.expect(&TokenKind::Colon)?;
                
                // 字段值
                let field_value = self.parse_expression()?;
                
                fields.push((field_name, field_value));
                
                // 跳过空行
                while self.check(&TokenKind::Newline) {
                    self.advance();
                }
                
                if !self.check(&TokenKind::Comma) {
                    break;
                }
                self.advance(); // 消费 ','
            }
        }
        
        self.expect(&TokenKind::RightBrace)?;
        let end_span = self.previous_span();
        
        Ok(Expr::StructLiteral {
            name,
            fields,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    /// 解析命名函数定义
    /// func name(params) return_type { body }
    fn parse_named_function_with_visibility(&mut self, visibility: super::ast::Visibility) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'func'
        
        // 函数名
        let name = self.expect_identifier()?;
        
        // 解析可选的泛型类型参数 <T, U>
        let type_params = self.parse_type_params()?;
        
        // 参数列表（包级函数不允许字段修饰符）
        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_fn_params(false)?;
        self.expect(&TokenKind::RightParen)?;
        
        // 返回类型（可选）
        let return_type = if !self.check(&TokenKind::LeftBrace) && !self.check(&TokenKind::Newline) {
            Some(self.parse_type_annotation()?)
        } else {
            None
        };
        
        // 函数体
        let body = Box::new(self.parse_block()?);
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::FnDef { name, type_params, where_clauses: Vec::new(), params, return_type, body, visibility, span })
    }
    
    /// 解析闭包表达式
    fn parse_closure(&mut self, start_span: Span) -> Result<Expr, ParseError> {
        // 解析参数列表（闭包不允许字段修饰符）
        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_fn_params(false)?;
        self.expect(&TokenKind::RightParen)?;
        
        // 解析可选的返回类型
        let return_type = if !self.check(&TokenKind::LeftBrace) {
            Some(self.parse_type_annotation()?)
        } else {
            None
        };
        
        // 解析函数体
        let body = Box::new(self.parse_block()?);
        let end_span = self.previous_span();
        
        Ok(Expr::Closure {
            params,
            return_type,
            body,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }
    
    /// 解析函数参数列表
    /// allow_field_modifiers: 是否允许 var/val/const 修饰符（仅在 init 方法中允许）
    fn parse_fn_params(&mut self, allow_field_modifiers: bool) -> Result<Vec<FnParam>, ParseError> {
        let mut params = Vec::new();
        
        // 跳过开头的换行符
        while self.check(&TokenKind::Newline) {
            self.advance();
        }
        
        if self.check(&TokenKind::RightParen) {
            return Ok(params);
        }
        
        loop {
            // 跳过参数前的换行符
            while self.check(&TokenKind::Newline) {
                self.advance();
            }
            
            // 再次检查是否到达参数列表末尾
            if self.check(&TokenKind::RightParen) {
                break;
            }
            
            let start_span = self.current_span();
            
            // 检查是否有字段修饰符（可选的可见性 + var/val/const）
            let mut is_field = false;
            let mut is_mutable = false;
            let mut field_visibility: Option<super::ast::Visibility> = None;
            
            if allow_field_modifiers {
                // 可选的可见性修饰符
                if self.check(&TokenKind::Public) || self.check(&TokenKind::Private) 
                    || self.check(&TokenKind::Protected) || self.check(&TokenKind::Internal) {
                    field_visibility = Some(self.parse_visibility());
                }
                
                // 检查 var/val/const
                if self.check(&TokenKind::Var) {
                    self.advance();
                    is_field = true;
                    is_mutable = true;
                } else if self.check(&TokenKind::Val) || self.check(&TokenKind::Const) {
                    self.advance();
                    is_field = true;
                    is_mutable = false;
                } else if field_visibility.is_some() {
                    // 有可见性但没有 var/val/const 是错误的
                    let msg = "Visibility modifier must be followed by 'var', 'val', or 'const'".to_string();
                    return Err(ParseError::new(msg, self.current_span()));
                }
            }
            
            // 参数名
            let name = self.expect_identifier()?;
            
            // 冒号和类型
            self.expect(&TokenKind::Colon)?;
            
            // 检查是否是可变参数 name:int...
            let type_ann = self.parse_type_annotation()?;
            let variadic = if self.check(&TokenKind::DotDotDot) {
                self.advance();
                true
            } else {
                false
            };
            
            // 可选的默认值
            let default = if self.check(&TokenKind::Equal) {
                self.advance();
                Some(self.parse_expression()?)
            } else {
                None
            };
            
            let end_span = self.previous_span();
            
            params.push(FnParam {
                name,
                type_ann,
                default,
                variadic,
                is_field,
                is_mutable,
                field_visibility,
                span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
            });
            
            // 可变参数必须是最后一个
            if variadic {
                break;
            }
            
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance(); // 消费 ','
            // 跳过逗号后的换行符
            while self.check(&TokenKind::Newline) {
                self.advance();
            }
        }
        
        Ok(params)
    }

    /// 获取当前 token 的优先级
    fn current_precedence(&self) -> Precedence {
        if self.is_at_end() {
            return Precedence::None;
        }
        
        match &self.current_token().kind {
            // 成员访问和调用 - 最高优先级
            TokenKind::Dot | TokenKind::QuestionDot | TokenKind::BangDot | TokenKind::LeftParen | TokenKind::LeftBracket => Precedence::Call,
            // 空值合并运算符
            TokenKind::QuestionQuestion => Precedence::Or,
            // 范围运算符
            TokenKind::DotDot | TokenKind::DotDotEqual => Precedence::Comparison,
            TokenKind::PipePipe => Precedence::Or,
            TokenKind::AmpAmp => Precedence::And,
            TokenKind::EqualEqual | TokenKind::BangEqual => Precedence::Equality,
            TokenKind::Less | TokenKind::LessEqual | TokenKind::Greater | TokenKind::GreaterEqual => {
                Precedence::Comparison
            }
            // 类型转换和检查
            TokenKind::As | TokenKind::Is => Precedence::Comparison,
            TokenKind::Plus | TokenKind::Minus => Precedence::Term,
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent => Precedence::Factor,
            TokenKind::StarStar => Precedence::Power,
            _ => Precedence::None,
        }
    }

    /// 检查当前 token 是否是指定类型
    fn check(&self, kind: &TokenKind) -> bool {
        if self.is_at_end() {
            return false;
        }
        std::mem::discriminant(&self.current_token().kind) == std::mem::discriminant(kind)
    }

    /// 检查当前 token 是否是指定的标识符
    fn check_identifier(&self, name: &str) -> bool {
        if self.is_at_end() {
            return false;
        }
        matches!(&self.current_token().kind, TokenKind::Identifier(n) if n == name)
    }

    /// 前进一个 token 并返回之前的 token
    fn advance(&mut self) -> Token {
        if !self.is_at_end() {
            self.current += 1;
        }
        self.previous_token().clone()
    }

    /// 期望指定类型的 token
    fn expect(&mut self, kind: &TokenKind) -> Result<Token, ParseError> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            let msg = format_message(
                messages::ERR_COMPILE_EXPECTED_TOKEN,
                self.locale,
                &[&format!("{}", kind), &self.current_token().lexeme],
            );
            Err(ParseError::new(msg, self.current_span()))
        }
    }

    /// 判断是否到达末尾
    fn is_at_end(&self) -> bool {
        self.current >= self.tokens.len() || self.current_token().is_eof()
    }

    /// 获取当前 token
    fn current_token(&self) -> &Token {
        &self.tokens[self.current.min(self.tokens.len() - 1)]
    }
    
    /// 获取下一个 token（peek）
    fn peek_token(&self) -> Option<&Token> {
        if self.current + 1 < self.tokens.len() {
            Some(&self.tokens[self.current + 1])
        } else {
            None
        }
    }

    /// 获取前一个 token
    /// 解析字符串插值 "Hello, ${name}!"
    fn parse_string_interpolation(&self, s: String, span: Span) -> Result<Expr, ParseError> {
        use super::ast::StringInterpPart;
        
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut chars = s.chars().peekable();
        
        while let Some(c) = chars.next() {
            if c == '$' && chars.peek() == Some(&'{') {
                // 保存之前的字符串部分
                if !current.is_empty() {
                    parts.push(StringInterpPart::Literal(current.clone()));
                    current.clear();
                }
                
                // 跳过 '{'
                chars.next();
                
                // 收集表达式字符串（直到找到匹配的 '}'）
                let mut expr_str = String::new();
                let mut brace_depth = 1;
                
                while let Some(ec) = chars.next() {
                    if ec == '{' {
                        brace_depth += 1;
                        expr_str.push(ec);
                    } else if ec == '}' {
                        brace_depth -= 1;
                        if brace_depth == 0 {
                            break;
                        }
                        expr_str.push(ec);
                    } else {
                        expr_str.push(ec);
                    }
                }
                
                // 解析表达式
                let mut scanner = crate::lexer::Scanner::new(&expr_str);
                let tokens = scanner.scan_tokens();
                let mut parser = Parser::new(tokens, self.locale.clone());
                
                match parser.parse_expression() {
                    Ok(expr) => parts.push(StringInterpPart::Expr(expr)),
                    Err(e) => return Err(ParseError::new(
                        format!("Error in string interpolation: {}", e.message),
                        span,
                    )),
                }
            } else {
                current.push(c);
            }
        }
        
        // 添加剩余的字符串部分
        if !current.is_empty() {
            parts.push(StringInterpPart::Literal(current));
        }
        
        Ok(Expr::StringInterpolation { parts, span })
    }

    fn previous_token(&self) -> &Token {
        &self.tokens[(self.current - 1).max(0)]
    }

    /// 获取当前位置信息
    fn current_span(&self) -> Span {
        self.current_token().span
    }

    /// 获取前一个 token 的位置信息
    fn previous_span(&self) -> Span {
        self.previous_token().span
    }

    /// 错误恢复：同步到下一个语句
    /// 错误恢复：同步到下一个安全点
    /// 
    /// 在遇到解析错误后，跳过 token 直到找到可能的语句开始位置，
    /// 从而能够继续解析并发现更多错误
    fn synchronize(&mut self) {
        self.panic_mode = true;
        
        while !self.is_at_end() {
            // 如果前一个是换行或分号，认为同步完成
            if matches!(
                self.previous_token().kind,
                TokenKind::Newline | TokenKind::Semicolon
            ) {
                self.panic_mode = false;
                return;
            }
            
            // 检查是否到达可能的语句开始
            match self.current_token().kind {
                // 声明关键字 - 可以开始新语句
                TokenKind::Var | TokenKind::Const | TokenKind::Func |
                TokenKind::Class | TokenKind::Struct | TokenKind::Enum |
                TokenKind::Interface | TokenKind::Trait | TokenKind::Type |
                TokenKind::Import | TokenKind::Package |
                // 控制流关键字
                TokenKind::If | TokenKind::For |
                TokenKind::Match | TokenKind::Return | TokenKind::Break |
                TokenKind::Continue | TokenKind::Throw | TokenKind::Try |
                // 可见性修饰符
                TokenKind::Public | TokenKind::Private | TokenKind::Internal |
                // 块结束
                TokenKind::RightBrace => {
                    self.panic_mode = false;
                    return;
                }
                TokenKind::Newline => {
                    self.advance();
                    self.panic_mode = false;
                    return;
                }
                _ => {
                    self.advance();
                }
            }
        }
        
        self.panic_mode = false;
    }
    
    /// 在错误恢复模式下跳过到指定 token
    fn synchronize_to(&mut self, kinds: &[TokenKind]) {
        self.panic_mode = true;
        
        while !self.is_at_end() {
            if kinds.contains(&self.current_token().kind) {
                self.panic_mode = false;
                return;
            }
            self.advance();
        }
        
        self.panic_mode = false;
    }
    
    /// 报告错误但不立即返回，用于收集多个错误
    fn report_error(&mut self, error: ParseError) {
        // 在恐慌模式下不报告错误，避免级联错误
        if !self.panic_mode {
            self.errors.push(error);
        }
    }
    
    /// 期望其中一个 token
    fn expect_one_of(&mut self, kinds: &[TokenKind]) -> Result<Token, ParseError> {
        for kind in kinds {
            if self.check(kind) {
                return Ok(self.advance());
            }
        }
        
        let expected: Vec<String> = kinds.iter().map(|k| format!("{}", k)).collect();
        let found = self.current_token().lexeme.clone();
        let span = self.current_span();
        
        Err(ParseError::expected_token(
            &expected.join(" or "),
            &found,
            span,
        ))
    }
    
    /// 解析 try-catch-finally 语句
    fn parse_try_statement(&mut self) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'try'
        
        // 解析 try 块
        let try_block = self.parse_block()?;
        
        // 期望 catch
        if !self.check(&TokenKind::Catch) {
            let msg = "Expected 'catch' after try block".to_string();
            return Err(ParseError::new(msg, self.current_span()));
        }
        self.advance(); // 消费 'catch'
        
        // 可选的参数名 (e)
        let catch_param = if self.check(&TokenKind::LeftParen) {
            self.advance(); // 消费 '('
            let param = self.expect_identifier()?;
            self.expect(&TokenKind::RightParen)?;
            Some(param)
        } else {
            None
        };
        
        // 解析 catch 块
        let catch_block = self.parse_block()?;
        
        // 可选的 finally 块
        let finally_block = if self.check(&TokenKind::Finally) {
            self.advance(); // 消费 'finally'
            Some(Box::new(self.parse_block()?))
        } else {
            None
        };
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::TryCatch {
            try_block: Box::new(try_block),
            catch_param,
            catch_block: Box::new(catch_block),
            finally_block,
            span,
        })
    }
    
    /// 解析 throw 语句
    fn parse_throw_statement(&mut self) -> Result<Stmt, ParseError> {
        let start_span = self.current_span();
        self.advance(); // 消费 'throw'
        
        // 解析要抛出的表达式
        let value = self.parse_expression()?;
        
        // 可选的换行或分号
        if self.check(&TokenKind::Newline) || self.check(&TokenKind::Semicolon) {
            self.advance();
        }
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end, start_span.line, start_span.column);
        
        Ok(Stmt::Throw { value, span })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Scanner;

    fn parse(source: &str) -> Result<Program, Vec<ParseError>> {
        let mut scanner = Scanner::new(source);
        let tokens = scanner.scan_tokens();
        let mut parser = Parser::new(tokens, Locale::En);
        parser.parse()
    }

    #[test]
    fn test_parse_integer() {
        let program = parse("123").unwrap();
        assert_eq!(program.statements.len(), 1);
        if let Stmt::Expression { expr, .. } = &program.statements[0] {
            assert!(matches!(expr, Expr::Integer { value: 123, .. }));
        } else {
            panic!("Expected expression statement");
        }
    }

    #[test]
    fn test_parse_binary() {
        let program = parse("1 + 2 * 3").unwrap();
        assert_eq!(program.statements.len(), 1);
        // 应该解析为 1 + (2 * 3)
    }

    #[test]
    fn test_parse_print() {
        let program = parse("print(42)").unwrap();
        assert_eq!(program.statements.len(), 1);
        assert!(matches!(program.statements[0], Stmt::Print { .. }));
    }
    
    #[test]
    fn test_parse_var_decl() {
        // 带初始化
        let program = parse("var x = 10").unwrap();
        assert_eq!(program.statements.len(), 1);
        if let Stmt::VarDecl { name, initializer, .. } = &program.statements[0] {
            assert_eq!(name, "x");
            assert!(initializer.is_some());
        } else {
            panic!("Expected VarDecl");
        }
        
        // 带类型注解
        let program = parse("var x: int = 10").unwrap();
        assert_eq!(program.statements.len(), 1);
        if let Stmt::VarDecl { name, type_ann, .. } = &program.statements[0] {
            assert_eq!(name, "x");
            assert!(type_ann.is_some());
        } else {
            panic!("Expected VarDecl");
        }
    }
    
    #[test]
    fn test_parse_const_decl() {
        let program = parse("const PI = 3.14").unwrap();
        assert_eq!(program.statements.len(), 1);
        assert!(matches!(program.statements[0], Stmt::ConstDecl { .. }));
    }
    
    #[test]
    fn test_parse_assignment() {
        let program = parse("x = 10").unwrap();
        assert_eq!(program.statements.len(), 1);
        if let Stmt::Expression { expr, .. } = &program.statements[0] {
            assert!(matches!(expr, Expr::Assign { .. }));
        } else {
            panic!("Expected Expression with Assign");
        }
    }
    
    #[test]
    fn test_parse_if() {
        let program = parse("if x > 5 { print(x) }").unwrap();
        assert_eq!(program.statements.len(), 1);
        assert!(matches!(program.statements[0], Stmt::If { .. }));
    }
    
    #[test]
    fn test_parse_for() {
        // 条件循环
        let program = parse("for x < 10 { print(x) }").unwrap();
        assert_eq!(program.statements.len(), 1);
        assert!(matches!(program.statements[0], Stmt::While { .. }));
        
        // 无限循环
        let program = parse("for { break }").unwrap();
        assert_eq!(program.statements.len(), 1);
        if let Stmt::While { condition, .. } = &program.statements[0] {
            assert!(condition.is_none());
        } else {
            panic!("Expected While");
        }
    }
    
    #[test]
    fn test_parse_block() {
        let program = parse("{ var x = 1\nvar y = 2 }").unwrap();
        assert_eq!(program.statements.len(), 1);
        if let Stmt::Block { statements, .. } = &program.statements[0] {
            assert_eq!(statements.len(), 2);
        } else {
            panic!("Expected Block");
        }
    }
    
    #[test]
    fn test_parse_closure() {
        // 无参数无返回值
        let program = parse("var f = func() { println(42) }").unwrap();
        assert_eq!(program.statements.len(), 1);
        if let Stmt::VarDecl { initializer: Some(expr), .. } = &program.statements[0] {
            assert!(matches!(expr, Expr::Closure { .. }));
        } else {
            panic!("Expected VarDecl with Closure");
        }
        
        // 带参数和返回类型
        let program = parse("var add = func(a:int, b:int) int { return a + b }").unwrap();
        assert_eq!(program.statements.len(), 1);
        if let Stmt::VarDecl { initializer: Some(Expr::Closure { params, return_type, .. }), .. } = &program.statements[0] {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].name, "a");
            assert_eq!(params[1].name, "b");
            assert!(return_type.is_some());
        } else {
            panic!("Expected VarDecl with Closure");
        }
    }
}
