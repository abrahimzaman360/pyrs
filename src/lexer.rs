use logos::{Lexer as LogosLexer, Logos};
use std::collections::VecDeque;

#[derive(Logos, Debug, PartialEq, Clone)]
#[logos(skip r"[ \t\r\f]+")] // Skip inline whitespace except newlines
#[logos(skip(r"//[^\n]*", allow_greedy = true))] // Skip comments
pub enum RawToken {
    #[token("def")]
    Def,
    #[token("let")]
    Let,
    #[token("extern")]
    Extern,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("while")]
    While,
    #[token("elif")]
    Elif,
    #[token("for")]
    For,
    #[token("in")]
    In,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,
    #[token("range")]
    Range,
    #[token("return")]
    Return,
    #[token("and")]
    And,
    #[token("or")]
    Or,
    #[token("not")]
    Not,
    #[token("->")]
    Arrow,
    #[token("import")]
    Import,
    #[token("from")]
    From,
    #[token("as")]
    As,
    #[token(".")]
    Dot,

    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),
    #[regex(r"[0-9]+", |lex| lex.slice().parse::<i64>().ok())]
    Int(i64),
    #[regex(r"[0-9]*\.[0-9]+", |lex| lex.slice().parse::<f64>().ok())]
    Float(f64),
    #[regex(r#""([^"\\]|\\.)*""#, |lex| lex.slice().to_string())]
    StringLit(String),

    #[token("true", |_| true)]
    #[token("false", |_| false)]
    Bool(bool),

    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token(":")]
    Colon,
    #[token(",")]
    Comma,
    #[token(";")]
    Semicolon,

    #[token("+=")]
    PlusAssign,
    #[token("-=")]
    MinusAssign,
    #[token("*=")]
    StarAssign,
    #[token("/=")]
    SlashAssign,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("==")]
    Eq,
    #[token("=")]
    Assign,
    #[token("!=")]
    Ne,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token("<=")]
    Le,
    #[token(">=")]
    Ge,

    #[regex(r"\r?\n")]
    Newline,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    Def,
    Let,
    Extern,
    If,
    Else,
    While,
    Elif,
    For,
    In,
    Break,
    Continue,
    Range,
    Return,
    And,
    Or,
    Not,
    Arrow,
    Import,
    From,
    As,
    Dot,
    Ident(String),
    Int(i64),
    Float(f64),
    StringLit(String),
    Bool(bool),
    LParen,
    RParen,
    Colon,
    Comma,
    Semicolon,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,
    Assign,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Indent,
    Dedent,
    Newline,
    Error,
}

impl From<RawToken> for Token {
    fn from(rt: RawToken) -> Self {
        match rt {
            RawToken::Def => Token::Def,
            RawToken::Let => Token::Let,
            RawToken::Extern => Token::Extern,
            RawToken::If => Token::If,
            RawToken::Else => Token::Else,
            RawToken::While => Token::While,
            RawToken::Elif => Token::Elif,
            RawToken::For => Token::For,
            RawToken::In => Token::In,
            RawToken::Break => Token::Break,
            RawToken::Continue => Token::Continue,
            RawToken::Range => Token::Range,
            RawToken::Return => Token::Return,
            RawToken::And => Token::And,
            RawToken::Or => Token::Or,
            RawToken::Not => Token::Not,
            RawToken::Arrow => Token::Arrow,
            RawToken::Import => Token::Import,
            RawToken::From => Token::From,
            RawToken::As => Token::As,
            RawToken::Dot => Token::Dot,
            RawToken::Ident(s) => Token::Ident(s),
            RawToken::Int(i) => Token::Int(i),
            RawToken::Float(f) => Token::Float(f),
            RawToken::StringLit(s) => Token::StringLit(s),
            RawToken::Bool(b) => Token::Bool(b),
            RawToken::LParen => Token::LParen,
            RawToken::RParen => Token::RParen,
            RawToken::Colon => Token::Colon,
            RawToken::Comma => Token::Comma,
            RawToken::Semicolon => Token::Semicolon,
            RawToken::PlusAssign => Token::PlusAssign,
            RawToken::MinusAssign => Token::MinusAssign,
            RawToken::StarAssign => Token::StarAssign,
            RawToken::SlashAssign => Token::SlashAssign,
            RawToken::Plus => Token::Plus,
            RawToken::Minus => Token::Minus,
            RawToken::Star => Token::Star,
            RawToken::Slash => Token::Slash,
            RawToken::Percent => Token::Percent,
            RawToken::Eq => Token::Eq,
            RawToken::Assign => Token::Assign,
            RawToken::Ne => Token::Ne,
            RawToken::Lt => Token::Lt,
            RawToken::Gt => Token::Gt,
            RawToken::Le => Token::Le,
            RawToken::Ge => Token::Ge,
            RawToken::Newline => Token::Newline,
        }
    }
}

