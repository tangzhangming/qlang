//! 抽象语法树（AST）定义
//! 
//! 表示程序的树形结构

#![allow(dead_code)]

use crate::lexer::Span;
use crate::types::Type;

/// 二元运算符
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    // 算术运算符
    /// +
    Add,
    /// -
    Sub,
    /// *
    Mul,
    /// /
    Div,
    /// %
    Mod,
    /// **
    Pow,
    
    // 比较运算符
    /// ==
    Eq,
    /// !=
    Ne,
    /// <
    Lt,
    /// <=
    Le,
    /// >
    Gt,
    /// >=
    Ge,
    
    // 逻辑运算符
    /// &&
    And,
    /// ||
    Or,
    
    // 位运算符
    /// &
    BitAnd,
    /// |
    BitOr,
    /// ^
    BitXor,
    /// <<
    Shl,
    /// >>
    Shr,
}

/// 一元运算符
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// -（负号）
    Neg,
    /// !（逻辑非）
    Not,
    /// ~（按位取反）
    BitNot,
}

/// 赋值运算符
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    /// =
    Assign,
    /// +=
    AddAssign,
    /// -=
    SubAssign,
    /// *=
    MulAssign,
    /// /=
    DivAssign,
    /// %=
    ModAssign,
    /// &=
    BitAndAssign,
    /// |=
    BitOrAssign,
    /// ^=
    BitXorAssign,
    /// <<=
    ShlAssign,
    /// >>=
    ShrAssign,
}

/// 类型注解
#[derive(Debug, Clone, PartialEq)]
pub struct TypeAnnotation {
    /// 类型
    pub ty: Type,
    /// 位置信息
    pub span: Span,
}

/// 泛型类型参数（如 T、K、V）
#[derive(Debug, Clone, PartialEq)]
pub struct TypeParam {
    /// 参数名（如 T）
    pub name: String,
    /// 类型约束（如 T: Comparable<T> + Printable）
    pub bounds: Vec<crate::types::TypeBound>,
    /// 默认类型（可选）
    pub default_type: Option<TypeAnnotation>,
    /// 位置信息
    pub span: Span,
}

/// Where 子句约束项（如 where T: Comparable<T>, U: Printable）
#[derive(Debug, Clone, PartialEq)]
pub struct WhereClause {
    /// 被约束的类型参数名
    pub type_param: String,
    /// 约束列表
    pub bounds: Vec<crate::types::TypeBound>,
    /// 位置信息
    pub span: Span,
}

/// 字符串插值的组成部分
#[derive(Debug, Clone, PartialEq)]
pub enum StringInterpPart {
    /// 字符串字面量部分
    Literal(String),
    /// 表达式部分
    Expr(Expr),
}

/// 函数参数
#[derive(Debug, Clone, PartialEq)]
pub struct FnParam {
    /// 参数名
    pub name: String,
    /// 参数类型
    pub type_ann: TypeAnnotation,
    /// 默认值（可选）
    pub default: Option<Expr>,
    /// 是否是可变参数（如 numbers:int...）
    pub variadic: bool,
    /// 是否自动提升为类字段（构造函数参数属性提升）
    /// 仅在 init 方法参数前有 var/val/const 时为 true
    pub is_field: bool,
    /// 字段是否可变（var=true, val/const=false）
    /// 仅当 is_field=true 时有意义
    pub is_mutable: bool,
    /// 字段可见性（仅当 is_field=true 时有意义）
    pub field_visibility: Option<Visibility>,
    /// 位置信息
    pub span: Span,
}

