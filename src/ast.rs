#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,
    Bool,
    String,
    Void,
    Custom(String),
    List(Box<Type>),
    Ref(Box<Type>),
    MutRef(Box<Type>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Var(String),
    Binary(Box<Expr>, BinaryOp, Box<Expr>),
    Unary(UnaryOp, Box<Expr>),
    Call(String, Vec<Expr>),
    MemberAccess(Box<Expr>, String),
    MethodCall(Box<Expr>, String, Vec<Expr>),
    List(Vec<Expr>),
    Index(Box<Expr>, Box<Expr>),
    Borrow(Box<Expr>, bool), // (expr, is_mut)
    Deref(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let(String, Type, Option<Expr>),
    Assign(String, Expr),
    IndexAssign(Box<Expr>, Box<Expr>, Box<Expr>), // target, index, value
    MemberAssign(Box<Expr>, String, Expr), // (object_expr, field_name, value)
    If(Expr, Vec<Stmt>, Vec<(Expr, Vec<Stmt>)>, Option<Vec<Stmt>>),
    While(Expr, Vec<Stmt>),
    For(String, Expr, Expr, Option<Expr>, Vec<Stmt>),
    Break,
    Continue,
    Return(Option<Expr>),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TopLevel {
    Function(Function),
    Extern(ExternDecl),
    Import(Import),
    FromImport(FromImport),
    Struct(Struct),
    Impl(Impl),
    Trait(Trait),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Struct {
    pub name: String,
    pub fields: Vec<(String, Type)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Impl {
    pub target: String,
    pub trait_name: Option<String>,
    pub methods: Vec<Function>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Trait {
    pub name: String,
    pub methods: Vec<Function>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    pub path: Vec<String>,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FromImport {
    pub module_path: Vec<String>,
    pub names: Vec<(String, Option<String>)>, // name, alias
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternDecl {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub is_variadic: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub items: Vec<TopLevel>,
}