#[allow(unused)]
pub struct Lexer<'a> {
    inner: LogosLexer<'a, RawToken>,
    indent_stack: Vec<usize>,
    pending_tokens: VecDeque<Token>,
    at_line_start: bool,
    eof_sent: bool,
    input: &'a str,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            inner: RawToken::lexer(input),
            indent_stack: vec![0],
            pending_tokens: VecDeque::new(),
            at_line_start: true,
            eof_sent: false,
            input,
        }
    }

    fn check_indentation(&mut self) -> bool {
        let remainder = self.inner.remainder();
        let mut indentation = 0;
        let mut bytes_to_skip = 0;
        let mut chars = remainder.chars();

        while let Some(c) = chars.next() {
            if c == ' ' {
                indentation += 1;
                bytes_to_skip += 1;
            } else if c == '\t' {
                indentation = (indentation / 4 + 1) * 4;
                bytes_to_skip += 1;
            } else if c == '\n' || c == '\r' {
                // Blank line, skip and stay at line start
                self.inner.bump(bytes_to_skip + 1);
                return true;
            } else {
                break;
            }
        }

        // Check for comment-only line
        let remaining = &remainder[bytes_to_skip..];
        if remaining.starts_with("//") {
            let line_end = remaining.find('\n').map(|p| p + 1).unwrap_or(remaining.len());
            self.inner.bump(bytes_to_skip + line_end);
            return true;
        }

        self.inner.bump(bytes_to_skip);

        let current_indent = *self.indent_stack.last().unwrap();
        if indentation > current_indent {
            self.indent_stack.push(indentation);
            self.pending_tokens.push_back(Token::Indent);
        } else if indentation < current_indent {
            while indentation < *self.indent_stack.last().unwrap() {
                self.indent_stack.pop();
                self.pending_tokens.push_back(Token::Dedent);
            }
            if indentation != *self.indent_stack.last().unwrap() {
                self.pending_tokens.push_back(Token::Error);
            }
        }
        false
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(t) = self.pending_tokens.pop_front() {
            return Some(t);
        }

        if self.eof_sent {
            return None;
        }

        loop {
            if self.at_line_start {
                if self.check_indentation() {
                    continue; // Skip blank line
                }
                self.at_line_start = false;
                if let Some(t) = self.pending_tokens.pop_front() {
                    return Some(t);
                }
            }

            match self.inner.next() {
                Some(Ok(RawToken::Newline)) => {
                    self.at_line_start = true;
                    return Some(Token::Newline);
                }
                Some(Ok(rt)) => return Some(Token::from(rt)),
                Some(Err(_)) => return Some(Token::Error),
                None => {
                    if !self.eof_sent {
                        while self.indent_stack.len() > 1 {
                            self.indent_stack.pop();
                            self.pending_tokens.push_back(Token::Dedent);
                        }
                        self.eof_sent = true;
                        if let Some(t) = self.pending_tokens.pop_front() {
                            return Some(t);
                        }
                    }
                    return None;
                }
            }
        }
    }
}