/// 表达式节点
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// 整数字面量
    Integer {
        value: i64,
        span: Span,
    },
    /// 浮点数字面量
    Float {
        value: f64,
        span: Span,
    },
    /// 字符串字面量
    String {
        value: String,
        span: Span,
    },
    /// 字符串插值 "Hello, ${name}!"
    StringInterpolation {
        /// 字符串部分和表达式交替：[StringPart, Expr, StringPart, Expr, ...]
        parts: Vec<StringInterpPart>,
        span: Span,
    },
    /// 布尔字面量
    Bool {
        value: bool,
        span: Span,
    },
    /// 字符字面量
    Char {
        value: char,
        span: Span,
    },
    /// null 字面量
    Null {
        span: Span,
    },
    /// 标识符
    Identifier {
        name: String,
        span: Span,
    },
    /// 二元表达式
    Binary {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
        span: Span,
    },
    /// 一元表达式
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },
    /// 分组表达式（括号）
    Grouping {
        expr: Box<Expr>,
        span: Span,
    },
    /// 函数调用参数（支持命名参数）
    /// 
    /// 位置参数: (None, expr)
    /// 命名参数: (Some("name"), expr)
    /// 
    /// 注意: 命名参数必须在位置参数之后
    Call {
        callee: Box<Expr>,
        /// 参数列表：(参数名（如果是命名参数）, 参数值)
        args: Vec<(Option<String>, Expr)>,
        span: Span,
    },
    /// go 表达式：启动协程
    Go {
        call: Box<Expr>,  // 必须是一个 Call 表达式
        span: Span,
    },
    /// 赋值表达式
    Assign {
        target: Box<Expr>,
        op: AssignOp,
        value: Box<Expr>,
        span: Span,
    },
    /// 索引表达式 a[i]
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    /// 成员访问 a.b
    Member {
        object: Box<Expr>,
        member: String,
        span: Span,
    },
    /// 安全成员访问 a?.b
    SafeMember {
        object: Box<Expr>,
        member: String,
        span: Span,
    },
    /// 非空断言 a!.b
    NonNullMember {
        object: Box<Expr>,
        member: String,
        span: Span,
    },
    /// 空值合并 a ?? b
    NullCoalesce {
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    /// 后缀自增 a++
    PostIncrement {
        operand: Box<Expr>,
        span: Span,
    },
    /// 后缀自减 a--
    PostDecrement {
        operand: Box<Expr>,
        span: Span,
    },
    /// 类型转换 a as T
    Cast {
        expr: Box<Expr>,
        target_type: TypeAnnotation,
        force: bool, // true for as!, false for as
        span: Span,
    },
    /// 类型检查 a is T
    TypeCheck {
        expr: Box<Expr>,
        check_type: TypeAnnotation,
        span: Span,
    },
    /// 范围表达式 a..b 或 a..=b
    Range {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        inclusive: bool,
        span: Span,
    },
    /// if 表达式（返回值）
    IfExpr {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
        span: Span,
    },
    /// 数组字面量 [1, 2, 3]
    Array {
        elements: Vec<Expr>,
        span: Span,
    },
    /// Map 字面量 { "a": 1, "b": 2 }
    MapLiteral {
        entries: Vec<(Expr, Expr)>,
        span: Span,
    },
    /// 闭包表达式 fn(a:int, b:int) int { return a + b }
    Closure {
        /// 参数列表
        params: Vec<FnParam>,
        /// 返回类型（可选，支持类型推导）
        return_type: Option<TypeAnnotation>,
        /// 函数体
        body: Box<Stmt>,
        /// 位置信息
        span: Span,
    },
    /// struct 字面量 Point { x: 1, y: 2 }
    StructLiteral {
        /// 结构体名称
        name: String,
        /// 字段赋值
        fields: Vec<(String, Expr)>,
        /// 位置信息
        span: Span,
    },
    /// new 表达式 new MyClass(args)
    New {
        /// 类名
        class_name: String,
        /// 构造参数
        args: Vec<Expr>,
        /// 位置信息
        span: Span,
    },
    /// this 关键字
    This {
        span: Span,
    },
    /// super 关键字
    Super {
        span: Span,
    },
    /// default 关键字（默认初始化）
    Default {
        type_name: String,
        span: Span,
    },
    /// 静态成员访问 ClassName::member
    StaticMember {
        class_name: String,
        member: String,
        span: Span,
    },
}

