use crate::ast::*;
use crate::lexer::{Lexer, Token};
use anyhow::{Result, anyhow};

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    current_token: Option<Token>,
    peek_token: Option<Token>,
}

impl<'a> Parser<'a> {
    pub fn new(lexer: Lexer<'a>) -> Self {
        let mut p = Self {
            lexer,
            current_token: None,
            peek_token: None,
        };
        p.advance();
        p.advance();
        p
    }

    fn advance(&mut self) {
        self.current_token = self.peek_token.take();
        self.peek_token = self.lexer.next();
    }

    fn expect(&mut self, token: Token) -> Result<()> {
        match &self.current_token {
            Some(t) if t == &token => {
                self.advance();
                Ok(())
            }
            Some(t) => Err(anyhow!("Expected {:?}, found {:?}", token, t)),
            None => Err(anyhow!("Expected {:?}, found EOF", token)),
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.current_token.as_ref()
    }

    fn peek_ahead(&self) -> Option<&Token> {
        self.peek_token.as_ref()
    }

    fn skip_newlines(&mut self) {
        while self.peek() == Some(&Token::Newline) {
            self.advance();
        }
    }

    fn expect_semicolon(&mut self) -> Result<()> {
        self.expect(Token::Semicolon)?;
        // Accept Newline, Dedent, or EOF after semicolon
        if self.peek() == Some(&Token::Newline) {
            self.advance();
        }
        Ok(())
    }

    pub fn parse_program(&mut self) -> Result<Program> {
        let mut items = Vec::new();
        self.skip_newlines();
        while self.peek().is_some() {
            items.push(self.parse_top_level()?);
            self.skip_newlines();
        }
        Ok(Program { items })
    }

    fn parse_top_level(&mut self) -> Result<TopLevel> {
        match self.peek() {
            Some(Token::Def) => Ok(TopLevel::Function(self.parse_function()?)),
            Some(Token::Extern) => Ok(TopLevel::Extern(self.parse_extern()?)),
            Some(Token::Import) => Ok(TopLevel::Import(self.parse_import()?)),
            Some(Token::From) => Ok(TopLevel::FromImport(self.parse_from_import()?)),
            Some(Token::Newline) => {
                self.advance();
                self.parse_top_level()
            }
            Some(t) => Err(anyhow!("Unexpected top-level token: {:?}", t)),
            None => Err(anyhow!("Unexpected EOF at top level")),
        }
    }

    fn parse_function(&mut self) -> Result<Function> {
        self.expect(Token::Def)?;
        let name = match self.advance_with_token() {
            Some(Token::Ident(s)) => s,
            _ => return Err(anyhow!("Expected function name")),
        };

        self.expect(Token::LParen)?;
        let mut params = Vec::new();
        if self.peek() != Some(&Token::RParen) {
            loop {
                let p_name = match self.advance_with_token() {
                    Some(Token::Ident(s)) => s,
                    _ => return Err(anyhow!("Expected parameter name")),
                };
                self.expect(Token::Colon)?;
                let p_type = self.parse_type()?;
                params.push((p_name, p_type));

                if self.peek() == Some(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        self.expect(Token::RParen)?;

        let mut return_type = Type::Void;
        if self.peek() == Some(&Token::Arrow) {
            self.advance();
            return_type = self.parse_type()?;
        }

        self.expect(Token::Colon)?;
        self.expect(Token::Newline)?;
        let body = self.parse_block()?;

        Ok(Function {
            name,
            params,
            return_type,
            body,
        })
    }

    fn parse_extern(&mut self) -> Result<ExternDecl> {
        self.expect(Token::Extern)?;
        self.expect(Token::Def)?;
        let name = match self.advance_with_token() {
            Some(Token::Ident(s)) => s,
            _ => return Err(anyhow!("Expected extern function name")),
        };

        self.expect(Token::LParen)?;
        let mut params = Vec::new();
        if self.peek() != Some(&Token::RParen) {
            loop {
                let p_name = match self.advance_with_token() {
                    Some(Token::Ident(s)) => s,
                    _ => return Err(anyhow!("Expected parameter name")),
                };
                self.expect(Token::Colon)?;
                let p_type = self.parse_type()?;
                params.push((p_name, p_type));

                if self.peek() == Some(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        self.expect(Token::RParen)?;

        let mut return_type = Type::Void;
        if self.peek() == Some(&Token::Arrow) {
            self.advance();
            return_type = self.parse_type()?;
        }

        self.expect_semicolon()?;

        Ok(ExternDecl {
            name,
            params,
            return_type,
        })
    }

    fn parse_import(&mut self) -> Result<Import> {
        self.expect(Token::Import)?;
        let path = self.parse_module_path()?;
        let mut alias = None;
        if self.peek() == Some(&Token::As) {
            self.advance();
            alias = Some(match self.advance_with_token() {
                Some(Token::Ident(s)) => s,
                _ => return Err(anyhow!("Expected identifier after 'as'")),
            });
        }
        self.expect_semicolon()?;
        Ok(Import { path, alias })
    }

    fn parse_from_import(&mut self) -> Result<FromImport> {
        self.expect(Token::From)?;
        let module_path = self.parse_module_path()?;
        self.expect(Token::Import)?;
        let mut names = Vec::new();
        loop {
            let name = match self.advance_with_token() {
                Some(Token::Ident(s)) => s,
                _ => return Err(anyhow!("Expected identifier in import list")),
            };
            let mut alias = None;
            if self.peek() == Some(&Token::As) {
                self.advance();
                alias = Some(match self.advance_with_token() {
                    Some(Token::Ident(s)) => s,
                    _ => return Err(anyhow!("Expected identifier after 'as'")),
                });
            }
            names.push((name, alias));
            if self.peek() == Some(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.expect_semicolon()?;
        Ok(FromImport { module_path, names })
    }

    fn parse_module_path(&mut self) -> Result<Vec<String>> {
        let mut path = Vec::new();
        path.push(match self.advance_with_token() {
            Some(Token::Ident(s)) => s,
            _ => return Err(anyhow!("Expected module identifier")),
        });
        while self.peek() == Some(&Token::Dot) {
            self.advance();
            path.push(match self.advance_with_token() {
                Some(Token::Ident(s)) => s,
                _ => return Err(anyhow!("Expected identifier after '.'")),
            });
        }
        Ok(path)
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>> {
        self.expect(Token::Indent)?;
        let mut stmts = Vec::new();
        while self.peek() != Some(&Token::Dedent) && self.peek().is_some() {
            if self.peek() == Some(&Token::Newline) {
                self.advance();
                continue;
            }
            stmts.push(self.parse_stmt()?);
        }
        self.expect(Token::Dedent)?;
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> Result<Stmt> {
        match self.peek() {
            Some(Token::Let) => self.parse_let_stmt(),
            Some(Token::If) => self.parse_if_stmt(),
            Some(Token::While) => self.parse_while_stmt(),
            Some(Token::For) => self.parse_for_stmt(),
            Some(Token::Return) => self.parse_return_stmt(),
            Some(Token::Break) => {
                self.advance();
                self.expect_semicolon()?;
                Ok(Stmt::Break)
            }
            Some(Token::Continue) => {
                self.advance();
                self.expect_semicolon()?;
                Ok(Stmt::Continue)
            }
            Some(Token::Ident(_)) => {
                // Check for assignment or augmented assignment
                match self.peek_ahead() {
                    Some(Token::Assign) => {
                        let name = match self.advance_with_token() {
                            Some(Token::Ident(s)) => s,
                            _ => unreachable!(),
                        };
                        self.expect(Token::Assign)?;
                        let expr = self.parse_expr()?;
                        self.expect_semicolon()?;
                        Ok(Stmt::Assign(name, expr))
                    }
                    Some(Token::PlusAssign)
                    | Some(Token::MinusAssign)
                    | Some(Token::StarAssign)
                    | Some(Token::SlashAssign) => {
                        let name = match self.advance_with_token() {
                            Some(Token::Ident(s)) => s,
                            _ => unreachable!(),
                        };
                        let op = match self.advance_with_token() {
                            Some(Token::PlusAssign) => BinaryOp::Add,
                            Some(Token::MinusAssign) => BinaryOp::Sub,
                            Some(Token::StarAssign) => BinaryOp::Mul,
                            Some(Token::SlashAssign) => BinaryOp::Div,
                            _ => unreachable!(),
                        };
                        let rhs = self.parse_expr()?;
                        self.expect_semicolon()?;
                        // Desugar: x += y => x = x + y
                        Ok(Stmt::Assign(
                            name.clone(),
                            Expr::Binary(Box::new(Expr::Var(name)), op, Box::new(rhs)),
                        ))
                    }
                    _ => {
                        let expr = self.parse_expr()?;
                        self.expect_semicolon()?;
                        Ok(Stmt::Expr(expr))
                    }
                }
            }
            _ => {
                let expr = self.parse_expr()?;
                self.expect_semicolon()?;
                Ok(Stmt::Expr(expr))
            }
        }
    }

    fn parse_let_stmt(&mut self) -> Result<Stmt> {
        self.advance();
        let name = match self.advance_with_token() {
            Some(Token::Ident(s)) => s,
            _ => return Err(anyhow!("Expected variable name")),
        };
        self.expect(Token::Colon)?;
        let ty = self.parse_type()?;
        let mut value = None;
        if self.peek() == Some(&Token::Assign) {
            self.advance();
            value = Some(self.parse_expr()?);
        }
        self.expect_semicolon()?;
        Ok(Stmt::Let(name, ty, value))
    }

    fn parse_if_stmt(&mut self) -> Result<Stmt> {
        self.advance(); // consume 'if'
        let cond = self.parse_expr()?;
        self.expect(Token::Colon)?;
        self.expect(Token::Newline)?;
        let then_branch = self.parse_block()?;

        // Parse elif chains
        let mut elif_branches = Vec::new();
        while self.peek() == Some(&Token::Elif) {
            self.advance(); // consume 'elif'
            let elif_cond = self.parse_expr()?;
            self.expect(Token::Colon)?;
            self.expect(Token::Newline)?;
            let elif_body = self.parse_block()?;
            elif_branches.push((elif_cond, elif_body));
        }

        let mut else_branch = None;
        if self.peek() == Some(&Token::Else) {
            self.advance();
            self.expect(Token::Colon)?;
            self.expect(Token::Newline)?;
            else_branch = Some(self.parse_block()?);
        }
        Ok(Stmt::If(cond, then_branch, elif_branches, else_branch))
    }

    fn parse_while_stmt(&mut self) -> Result<Stmt> {
        self.advance();
        let cond = self.parse_expr()?;
        self.expect(Token::Colon)?;
        self.expect(Token::Newline)?;
        let body = self.parse_block()?;
        Ok(Stmt::While(cond, body))
    }

    fn parse_for_stmt(&mut self) -> Result<Stmt> {
        self.advance(); // consume 'for'
        let var_name = match self.advance_with_token() {
            Some(Token::Ident(s)) => s,
            _ => return Err(anyhow!("Expected loop variable name after 'for'")),
        };
        self.expect(Token::In)?;
        self.expect(Token::Range)?;
        self.expect(Token::LParen)?;

        let start = self.parse_expr()?;
        self.expect(Token::Comma)?;
        let end = self.parse_expr()?;

        let step = if self.peek() == Some(&Token::Comma) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };

        self.expect(Token::RParen)?;
        self.expect(Token::Colon)?;
        self.expect(Token::Newline)?;
        let body = self.parse_block()?;

        Ok(Stmt::For(var_name, start, end, step, body))
    }

    fn parse_return_stmt(&mut self) -> Result<Stmt> {
        self.advance();
        let mut value = None;
        if self.peek() != Some(&Token::Semicolon) {
            value = Some(self.parse_expr()?);
        }
        self.expect_semicolon()?;
        Ok(Stmt::Return(value))
    }

    fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_logical_or()
    }

    fn parse_logical_or(&mut self) -> Result<Expr> {
        let mut expr = self.parse_logical_and()?;
        while let Some(Token::Or) = self.peek() {
            self.advance();
            let right = self.parse_logical_and()?;
            expr = Expr::Binary(Box::new(expr), BinaryOp::Or, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_logical_and(&mut self) -> Result<Expr> {
        let mut expr = self.parse_equality()?;
        while let Some(Token::And) = self.peek() {
            self.advance();
            let right = self.parse_equality()?;
            expr = Expr::Binary(Box::new(expr), BinaryOp::And, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_equality(&mut self) -> Result<Expr> {
        let mut expr = self.parse_comparison()?;
        while let Some(Token::Eq) | Some(Token::Ne) = self.peek() {
            let op = match self.advance_with_token().unwrap() {
                Token::Eq => BinaryOp::Eq,
                Token::Ne => BinaryOp::Ne,
                _ => unreachable!(),
            };
            let right = self.parse_comparison()?;
            expr = Expr::Binary(Box::new(expr), op, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_comparison(&mut self) -> Result<Expr> {
        let mut expr = self.parse_term()?;
        while let Some(Token::Lt) | Some(Token::Gt) | Some(Token::Le) | Some(Token::Ge) =
            self.peek()
        {
            let op = match self.advance_with_token().unwrap() {
                Token::Lt => BinaryOp::Lt,
                Token::Gt => BinaryOp::Gt,
                Token::Le => BinaryOp::Le,
                Token::Ge => BinaryOp::Ge,
                _ => unreachable!(),
            };
            let right = self.parse_term()?;
            expr = Expr::Binary(Box::new(expr), op, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_term(&mut self) -> Result<Expr> {
        let mut expr = self.parse_factor()?;
        while let Some(Token::Plus) | Some(Token::Minus) = self.peek() {
            let op = match self.advance_with_token().unwrap() {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => unreachable!(),
            };
            let right = self.parse_factor()?;
            expr = Expr::Binary(Box::new(expr), op, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_factor(&mut self) -> Result<Expr> {
        let mut expr = self.parse_unary()?;
        while let Some(Token::Star) | Some(Token::Slash) | Some(Token::Percent) = self.peek() {
            let op = match self.advance_with_token().unwrap() {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                Token::Percent => BinaryOp::Mod,
                _ => unreachable!(),
            };
            let right = self.parse_unary()?;
            expr = Expr::Binary(Box::new(expr), op, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        match self.peek() {
            Some(Token::Not) => {
                self.advance();
                let right = self.parse_unary()?;
                Ok(Expr::Unary(UnaryOp::Not, Box::new(right)))
            }
            Some(Token::Minus) => {
                self.advance();
                let right = self.parse_unary()?;
                Ok(Expr::Unary(UnaryOp::Neg, Box::new(right)))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        match self.advance_with_token() {
            Some(Token::Int(i)) => Ok(Expr::Int(i)),
            Some(Token::Float(f)) => Ok(Expr::Float(f)),
            Some(Token::Bool(b)) => Ok(Expr::Bool(b)),
            Some(Token::StringLit(s)) => Ok(Expr::String(s)),
            Some(Token::Ident(s)) => {
                if self.peek() == Some(&Token::LParen) {
                    self.advance();
                    let mut args = Vec::new();
                    if self.peek() != Some(&Token::RParen) {
                        loop {
                            args.push(self.parse_expr()?);
                            if self.peek() == Some(&Token::Comma) {
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    }
                    self.expect(Token::RParen)?;
                    Ok(Expr::Call(s, args))
                } else {
                    Ok(Expr::Var(s))
                }
            }
            Some(Token::LParen) => {
                let expr = self.parse_expr()?;
                self.expect(Token::RParen)?;
                Ok(expr)
            }
            Some(t) => Err(anyhow!("Unexpected token in expression: {:?}", t)),
            None => Err(anyhow!("Unexpected EOF in expression")),
        }
    }

    fn parse_type(&mut self) -> Result<Type> {
        match self.advance_with_token() {
            Some(Token::Ident(s)) => match s.as_str() {
                "i64" | "int" => Ok(Type::Int),
                "f64" | "float" => Ok(Type::Float),
                "bool" => Ok(Type::Bool),
                "str" | "string" => Ok(Type::String),
                "void" => Ok(Type::Void),
                other => Err(anyhow!(
                    "Unknown type '{}'. Supported types: int, float, bool, str, void",
                    other
                )),
            },
            _ => Err(anyhow!("Expected type name")),
        }
    }

    fn advance_with_token(&mut self) -> Option<Token> {
        let t = self.current_token.clone();
        self.advance();
        t
    }
}
