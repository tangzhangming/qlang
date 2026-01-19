//! Token 定义
//! 
//! 词法分析器产生的标记类型

#![allow(dead_code)]

use std::fmt;

/// Token 类型
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // ============ 字面量 ============
    /// 整数字面量
    Integer(i64),
    /// 浮点数字面量
    Float(f64),
    /// 字符串字面量（双引号，支持插值）
    String(String),
    /// 原始字符串字面量（单引号，不支持插值）
    RawString(String),
    /// 字符字面量
    Char(char),

    // ============ 标识符和关键字 ============
    /// 标识符
    Identifier(String),

    // ============ 声明关键字 ============
    /// var
    Var,
    /// const
    Const,
    /// func
    Func,
    /// struct
    Struct,
    /// class
    Class,
    /// interface
    Interface,
    /// trait
    Trait,
    /// use (for trait inclusion)
    Use,
    /// enum
    Enum,
    /// type
    Type,

    // ============ 可见性关键字 ============
    /// public（默认可见性）
    Public,
    /// internal（模块内可见）
    Internal,
    /// private（仅当前文件/类可见）
    Private,
    /// protected（当前类和子类可见）
    Protected,

    // ============ 类型关键字 ============
    /// int
    Int,
    /// uint
    Uint,
    /// i8
    I8,
    /// i16
    I16,
    /// i32
    I32,
    /// i64
    I64,
    /// u8
    U8,
    /// u16
    U16,
    /// u32
    U32,
    /// u64
    U64,
    /// f32
    F32,
    /// f64
    F64,
    /// bool
    Bool,
    /// byte
    Byte,
    /// char
    CharType,
    /// string
    StringType,
    /// unknown
    Unknown,
    /// dynamic
    Dynamic,

    // ============ 控制流关键字 ============
    /// if
    If,
    /// else
    Else,
    /// for
    For,
    /// break
    Break,
    /// continue
    Continue,
    /// return
    Return,
    /// match
    Match,
    /// go
    Go,

    // ============ 面向对象关键字 ============
    /// new
    New,
    /// this
    This,
    /// super
    Super,
    /// extends
    Extends,
    /// implements
    Implements,
    /// abstract
    Abstract,
    /// static
    Static,
    /// override
    Override,

    // ============ 字面量关键字 ============
    /// true
    True,
    /// false
    False,
    /// null
    Null,

    // ============ 其他关键字 ============
    /// import
    Import,
    /// package
    Package,
    /// as
    As,
    /// in
    In,
    /// is
    Is,
    /// try
    Try,
    /// catch
    Catch,
    /// finally
    Finally,
    /// throw
    Throw,
    /// make
    Make,
    /// default
    Default,
    /// sizeof
    Sizeof,
    /// typeof
    Typeof,
    /// panic
    Panic,
    /// map
    Map,
    /// with
    With,

    // ============ 算术运算符 ============
    /// +
    Plus,
    /// -
    Minus,
    /// *
    Star,
    /// /
    Slash,
    /// %
    Percent,
    /// **
    StarStar,

    // ============ 位运算符 ============
    /// &
    Amp,
    /// |
    Pipe,
    /// ^
    Caret,
    /// ~
    Tilde,
    /// <<
    LessLess,
    /// >>
    GreaterGreater,

    // ============ 比较运算符 ============
    /// ==
    EqualEqual,
    /// !=
    BangEqual,
    /// <
    Less,
    /// <=
    LessEqual,
    /// >
    Greater,
    /// >=
    GreaterEqual,

    // ============ 逻辑运算符 ============
    /// !
    Bang,
    /// &&
    AmpAmp,
    /// ||
    PipePipe,

    // ============ 赋值运算符 ============
    /// =
    Equal,
    /// +=
    PlusEqual,
    /// -=
    MinusEqual,
    /// *=
    StarEqual,
    /// /=
    SlashEqual,
    /// %=
    PercentEqual,
    /// &=
    AmpEqual,
    /// |=
    PipeEqual,
    /// ^=
    CaretEqual,
    /// <<=
    LessLessEqual,
    /// >>=
    GreaterGreaterEqual,

    // ============ 自增自减 ============
    /// ++
    PlusPlus,
    /// --
    MinusMinus,

    // ============ 可空相关运算符 ============
    /// ?
    Question,
    /// ??
    QuestionQuestion,
    /// ?.
    QuestionDot,
    /// !.
    BangDot,

    // ============ 范围运算符 ============
    /// ..
    DotDot,
    /// ..=
    DotDotEqual,
    /// ... (可变参数)
    DotDotDot,

    // ============ 其他运算符 ============
    /// =>
    FatArrow,
    /// ::
    ColonColon,
    /// _ (下划线/通配符)
    Underscore,

    // ============ 分隔符 ============
    /// (
    LeftParen,
    /// )
    RightParen,
    /// {
    LeftBrace,
    /// }
    RightBrace,
    /// [
    LeftBracket,
    /// ]
    RightBracket,
    /// ,
    Comma,
    /// .
    Dot,
    /// :
    Colon,
    /// ;
    Semicolon,

    // ============ 特殊 ============
    /// 换行
    Newline,
    /// 文件结束
    Eof,
    /// 错误 token
    Error(String),
}