impl Expr {
    /// 获取表达式的位置信息
    pub fn span(&self) -> Span {
        match self {
            Expr::Integer { span, .. } => *span,
            Expr::Float { span, .. } => *span,
            Expr::String { span, .. } => *span,
            Expr::StringInterpolation { span, .. } => *span,
            Expr::Bool { span, .. } => *span,
            Expr::Char { span, .. } => *span,
            Expr::Null { span } => *span,
            Expr::Identifier { span, .. } => *span,
            Expr::Binary { span, .. } => *span,
            Expr::Unary { span, .. } => *span,
            Expr::Grouping { span, .. } => *span,
            Expr::Call { span, .. } => *span,
            Expr::Go { span, .. } => *span,
            Expr::Assign { span, .. } => *span,
            Expr::Index { span, .. } => *span,
            Expr::Member { span, .. } => *span,
            Expr::SafeMember { span, .. } => *span,
            Expr::NonNullMember { span, .. } => *span,
            Expr::NullCoalesce { span, .. } => *span,
            Expr::PostIncrement { span, .. } => *span,
            Expr::PostDecrement { span, .. } => *span,
            Expr::Cast { span, .. } => *span,
            Expr::TypeCheck { span, .. } => *span,
            Expr::Range { span, .. } => *span,
            Expr::StructLiteral { span, .. } => *span,
            Expr::New { span, .. } => *span,
            Expr::This { span } => *span,
            Expr::Super { span } => *span,
            Expr::Default { span, .. } => *span,
            Expr::StaticMember { span, .. } => *span,
            Expr::IfExpr { span, .. } => *span,
            Expr::Array { span, .. } => *span,
            Expr::MapLiteral { span, .. } => *span,
            Expr::Closure { span, .. } => *span,
        }
    }
    
    /// 判断表达式是否可作为赋值目标（左值）
    pub fn is_lvalue(&self) -> bool {
        matches!(
            self,
            Expr::Identifier { .. } | Expr::Index { .. } | Expr::Member { .. }
        )
    }
}

