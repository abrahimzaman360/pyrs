#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,
    Bool,
    String,
    Void,
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
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let(String, Type, Option<Expr>),
    Assign(String, Expr),
    If(Expr, Vec<Stmt>, Vec<(Expr, Vec<Stmt>)>, Option<Vec<Stmt>>),
    // If(condition, then_body, elif_branches: Vec<(condition, body)>, else_body)
    While(Expr, Vec<Stmt>),
    For(String, Expr, Expr, Option<Expr>, Vec<Stmt>),
    // For(var_name, start, end, step, body) - for i in range(start, end[, step])
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
}

#[allow(unused)]
#[derive(Debug)]
pub struct Program {
    pub items: Vec<TopLevel>,
}