/// 源码位置信息
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    /// 起始位置（字节偏移）
    pub start: usize,
    /// 结束位置（字节偏移）
    pub end: usize,
    /// 行号（从1开始）
    pub line: usize,
    /// 列号（从1开始）
    pub column: usize,
}

impl Span {
    /// 创建新的位置信息
    pub fn new(start: usize, end: usize, line: usize, column: usize) -> Self {
        Self { start, end, line, column }
    }
}

/// Token 结构
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// Token 类型
    pub kind: TokenKind,
    /// 原始文本
    pub lexeme: String,
    /// 位置信息
    pub span: Span,
}

impl Token {
    /// 创建新的 Token
    pub fn new(kind: TokenKind, lexeme: String, span: Span) -> Self {
        Self { kind, lexeme, span }
    }

    /// 判断是否是指定类型
    pub fn is(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.kind) == std::mem::discriminant(kind)
    }

    /// 判断是否是文件结束
    pub fn is_eof(&self) -> bool {
        matches!(self.kind, TokenKind::Eof)
    }

    /// 判断是否是错误
    pub fn is_error(&self) -> bool {
        matches!(self.kind, TokenKind::Error(_))
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} '{}' at {}:{}", self.kind, self.lexeme, self.span.line, self.span.column)
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // 字面量
            TokenKind::Integer(n) => write!(f, "{}", n),
            TokenKind::Float(n) => write!(f, "{}", n),
            TokenKind::String(s) => write!(f, "\"{}\"", s),
            TokenKind::RawString(s) => write!(f, "'{}'", s),
            TokenKind::Char(c) => write!(f, "'{}'", c),
            TokenKind::Identifier(s) => write!(f, "{}", s),
            
            // 声明关键字
            TokenKind::Var => write!(f, "var"),
            TokenKind::Const => write!(f, "const"),
            TokenKind::Func => write!(f, "func"),
            TokenKind::Struct => write!(f, "struct"),
            TokenKind::Class => write!(f, "class"),
            TokenKind::Interface => write!(f, "interface"),
            TokenKind::Trait => write!(f, "trait"),
            TokenKind::Use => write!(f, "use"),
            TokenKind::Enum => write!(f, "enum"),
            TokenKind::Type => write!(f, "type"),
            
            // 可见性关键字
            TokenKind::Public => write!(f, "public"),
            TokenKind::Internal => write!(f, "internal"),
            TokenKind::Private => write!(f, "private"),
            TokenKind::Protected => write!(f, "protected"),
            
            // 类型关键字
            TokenKind::Int => write!(f, "int"),
            TokenKind::Uint => write!(f, "uint"),
            TokenKind::I8 => write!(f, "i8"),
            TokenKind::I16 => write!(f, "i16"),
            TokenKind::I32 => write!(f, "i32"),
            TokenKind::I64 => write!(f, "i64"),
            TokenKind::U8 => write!(f, "u8"),
            TokenKind::U16 => write!(f, "u16"),
            TokenKind::U32 => write!(f, "u32"),
            TokenKind::U64 => write!(f, "u64"),
            TokenKind::F32 => write!(f, "f32"),
            TokenKind::F64 => write!(f, "f64"),
            TokenKind::Bool => write!(f, "bool"),
            TokenKind::Byte => write!(f, "byte"),
            TokenKind::CharType => write!(f, "char"),
            TokenKind::StringType => write!(f, "string"),
            TokenKind::Unknown => write!(f, "unknown"),
            TokenKind::Dynamic => write!(f, "dynamic"),
            
            // 控制流关键字
            TokenKind::If => write!(f, "if"),
            TokenKind::Else => write!(f, "else"),
            TokenKind::For => write!(f, "for"),
            TokenKind::Break => write!(f, "break"),
            TokenKind::Continue => write!(f, "continue"),
            TokenKind::Return => write!(f, "return"),
            TokenKind::Match => write!(f, "match"),
            TokenKind::Go => write!(f, "go"),
            
            // 面向对象关键字
            TokenKind::New => write!(f, "new"),
            TokenKind::This => write!(f, "this"),
            TokenKind::Super => write!(f, "super"),
            TokenKind::Extends => write!(f, "extends"),
            TokenKind::Implements => write!(f, "implements"),
            TokenKind::Abstract => write!(f, "abstract"),
            TokenKind::Static => write!(f, "static"),
            TokenKind::Override => write!(f, "override"),
            
            // 字面量关键字
            TokenKind::True => write!(f, "true"),
            TokenKind::False => write!(f, "false"),
            TokenKind::Null => write!(f, "null"),
            
            // 其他关键字
            TokenKind::Import => write!(f, "import"),
            TokenKind::Package => write!(f, "package"),
            TokenKind::As => write!(f, "as"),
            TokenKind::In => write!(f, "in"),
            TokenKind::Is => write!(f, "is"),
            TokenKind::Try => write!(f, "try"),
            TokenKind::Catch => write!(f, "catch"),
            TokenKind::Finally => write!(f, "finally"),
            TokenKind::Throw => write!(f, "throw"),
            TokenKind::Make => write!(f, "make"),
            TokenKind::Default => write!(f, "default"),
            TokenKind::Sizeof => write!(f, "sizeof"),
            TokenKind::Typeof => write!(f, "typeof"),
            TokenKind::Panic => write!(f, "panic"),
            TokenKind::Map => write!(f, "map"),
            TokenKind::With => write!(f, "with"),
            
            // 算术运算符
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Star => write!(f, "*"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::Percent => write!(f, "%"),
            TokenKind::StarStar => write!(f, "**"),
            
            // 位运算符
            TokenKind::Amp => write!(f, "&"),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::Caret => write!(f, "^"),
            TokenKind::Tilde => write!(f, "~"),
            TokenKind::LessLess => write!(f, "<<"),
            TokenKind::GreaterGreater => write!(f, ">>"),
            
            // 比较运算符
            TokenKind::EqualEqual => write!(f, "=="),
            TokenKind::BangEqual => write!(f, "!="),
            TokenKind::Less => write!(f, "<"),
            TokenKind::LessEqual => write!(f, "<="),
            TokenKind::Greater => write!(f, ">"),
            TokenKind::GreaterEqual => write!(f, ">="),
            
            // 逻辑运算符
            TokenKind::Bang => write!(f, "!"),
            TokenKind::AmpAmp => write!(f, "&&"),
            TokenKind::PipePipe => write!(f, "||"),
            
            // 赋值运算符
            TokenKind::Equal => write!(f, "="),
            TokenKind::PlusEqual => write!(f, "+="),
            TokenKind::MinusEqual => write!(f, "-="),
            TokenKind::StarEqual => write!(f, "*="),
            TokenKind::SlashEqual => write!(f, "/="),
            TokenKind::PercentEqual => write!(f, "%="),
            TokenKind::AmpEqual => write!(f, "&="),
            TokenKind::PipeEqual => write!(f, "|="),
            TokenKind::CaretEqual => write!(f, "^="),
            TokenKind::LessLessEqual => write!(f, "<<="),
            TokenKind::GreaterGreaterEqual => write!(f, ">>="),
            
            // 自增自减
            TokenKind::PlusPlus => write!(f, "++"),
            TokenKind::MinusMinus => write!(f, "--"),
            
            // 可空相关
            TokenKind::Question => write!(f, "?"),
            TokenKind::QuestionQuestion => write!(f, "??"),
            TokenKind::QuestionDot => write!(f, "?."),
            TokenKind::BangDot => write!(f, "!."),
            
            // 范围运算符
            TokenKind::DotDot => write!(f, ".."),
            TokenKind::DotDotEqual => write!(f, "..="),
            TokenKind::DotDotDot => write!(f, "..."),
            
            // 其他运算符
            TokenKind::FatArrow => write!(f, "=>"),
            TokenKind::ColonColon => write!(f, "::"),
            TokenKind::Underscore => write!(f, "_"),
            
            // 分隔符
            TokenKind::LeftParen => write!(f, "("),
            TokenKind::RightParen => write!(f, ")"),
            TokenKind::LeftBrace => write!(f, "{{"),
            TokenKind::RightBrace => write!(f, "}}"),
            TokenKind::LeftBracket => write!(f, "["),
            TokenKind::RightBracket => write!(f, "]"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Dot => write!(f, "."),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::Semicolon => write!(f, ";"),
            
            // 特殊
            TokenKind::Newline => write!(f, "\\n"),
            TokenKind::Eof => write!(f, "EOF"),
            TokenKind::Error(msg) => write!(f, "Error: {}", msg),
        }
    }
}