/// 语句节点
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// 表达式语句
    Expression {
        expr: Expr,
        span: Span,
    },
    /// Print 语句（内置）
    Print {
        expr: Expr,
        newline: bool, // true for println, false for print
        span: Span,
    },
    /// 变量声明
    VarDecl {
        name: String,
        type_ann: Option<TypeAnnotation>,
        initializer: Option<Expr>,
        span: Span,
    },
    /// 常量声明
    ConstDecl {
        name: String,
        type_ann: Option<TypeAnnotation>,
        initializer: Expr,
        span: Span,
    },
    /// 块语句
    Block {
        statements: Vec<Stmt>,
        span: Span,
    },
    /// if 语句
    If {
        condition: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
        span: Span,
    },
    /// for 循环（C 风格）
    ForLoop {
        label: Option<String>,
        initializer: Option<Box<Stmt>>,
        condition: Option<Expr>,
        increment: Option<Expr>,
        body: Box<Stmt>,
        span: Span,
    },
    /// for-in 循环
    ForIn {
        label: Option<String>,
        /// 循环变量（可能有两个，如 for i, v in array）
        variables: Vec<String>,
        iterable: Expr,
        body: Box<Stmt>,
        span: Span,
    },
    /// 条件循环（for condition {}）
    While {
        label: Option<String>,
        condition: Option<Expr>, // None 表示无限循环
        body: Box<Stmt>,
        span: Span,
    },
    /// break 语句
    Break {
        label: Option<String>,
        span: Span,
    },
    /// continue 语句
    Continue {
        label: Option<String>,
        span: Span,
    },
    /// return 语句
    Return {
        value: Option<Expr>,
        span: Span,
    },
    /// match 表达式/语句
    Match {
        expr: Expr,
        arms: Vec<MatchArm>,
        span: Span,
    },
    /// struct 定义
    StructDef {
        name: String,
        /// 泛型类型参数（如 struct Pair<K, V>）
        type_params: Vec<TypeParam>,
        /// where 子句约束
        where_clauses: Vec<WhereClause>,
        /// 实现的接口列表
        interfaces: Vec<String>,
        fields: Vec<StructField>,
        methods: Vec<StructMethod>,
        span: Span,
    },
    /// class 定义
    ClassDef {
        name: String,
        /// 泛型类型参数（如 class List<T>）
        type_params: Vec<TypeParam>,
        /// where 子句约束
        where_clauses: Vec<WhereClause>,
        /// 是否是抽象类
        is_abstract: bool,
        parent: Option<String>,
        interfaces: Vec<String>,
        /// 使用的 trait 列表
        traits: Vec<String>,
        fields: Vec<ClassField>,
        methods: Vec<ClassMethod>,
        span: Span,
    },
    /// interface 定义
    InterfaceDef {
        name: String,
        /// 泛型类型参数
        type_params: Vec<TypeParam>,
        /// 父接口
        super_interfaces: Vec<String>,
        methods: Vec<InterfaceMethod>,
        span: Span,
    },
    /// trait 定义
    TraitDef {
        name: String,
        /// 泛型类型参数（如 trait Comparable<T>）
        type_params: Vec<TypeParam>,
        /// where 子句约束
        where_clauses: Vec<WhereClause>,
        /// 父 trait
        super_traits: Vec<crate::types::TypeBound>,
        methods: Vec<TraitMethod>,
        span: Span,
    },
    /// enum 定义
    EnumDef {
        name: String,
        variants: Vec<EnumVariant>,
        span: Span,
    },
    /// 类型别名
    TypeAlias {
        name: String,
        target_type: TypeAnnotation,
        span: Span,
    },
    /// try-catch 语句
    TryCatch {
        try_block: Box<Stmt>,
        catch_param: Option<String>,  // catch 的参数名，如 catch(e)
        catch_type: Option<String>,   // catch 的异常类型，如 catch(e:Exception)
        catch_block: Box<Stmt>,
        finally_block: Option<Box<Stmt>>,
        span: Span,
    },
    /// throw 语句
    Throw {
        value: Expr,
        span: Span,
    },
    /// 命名函数定义（包级函数）
    FnDef {
        name: String,
        /// 泛型类型参数（如 func map<T, U>(...)）
        type_params: Vec<TypeParam>,
        /// where 子句约束
        where_clauses: Vec<WhereClause>,
        params: Vec<FnParam>,
        return_type: Option<TypeAnnotation>,
        body: Box<Stmt>,
        visibility: Visibility,
        span: Span,
    },
    /// 包声明（必须是文件第一条非注释语句）
    Package {
        /// 包路径，如 "com.example.demo"
        path: String,
        span: Span,
    },
    /// 导入声明
    Import {
        import: ImportDecl,
        span: Span,
    },
}

/// 导入声明
#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    /// 导入路径（不含最后部分），如 "com.example.models"
    pub path: String,
    /// 导入的目标
    pub target: ImportTarget,
}

/// 导入目标类型
#[derive(Debug, Clone, PartialEq)]
pub enum ImportTarget {
    /// 导入所有公开成员：import com.example.services.*
    All,
    /// 导入指定成员：import com.example.models.UserModel
    Single(String),
    /// 导入多个成员：import com.example.models.{User, Product}
    Multiple(Vec<String>),
}

/// match 分支
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    /// 模式
    pub pattern: MatchPattern,
    /// 守卫条件（可选）
    pub guard: Option<Expr>,
    /// 分支体
    pub body: Box<Stmt>,
    /// 位置信息
    pub span: Span,
}

/// match 模式
#[derive(Debug, Clone, PartialEq)]
pub enum MatchPattern {
    /// 字面量模式
    Literal(Expr),
    /// 变量绑定模式
    Variable(String),
    /// 通配符模式 _
    Wildcard,
    /// 多值模式 1, 2, 3
    Or(Vec<MatchPattern>),
    /// 范围模式 1..10
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        inclusive: bool,
    },
    /// 类型模式 x:Type
    Type {
        name: String,
        type_ann: TypeAnnotation,
    },
}

/// 可见性修饰符（Kotlin 风格）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// 公开 (public) - 默认可见性，任何地方都可以访问
    Public,
    /// 模块内可见 (internal) - 同一模块内可见
    Internal,
    /// 私有 (private) - 仅当前文件/类可见
    Private,
    /// 保护 (protected) - 当前类和子类可见
    Protected,
}

impl Default for Visibility {
    fn default() -> Self {
        // Kotlin 风格：默认为 public
        Visibility::Public
    }
}

