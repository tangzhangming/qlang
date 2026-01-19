//! 词法扫描器
//! 
//! 将源代码字符串转换为 Token 流

use super::token::{Token, TokenKind, Span};

/// 词法扫描器
pub struct Scanner {
    /// 源代码字符
    source: Vec<char>,
    /// 当前位置
    current: usize,
    /// 当前 token 起始位置
    start: usize,
    /// 当前行号
    line: usize,
    /// 当前列号
    column: usize,
    /// token 起始列号
    start_column: usize,
}

impl Scanner {
    /// 创建新的扫描器
    pub fn new(source: &str) -> Self {
        Self {
            source: source.chars().collect(),
            current: 0,
            start: 0,
            line: 1,
            column: 1,
            start_column: 1,
        }
    }

    /// 扫描所有 token
    pub fn scan_tokens(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        
        loop {
            let token = self.scan_token();
            let is_eof = token.is_eof();
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        
        tokens
    }

    /// 扫描单个 token
    pub fn scan_token(&mut self) -> Token {
        self.skip_whitespace();
        
        self.start = self.current;
        self.start_column = self.column;
        
        if self.is_at_end() {
            return self.make_token(TokenKind::Eof);
        }
        
        let c = self.advance();
        
        match c {
            // 换行
            '\n' => {
                let token = self.make_token(TokenKind::Newline);
                self.line += 1;
                self.column = 1;
                token
            }
            
            // 分隔符
            '(' => self.make_token(TokenKind::LeftParen),
            ')' => self.make_token(TokenKind::RightParen),
            '{' => self.make_token(TokenKind::LeftBrace),
            '}' => self.make_token(TokenKind::RightBrace),
            '[' => self.make_token(TokenKind::LeftBracket),
            ']' => self.make_token(TokenKind::RightBracket),
            ',' => self.make_token(TokenKind::Comma),
            ';' => self.make_token(TokenKind::Semicolon),
            '~' => self.make_token(TokenKind::Tilde),
            
            // . 和 .. 和 ..= 和 ...
            '.' => {
                if self.match_char('.') {
                    if self.match_char('.') {
                        self.make_token(TokenKind::DotDotDot)
                    } else if self.match_char('=') {
                        self.make_token(TokenKind::DotDotEqual)
                    } else {
                        self.make_token(TokenKind::DotDot)
                    }
                } else {
                    self.make_token(TokenKind::Dot)
                }
            }
            
            // : 和 ::
            ':' => {
                if self.match_char(':') {
                    self.make_token(TokenKind::ColonColon)
                } else {
                    self.make_token(TokenKind::Colon)
                }
            }
            
            // + 和 ++ 和 +=
            '+' => {
                if self.match_char('+') {
                    self.make_token(TokenKind::PlusPlus)
                } else if self.match_char('=') {
                    self.make_token(TokenKind::PlusEqual)
                } else {
                    self.make_token(TokenKind::Plus)
                }
            }
            
            // - 和 -- 和 -=
            '-' => {
                if self.match_char('-') {
                    self.make_token(TokenKind::MinusMinus)
                } else if self.match_char('=') {
                    self.make_token(TokenKind::MinusEqual)
                } else {
                    self.make_token(TokenKind::Minus)
                }
            }
            
            // * 和 ** 和 *=
            '*' => {
                if self.match_char('*') {
                    self.make_token(TokenKind::StarStar)
                } else if self.match_char('=') {
                    self.make_token(TokenKind::StarEqual)
                } else {
                    self.make_token(TokenKind::Star)
                }
            }
            
            // / 和 /= 和注释
            '/' => {
                if self.match_char('/') {
                    // 单行注释
                    self.skip_line_comment();
                    self.scan_token()
                } else if self.match_char('*') {
                    // 多行注释
                    self.skip_block_comment();
                    self.scan_token()
                } else if self.match_char('=') {
                    self.make_token(TokenKind::SlashEqual)
                } else {
                    self.make_token(TokenKind::Slash)
                }
            }
            
            // % 和 %=
            '%' => {
                if self.match_char('=') {
                    self.make_token(TokenKind::PercentEqual)
                } else {
                    self.make_token(TokenKind::Percent)
                }
            }
            
            // = 和 == 和 =>
            '=' => {
                if self.match_char('=') {
                    self.make_token(TokenKind::EqualEqual)
                } else if self.match_char('>') {
                    self.make_token(TokenKind::FatArrow)
                } else {
                    self.make_token(TokenKind::Equal)
                }
            }
            
            // ! 和 != 和 !.
            '!' => {
                if self.match_char('=') {
                    self.make_token(TokenKind::BangEqual)
                } else if self.match_char('.') {
                    self.make_token(TokenKind::BangDot)
                } else {
                    self.make_token(TokenKind::Bang)
                }
            }
            
            // < 和 <= 和 << 和 <<=
            '<' => {
                if self.match_char('<') {
                    if self.match_char('=') {
                        self.make_token(TokenKind::LessLessEqual)
                    } else {
                        self.make_token(TokenKind::LessLess)
                    }
                } else if self.match_char('=') {
                    self.make_token(TokenKind::LessEqual)
                } else {
                    self.make_token(TokenKind::Less)
                }
            }
            
            // > 和 >= 和 >> 和 >>=
            '>' => {
                if self.match_char('>') {
                    if self.match_char('=') {
                        self.make_token(TokenKind::GreaterGreaterEqual)
                    } else {
                        self.make_token(TokenKind::GreaterGreater)
                    }
                } else if self.match_char('=') {
                    self.make_token(TokenKind::GreaterEqual)
                } else {
                    self.make_token(TokenKind::Greater)
                }
            }
            
            // & 和 && 和 &=
            '&' => {
                if self.match_char('&') {
                    self.make_token(TokenKind::AmpAmp)
                } else if self.match_char('=') {
                    self.make_token(TokenKind::AmpEqual)
                } else {
                    self.make_token(TokenKind::Amp)
                }
            }
            
            // | 和 || 和 |=
            '|' => {
                if self.match_char('|') {
                    self.make_token(TokenKind::PipePipe)
                } else if self.match_char('=') {
                    self.make_token(TokenKind::PipeEqual)
                } else {
                    self.make_token(TokenKind::Pipe)
                }
            }
            
            // ^ 和 ^=
            '^' => {
                if self.match_char('=') {
                    self.make_token(TokenKind::CaretEqual)
                } else {
                    self.make_token(TokenKind::Caret)
                }
            }
            
            // ? 和 ?? 和 ?.
            '?' => {
                if self.match_char('?') {
                    self.make_token(TokenKind::QuestionQuestion)
                } else if self.match_char('.') {
                    self.make_token(TokenKind::QuestionDot)
                } else {
                    self.make_token(TokenKind::Question)
                }
            }
            
            // 字符串
            '"' => self.scan_string(),
            '\'' => self.scan_raw_string(),
            
            // 数字
            '0'..='9' => self.scan_number(),
            
            // 标识符或关键字（支持 Unicode）
            c if Self::is_identifier_start(c) => self.scan_identifier(),
            
            // 未知字符 - 更好的错误恢复
            _ => {
                // 尝试跳过无效字符并继续
                self.error_token(&format!("Unexpected character '{}' (U+{:04X})", c, c as u32))
            }
        }
    }

    /// 跳过空白字符（除了换行）
    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            match self.peek() {
                ' ' | '\r' | '\t' => {
                    self.advance();
                }
                _ => break,
            }
        }
    }

    /// 跳过单行注释
    fn skip_line_comment(&mut self) {
        while !self.is_at_end() && self.peek() != '\n' {
            self.advance();
        }
    }

    /// 跳过多行注释
    fn skip_block_comment(&mut self) {
        let mut depth = 1;
        while !self.is_at_end() && depth > 0 {
            if self.peek() == '/' && self.peek_next() == Some('*') {
                self.advance();
                self.advance();
                depth += 1;
            } else if self.peek() == '*' && self.peek_next() == Some('/') {
                self.advance();
                self.advance();
                depth -= 1;
            } else {
                if self.peek() == '\n' {
                    self.line += 1;
                    self.column = 0;
                }
                self.advance();
            }
        }
    }

    /// 扫描字符串（双引号，支持转义）
    fn scan_string(&mut self) -> Token {
        // 检查是否是三引号（多行字符串）
        if self.peek() == '"' && self.peek_next() == Some('"') {
            self.advance(); // 消费第二个 "
            self.advance(); // 消费第三个 "
            return self.scan_multiline_string();
        }
        
        let mut value = String::new();
        
        while !self.is_at_end() && self.peek() != '"' {
            if self.peek() == '\n' {
                self.line += 1;
                self.column = 0;
            }
            
            if self.peek() == '\\' {
                self.advance();
                if self.is_at_end() {
                    return self.error_token("Unterminated string");
                }
                match self.advance() {
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    'r' => value.push('\r'),
                    '\\' => value.push('\\'),
                    '"' => value.push('"'),
                    '$' => value.push('$'),
                    '0' => value.push('\0'),
                    c => {
                        value.push('\\');
                        value.push(c);
                    }
                }
            } else {
                value.push(self.advance());
            }
        }
        
        if self.is_at_end() {
            return self.error_token("Unterminated string");
        }
        
        // 消费闭合的引号
        self.advance();
        
        self.make_token(TokenKind::String(value))
    }
    
    /// 扫描多行字符串（三引号）
    fn scan_multiline_string(&mut self) -> Token {
        let mut value = String::new();
        
        // 跳过开头的换行符（如果有）
        if self.peek() == '\n' {
            self.advance();
            self.line += 1;
            self.column = 0;
        }
        
        while !self.is_at_end() {
            // 检查是否遇到结束的三引号
            if self.peek() == '"' && self.peek_next() == Some('"') {
                // 检查第三个引号
                let pos = self.current;
                self.advance(); // 消费第一个 "
                self.advance(); // 消费第二个 "
                if self.peek() == '"' {
                    self.advance(); // 消费第三个 "
                    return self.make_token(TokenKind::String(value));
                } else {
                    // 不是三引号，回退并添加到值中
                    self.current = pos;
                    value.push(self.advance());
                }
            } else if self.peek() == '\n' {
                value.push(self.advance());
                self.line += 1;
                self.column = 0;
            } else {
                value.push(self.advance());
            }
        }
        
        self.error_token("Unterminated multiline string")
    }

    /// 扫描原始字符串（单引号，不支持转义）
    fn scan_raw_string(&mut self) -> Token {
        let mut value = String::new();
        
        while !self.is_at_end() && self.peek() != '\'' {
            if self.peek() == '\n' {
                self.line += 1;
                self.column = 0;
            }
            value.push(self.advance());
        }
        
        if self.is_at_end() {
            return self.error_token("Unterminated string");
        }
        
        // 消费闭合的引号
        self.advance();
        
        self.make_token(TokenKind::RawString(value))
    }

    /// 扫描数字（支持各种进制和数字分隔符）
    fn scan_number(&mut self) -> Token {
        // 检查进制前缀
        let first_char = self.source[self.start];
        
        if first_char == '0' && !self.is_at_end() {
            match self.peek() {
                'x' | 'X' => return self.scan_hex_number(),
                'b' | 'B' => return self.scan_binary_number(),
                'o' | 'O' => return self.scan_octal_number(),
                _ => {}
            }
        }
        
        // 扫描十进制整数部分（支持下划线分隔符）
        while !self.is_at_end() && (self.peek().is_ascii_digit() || self.peek() == '_') {
            if self.peek() == '_' {
                // 下划线不能在数字开头或连续出现
                let prev = self.source.get(self.current - 1).copied().unwrap_or('0');
                if !prev.is_ascii_digit() {
                    return self.error_token("Invalid number: underscore must be between digits");
                }
            }
            self.advance();
        }
        
        // 检查下划线不能在数字末尾
        let last = self.source.get(self.current - 1).copied().unwrap_or('0');
        if last == '_' {
            return self.error_token("Invalid number: underscore cannot be at the end");
        }
        
        // 检查是否有小数部分
        let is_float = if self.peek() == '.' {
            if let Some(next) = self.peek_next() {
                if next.is_ascii_digit() {
                    self.advance(); // 消费 '.'
                    while !self.is_at_end() && (self.peek().is_ascii_digit() || self.peek() == '_') {
                        self.advance();
                    }
                    // 检查下划线不能在小数末尾
                    let last = self.source.get(self.current - 1).copied().unwrap_or('0');
                    if last == '_' {
                        return self.error_token("Invalid number: underscore cannot be at the end");
                    }
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        
        // 检查科学计数法
        let has_exponent = if self.peek() == 'e' || self.peek() == 'E' {
            self.advance(); // 消费 'e' 或 'E'
            if self.peek() == '+' || self.peek() == '-' {
                self.advance();
            }
            if !self.peek().is_ascii_digit() {
                return self.error_token("Invalid number: expected digit after exponent");
            }
            while !self.is_at_end() && (self.peek().is_ascii_digit() || self.peek() == '_') {
                self.advance();
            }
            true
        } else {
            false
        };
        
        // 收集数字字符（移除下划线）
        let lexeme: String = self.source[self.start..self.current]
            .iter()
            .filter(|&&c| c != '_')
            .collect();
        
        if is_float || has_exponent {
            match lexeme.parse::<f64>() {
                Ok(value) => self.make_token(TokenKind::Float(value)),
                Err(_) => self.error_token(&format!("Invalid float: {}", lexeme)),
            }
        } else {
            match lexeme.parse::<i64>() {
                Ok(value) => self.make_token(TokenKind::Integer(value)),
                Err(_) => self.error_token(&format!("Invalid integer: {}", lexeme)),
            }
        }
    }
    
    /// 扫描十六进制数字 (0x...)
    fn scan_hex_number(&mut self) -> Token {
        self.advance(); // 消费 'x' 或 'X'
        
        if !self.is_at_end() && !self.peek().is_ascii_hexdigit() {
            return self.error_token("Invalid hexadecimal number: expected hex digit after 0x");
        }
        
        while !self.is_at_end() && (self.peek().is_ascii_hexdigit() || self.peek() == '_') {
            if self.peek() == '_' {
                let prev = self.source.get(self.current - 1).copied().unwrap_or('0');
                if !prev.is_ascii_hexdigit() {
                    return self.error_token("Invalid number: underscore must be between digits");
                }
            }
            self.advance();
        }
        
        let last = self.source.get(self.current - 1).copied().unwrap_or('0');
        if last == '_' {
            return self.error_token("Invalid number: underscore cannot be at the end");
        }
        
        // 移除 0x 前缀和下划线
        let hex_str: String = self.source[self.start + 2..self.current]
            .iter()
            .filter(|&&c| c != '_')
            .collect();
        
        match i64::from_str_radix(&hex_str, 16) {
            Ok(value) => self.make_token(TokenKind::Integer(value)),
            Err(_) => self.error_token(&format!("Invalid hexadecimal number: 0x{}", hex_str)),
        }
    }
    
    /// 扫描二进制数字 (0b...)
    fn scan_binary_number(&mut self) -> Token {
        self.advance(); // 消费 'b' 或 'B'
        
        if !self.is_at_end() && !matches!(self.peek(), '0' | '1') {
            return self.error_token("Invalid binary number: expected 0 or 1 after 0b");
        }
        
        while !self.is_at_end() && (matches!(self.peek(), '0' | '1') || self.peek() == '_') {
            if self.peek() == '_' {
                let prev = self.source.get(self.current - 1).copied().unwrap_or('0');
                if !matches!(prev, '0' | '1') {
                    return self.error_token("Invalid number: underscore must be between digits");
                }
            }
            self.advance();
        }
        
        let last = self.source.get(self.current - 1).copied().unwrap_or('0');
        if last == '_' {
            return self.error_token("Invalid number: underscore cannot be at the end");
        }
        
        // 移除 0b 前缀和下划线
        let bin_str: String = self.source[self.start + 2..self.current]
            .iter()
            .filter(|&&c| c != '_')
            .collect();
        
        match i64::from_str_radix(&bin_str, 2) {
            Ok(value) => self.make_token(TokenKind::Integer(value)),
            Err(_) => self.error_token(&format!("Invalid binary number: 0b{}", bin_str)),
        }
    }
    
    /// 扫描八进制数字 (0o...)
    fn scan_octal_number(&mut self) -> Token {
        self.advance(); // 消费 'o' 或 'O'
        
        if !self.is_at_end() && !matches!(self.peek(), '0'..='7') {
            return self.error_token("Invalid octal number: expected 0-7 after 0o");
        }
        
        while !self.is_at_end() && (matches!(self.peek(), '0'..='7') || self.peek() == '_') {
            if self.peek() == '_' {
                let prev = self.source.get(self.current - 1).copied().unwrap_or('0');
                if !matches!(prev, '0'..='7') {
                    return self.error_token("Invalid number: underscore must be between digits");
                }
            }
            self.advance();
        }
        
        let last = self.source.get(self.current - 1).copied().unwrap_or('0');
        if last == '_' {
            return self.error_token("Invalid number: underscore cannot be at the end");
        }
        
        // 移除 0o 前缀和下划线
        let oct_str: String = self.source[self.start + 2..self.current]
            .iter()
            .filter(|&&c| c != '_')
            .collect();
        
        match i64::from_str_radix(&oct_str, 8) {
            Ok(value) => self.make_token(TokenKind::Integer(value)),
            Err(_) => self.error_token(&format!("Invalid octal number: 0o{}", oct_str)),
        }
    }

    /// 扫描标识符或关键字（支持 Unicode 标识符）
    fn scan_identifier(&mut self) -> Token {
        // 支持 Unicode 标识符：
        // - 第一个字符：字母、下划线、$、或 Unicode XID_Start
        // - 后续字符：字母数字、下划线、或 Unicode XID_Continue
        while !self.is_at_end() && self.is_identifier_continue(self.peek()) {
            self.advance();
        }
        
        let lexeme: String = self.source[self.start..self.current].iter().collect();
        let kind = self.identifier_type(&lexeme);
        
        self.make_token(kind)
    }
    
    /// 检查字符是否可以作为标识符开头
    fn is_identifier_start(c: char) -> bool {
        // 支持 ASCII 字母、下划线、$、以及 Unicode 字母
        c.is_alphabetic() || c == '_' || c == '$'
    }
    
    /// 检查字符是否可以作为标识符的后续字符
    fn is_identifier_continue(&self, c: char) -> bool {
        // 支持 ASCII 字母数字、下划线、以及 Unicode 字母和数字
        c.is_alphanumeric() || c == '_'
    }
    
    /// 识别关键字或返回标识符
    fn identifier_type(&self, lexeme: &str) -> TokenKind {
        match lexeme {
            // 声明关键字
            "var" => TokenKind::Var,
            "val" => TokenKind::Val,
            "const" => TokenKind::Const,
            "func" => TokenKind::Func,
            "struct" => TokenKind::Struct,
            "class" => TokenKind::Class,
            "interface" => TokenKind::Interface,
            "trait" => TokenKind::Trait,
            "use" => TokenKind::Use,
            "enum" => TokenKind::Enum,
            "type" => TokenKind::Type,
            
            // 可见性关键字
            "public" => TokenKind::Public,
            "internal" => TokenKind::Internal,
            "private" => TokenKind::Private,
            "protected" => TokenKind::Protected,
            
            // 类型关键字
            "int" => TokenKind::Int,
            "uint" => TokenKind::Uint,
            "i8" => TokenKind::I8,
            "i16" => TokenKind::I16,
            "i32" => TokenKind::I32,
            "i64" => TokenKind::I64,
            "u8" => TokenKind::U8,
            "u16" => TokenKind::U16,
            "u32" => TokenKind::U32,
            "u64" => TokenKind::U64,
            "f32" => TokenKind::F32,
            "f64" => TokenKind::F64,
            "bool" => TokenKind::Bool,
            "byte" => TokenKind::Byte,
            "char" => TokenKind::CharType,
            "string" => TokenKind::StringType,
            "unknown" => TokenKind::Unknown,
            "dynamic" => TokenKind::Dynamic,
            
            // 控制流关键字
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "for" => TokenKind::For,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "return" => TokenKind::Return,
            "match" => TokenKind::Match,
            "go" => TokenKind::Go,
            
            // 面向对象关键字
            "new" => TokenKind::New,
            "this" => TokenKind::This,
            "super" => TokenKind::Super,
            "extends" => TokenKind::Extends,
            "implements" => TokenKind::Implements,
            "abstract" => TokenKind::Abstract,
            "static" => TokenKind::Static,
            "override" => TokenKind::Override,
            
            // 字面量关键字
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "null" => TokenKind::Null,
            
            // 其他关键字
            "import" => TokenKind::Import,
            "package" => TokenKind::Package,
            "as" => TokenKind::As,
            "in" => TokenKind::In,
            "is" => TokenKind::Is,
            "try" => TokenKind::Try,
            "catch" => TokenKind::Catch,
            "finally" => TokenKind::Finally,
            "throw" => TokenKind::Throw,
            "make" => TokenKind::Make,
            "default" => TokenKind::Default,
            "sizeof" => TokenKind::Sizeof,
            "typeof" => TokenKind::Typeof,
            "panic" => TokenKind::Panic,
            "map" => TokenKind::Map,
            "with" => TokenKind::With,
            "where" => TokenKind::Where,
            
            // 通配符/下划线
            "_" => TokenKind::Underscore,
            
            // 不是关键字，返回标识符
            _ => TokenKind::Identifier(lexeme.to_string()),
        }
    }

    /// 判断是否到达源码末尾
    fn is_at_end(&self) -> bool {
        self.current >= self.source.len()
    }

    /// 前进一个字符并返回
    fn advance(&mut self) -> char {
        let c = self.source[self.current];
        self.current += 1;
        self.column += 1;
        c
    }

    /// 查看当前字符
    fn peek(&self) -> char {
        if self.is_at_end() {
            '\0'
        } else {
            self.source[self.current]
        }
    }

    /// 查看下一个字符
    fn peek_next(&self) -> Option<char> {
        if self.current + 1 >= self.source.len() {
            None
        } else {
            Some(self.source[self.current + 1])
        }
    }

    /// 如果当前字符匹配，则前进
    fn match_char(&mut self, expected: char) -> bool {
        if self.is_at_end() || self.source[self.current] != expected {
            false
        } else {
            self.current += 1;
            self.column += 1;
            true
        }
    }

    /// 创建 token
    fn make_token(&self, kind: TokenKind) -> Token {
        let lexeme: String = self.source[self.start..self.current].iter().collect();
        let span = Span::new(self.start, self.current, self.line, self.start_column);
        Token::new(kind, lexeme, span)
    }

    /// 创建错误 token
    fn error_token(&self, message: &str) -> Token {
        let span = Span::new(self.start, self.current, self.line, self.start_column);
        Token::new(
            TokenKind::Error(message.to_string()),
            String::new(),
            span,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_numbers() {
        let mut scanner = Scanner::new("123 45.67");
        let tokens = scanner.scan_tokens();
        
        assert!(matches!(tokens[0].kind, TokenKind::Integer(123)));
        assert!(matches!(tokens[1].kind, TokenKind::Float(f) if (f - 45.67).abs() < 0.001));
    }

    #[test]
    fn test_scan_strings() {
        let mut scanner = Scanner::new("\"hello\" 'world'");
        let tokens = scanner.scan_tokens();
        
        assert!(matches!(&tokens[0].kind, TokenKind::String(s) if s == "hello"));
        assert!(matches!(&tokens[1].kind, TokenKind::RawString(s) if s == "world"));
    }

    #[test]
    fn test_scan_operators() {
        let mut scanner = Scanner::new("+ - * / ** == != ++ -- += -= :: ..");
        let tokens = scanner.scan_tokens();
        
        assert!(matches!(tokens[0].kind, TokenKind::Plus));
        assert!(matches!(tokens[1].kind, TokenKind::Minus));
        assert!(matches!(tokens[2].kind, TokenKind::Star));
        assert!(matches!(tokens[3].kind, TokenKind::Slash));
        assert!(matches!(tokens[4].kind, TokenKind::StarStar));
        assert!(matches!(tokens[5].kind, TokenKind::EqualEqual));
        assert!(matches!(tokens[6].kind, TokenKind::BangEqual));
        assert!(matches!(tokens[7].kind, TokenKind::PlusPlus));
        assert!(matches!(tokens[8].kind, TokenKind::MinusMinus));
        assert!(matches!(tokens[9].kind, TokenKind::PlusEqual));
        assert!(matches!(tokens[10].kind, TokenKind::MinusEqual));
        assert!(matches!(tokens[11].kind, TokenKind::ColonColon));
        assert!(matches!(tokens[12].kind, TokenKind::DotDot));
    }
    
    #[test]
    fn test_scan_keywords() {
        let mut scanner = Scanner::new("var const func if else for return true false null");
        let tokens = scanner.scan_tokens();
        
        assert!(matches!(tokens[0].kind, TokenKind::Var));
        assert!(matches!(tokens[1].kind, TokenKind::Const));
        assert!(matches!(tokens[2].kind, TokenKind::Func));
        assert!(matches!(tokens[3].kind, TokenKind::If));
        assert!(matches!(tokens[4].kind, TokenKind::Else));
        assert!(matches!(tokens[5].kind, TokenKind::For));
        assert!(matches!(tokens[6].kind, TokenKind::Return));
        assert!(matches!(tokens[7].kind, TokenKind::True));
        assert!(matches!(tokens[8].kind, TokenKind::False));
        assert!(matches!(tokens[9].kind, TokenKind::Null));
    }
    
    #[test]
    fn test_scan_type_keywords() {
        let mut scanner = Scanner::new("int i32 i64 f32 f64 bool string char");
        let tokens = scanner.scan_tokens();
        
        assert!(matches!(tokens[0].kind, TokenKind::Int));
        assert!(matches!(tokens[1].kind, TokenKind::I32));
        assert!(matches!(tokens[2].kind, TokenKind::I64));
        assert!(matches!(tokens[3].kind, TokenKind::F32));
        assert!(matches!(tokens[4].kind, TokenKind::F64));
        assert!(matches!(tokens[5].kind, TokenKind::Bool));
        assert!(matches!(tokens[6].kind, TokenKind::StringType));
        assert!(matches!(tokens[7].kind, TokenKind::CharType));
    }
    
    #[test]
    fn test_scan_identifiers() {
        let mut scanner = Scanner::new("foo bar_baz MyClass _private $name $value");
        let tokens = scanner.scan_tokens();
        
        assert!(matches!(&tokens[0].kind, TokenKind::Identifier(s) if s == "foo"));
        assert!(matches!(&tokens[1].kind, TokenKind::Identifier(s) if s == "bar_baz"));
        assert!(matches!(&tokens[2].kind, TokenKind::Identifier(s) if s == "MyClass"));
        assert!(matches!(&tokens[3].kind, TokenKind::Identifier(s) if s == "_private"));
        // $ 开头的变量名（类似 PHP 风格，可选）
        assert!(matches!(&tokens[4].kind, TokenKind::Identifier(s) if s == "$name"));
        assert!(matches!(&tokens[5].kind, TokenKind::Identifier(s) if s == "$value"));
    }
    
    #[test]
    fn test_scan_var_declaration() {
        let mut scanner = Scanner::new("var x:int = 10");
        let tokens = scanner.scan_tokens();
        
        assert!(matches!(tokens[0].kind, TokenKind::Var));
        assert!(matches!(&tokens[1].kind, TokenKind::Identifier(s) if s == "x"));
        assert!(matches!(tokens[2].kind, TokenKind::Colon));
        assert!(matches!(tokens[3].kind, TokenKind::Int));
        assert!(matches!(tokens[4].kind, TokenKind::Equal));
        assert!(matches!(tokens[5].kind, TokenKind::Integer(10)));
    }
}