/// struct 字段
#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub name: String,
    pub type_ann: TypeAnnotation,
    pub visibility: Visibility,
    pub span: Span,
}

/// struct 方法
#[derive(Debug, Clone, PartialEq)]
pub struct StructMethod {
    pub name: String,
    pub params: Vec<FnParam>,
    pub return_type: Option<TypeAnnotation>,
    pub body: Box<Stmt>,
    pub visibility: Visibility,
    pub span: Span,
}

/// class 字段
#[derive(Debug, Clone, PartialEq)]
pub struct ClassField {
    pub name: String,
    pub type_ann: Option<TypeAnnotation>,
    pub initializer: Option<Expr>,
    pub visibility: Visibility,
    pub is_static: bool,
    /// 是否是常量（static const）
    pub is_const: bool,
    pub span: Span,
}

/// class 方法
#[derive(Debug, Clone, PartialEq)]
pub struct ClassMethod {
    pub name: String,
    pub params: Vec<FnParam>,
    pub return_type: Option<TypeAnnotation>,
    /// 方法体（抽象方法没有方法体）
    pub body: Option<Box<Stmt>>,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_override: bool,
    /// 是否是抽象方法
    pub is_abstract: bool,
    pub span: Span,
}

/// interface 方法签名
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceMethod {
    pub name: String,
    pub params: Vec<FnParam>,
    pub return_type: Option<TypeAnnotation>,
    pub span: Span,
}

/// trait 方法（可以有默认实现）
#[derive(Debug, Clone, PartialEq)]
pub struct TraitMethod {
    pub name: String,
    pub params: Vec<FnParam>,
    pub return_type: Option<TypeAnnotation>,
    /// 默认实现（如果有）
    pub default_body: Option<Box<Stmt>>,
    pub span: Span,
}

/// enum 变体
#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: String,
    pub value: Option<Expr>,  // 关联值
    pub fields: Vec<(String, TypeAnnotation)>,  // 关联数据字段
    pub span: Span,
}

impl Stmt {
    /// 获取语句的位置信息
    pub fn span(&self) -> Span {
        match self {
            Stmt::Expression { span, .. } => *span,
            Stmt::Print { span, .. } => *span,
            Stmt::VarDecl { span, .. } => *span,
            Stmt::ConstDecl { span, .. } => *span,
            Stmt::Block { span, .. } => *span,
            Stmt::If { span, .. } => *span,
            Stmt::ForLoop { span, .. } => *span,
            Stmt::ForIn { span, .. } => *span,
            Stmt::While { span, .. } => *span,
            Stmt::Break { span, .. } => *span,
            Stmt::Continue { span, .. } => *span,
            Stmt::Return { span, .. } => *span,
            Stmt::Match { span, .. } => *span,
            Stmt::StructDef { span, .. } => *span,
            Stmt::ClassDef { span, .. } => *span,
            Stmt::InterfaceDef { span, .. } => *span,
            Stmt::TraitDef { span, .. } => *span,
            Stmt::EnumDef { span, .. } => *span,
            Stmt::TypeAlias { span, .. } => *span,
            Stmt::TryCatch { span, .. } => *span,
            Stmt::Throw { span, .. } => *span,
            Stmt::FnDef { span, .. } => *span,
            Stmt::Package { span, .. } => *span,
            Stmt::Import { span, .. } => *span,
        }
    }
}

/// 程序（AST 根节点）
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// 包声明（可选）
    pub package: Option<String>,
    /// 导入声明列表
    pub imports: Vec<ImportDecl>,
    /// 语句列表
    pub statements: Vec<Stmt>,
}

impl Program {
    /// 创建新的程序
    pub fn new(statements: Vec<Stmt>) -> Self {
        Self { 
            package: None,
            imports: Vec::new(),
            statements,
        }
    }
    
    /// 创建带包声明和导入的程序
    pub fn with_package_and_imports(
        package: Option<String>,
        imports: Vec<ImportDecl>,
        statements: Vec<Stmt>,
    ) -> Self {
        Self { package, imports, statements }
    }
}
