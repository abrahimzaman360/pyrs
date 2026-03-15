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
            Some(Token::Struct) => Ok(TopLevel::Struct(self.parse_struct()?)),
            Some(Token::Impl) => Ok(TopLevel::Impl(self.parse_impl()?)),
            Some(Token::Trait) => Ok(TopLevel::Trait(self.parse_trait()?)),
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
                    Some(Token::SelfLower) => "self".to_string(),
                    _ => return Err(anyhow!("Expected parameter name")),
                };
                if p_name != "self" {
                    self.expect(Token::Colon)?;
                    let p_type = self.parse_type()?;
                    params.push((p_name, p_type));
                } else {
                    params.push(("self".to_string(), Type::Void));
                }

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
        if self.peek() == Some(&Token::Def) {
            self.advance();
        }
        let name = match self.advance_with_token() {
            Some(Token::Ident(s)) => s,
            _ => return Err(anyhow!("Expected extern function name")),
        };

        self.expect(Token::LParen)?;
        let mut params = Vec::new();
        let mut is_variadic = false;
        if self.peek() != Some(&Token::RParen) {
            loop {
                if self.peek() == Some(&Token::Ellipsis) {
                    self.advance();
                    is_variadic = true;
                    if self.peek() == Some(&Token::Comma) {
                        return Err(anyhow!("Ellipsis must be the last parameter"));
                    }
                    break;
                }
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
            is_variadic,
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

    fn parse_struct(&mut self) -> Result<Struct> {
        self.expect(Token::Struct)?;
        let name = match self.advance_with_token() {
            Some(Token::Ident(s)) => s,
            _ => return Err(anyhow!("Expected struct name")),
        };
        self.expect(Token::Colon)?;
        self.expect(Token::Newline)?;
        self.expect(Token::Indent)?;
        let mut fields = Vec::new();
        while self.peek() != Some(&Token::Dedent) && self.peek().is_some() {
            if self.peek() == Some(&Token::Newline) {
                self.advance();
                continue;
            }
            let f_name = match self.advance_with_token() {
                Some(Token::Ident(s)) => s,
                _ => return Err(anyhow!("Expected field name")),
            };
            self.expect(Token::Colon)?;
            let f_type = self.parse_type()?;
            if self.peek() == Some(&Token::Comma) {
                self.advance();
            }
            if self.peek() == Some(&Token::Newline) {
                self.advance();
            }
            fields.push((f_name, f_type));
        }
        self.expect(Token::Dedent)?;
        Ok(Struct { name, fields })
    }

    fn parse_impl(&mut self) -> Result<Impl> {
        self.expect(Token::Impl)?;
        let name = match self.advance_with_token() {
            Some(Token::Ident(s)) => s,
            _ => return Err(anyhow!("Expected trait name or struct name in 'impl'")),
        };

        let mut trait_name = None;
        let mut target = name.clone();

        if self.peek() == Some(&Token::For) {
            self.advance();
            trait_name = Some(name);
            target = match self.advance_with_token() {
                Some(Token::Ident(s)) => s,
                _ => return Err(anyhow!("Expected target struct name after 'for'")),
            };
        }

        self.expect(Token::Colon)?;
        self.expect(Token::Newline)?;
        self.expect(Token::Indent)?;
        let mut methods = Vec::new();
        while self.peek() != Some(&Token::Dedent) && self.peek().is_some() {
            if self.peek() == Some(&Token::Newline) {
                self.advance();
                continue;
            }
            methods.push(self.parse_function()?);
            self.skip_newlines();
        }
        self.expect(Token::Dedent)?;
        Ok(Impl {
            target,
            trait_name,
            methods,
        })
    }

    fn parse_trait(&mut self) -> Result<Trait> {
        self.expect(Token::Trait)?;
        let name = match self.advance_with_token() {
            Some(Token::Ident(s)) => s,
            _ => return Err(anyhow!("Expected trait name")),
        };
        self.expect(Token::Colon)?;
        self.expect(Token::Newline)?;
        self.expect(Token::Indent)?;
        let mut methods = Vec::new();
        while self.peek() != Some(&Token::Dedent) && self.peek().is_some() {
            if self.peek() == Some(&Token::Newline) {
                self.advance();
                continue;
            }
            methods.push(self.parse_trait_method()?);
            self.skip_newlines();
        }
        self.expect(Token::Dedent)?;
        Ok(Trait { name, methods })
    }

    fn parse_trait_method(&mut self) -> Result<Function> {
        self.expect(Token::Def)?;
        let name = match self.advance_with_token() {
            Some(Token::Ident(s)) => s,
            _ => return Err(anyhow!("Expected method name")),
        };

        self.expect(Token::LParen)?;
        let mut params = Vec::new();
        if self.peek() != Some(&Token::RParen) {
            loop {
                let p_name = match self.advance_with_token() {
                    Some(Token::Ident(s)) => s,
                    Some(Token::SelfLower) => "self".to_string(),
                    _ => return Err(anyhow!("Expected parameter name")),
                };
                if p_name != "self" {
                    self.expect(Token::Colon)?;
                    let p_type = self.parse_type()?;
                    params.push((p_name, p_type));
                } else {
                    params.push(("self".to_string(), Type::Void));
                }

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

        let body = if self.peek() == Some(&Token::Semicolon) {
            self.advance();
            Vec::new()
        } else {
            self.expect(Token::Colon)?;
            self.expect(Token::Newline)?;
            self.parse_block()?
        };

        Ok(Function {
            name,
            params,
            return_type,
            body,
        })
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
            _ => {
                // Peek ahead for simple variable assignment: identifier followed by '=' or '+=' etc.
                if let Some(Token::Ident(name)) = self.peek() {
                    let name = name.clone();
                    match self.peek_ahead() {
                        Some(Token::Assign) => {
                            self.advance(); // consume ident
                            self.advance(); // consume '='
                            let expr = self.parse_expr()?;
                            self.expect_semicolon()?;
                            return Ok(Stmt::Assign(name, expr));
                        }
                        Some(Token::PlusAssign)
                        | Some(Token::MinusAssign)
                        | Some(Token::StarAssign)
                        | Some(Token::SlashAssign) => {
                            self.advance(); // consume ident
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
                            return Ok(Stmt::Assign(
                                name.clone(),
                                Expr::Binary(Box::new(Expr::Var(name)), op, Box::new(rhs)),
                            ));
                        }
                        _ => {} // Fall through to general expression/complex assignment
                    }
                }

                let expr = self.parse_expr()?;
                if self.peek() == Some(&Token::Assign) {
                    self.advance();
                    let value = self.parse_expr()?;
                    self.expect_semicolon()?;
                    if let Expr::Index(target, index) = expr {
                        return Ok(Stmt::IndexAssign(target, index, Box::new(value)));
                    } else if let Expr::MemberAccess(obj, field) = expr {
                        return Ok(Stmt::MemberAssign(obj, field, value));
                    } else if let Expr::Var(name) = expr {
                        // This handles cases where Expr::Var was returned but it was actually an assignment
                        // (though the simple case above should catch most)
                        return Ok(Stmt::Assign(name, value));
                    } else {
                        return Err(anyhow!("Invalid assignment target. Expected variable, index, or member access."));
                    }
                }
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
                let expr = self.parse_unary()?;
                Ok(Expr::Unary(UnaryOp::Not, Box::new(expr)))
            }
            Some(Token::Minus) => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary(UnaryOp::Neg, Box::new(expr)))
            }
            Some(Token::Ampersand) => {
                self.advance();
                let mut is_mut = false;
                if self.peek() == Some(&Token::Mut) {
                    self.advance();
                    is_mut = true;
                }
                let expr = self.parse_unary()?;
                Ok(Expr::Borrow(Box::new(expr), is_mut))
            }
            Some(Token::Star) => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Deref(Box::new(expr)))
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek() {
                Some(Token::Dot) => {
                    self.advance();
                    let member = match self.advance_with_token() {
                        Some(Token::Ident(s)) => s,
                        _ => return Err(anyhow!("Expected member name after '.'")),
                    };
                    expr = Expr::MemberAccess(Box::new(expr), member);
                }
                Some(Token::LParen) => {
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
                    match expr {
                        Expr::Var(name) => {
                            expr = Expr::Call(name, args);
                        }
                        Expr::MemberAccess(obj, method) => {
                            expr = Expr::MethodCall(obj, method, args);
                        }
                        _ => {
                            return Err(anyhow!(
                                "Calling non-identifier expressions is not yet supported"
                            ));
                        }
                    }
                }
                Some(Token::LBracket) => {
                    self.advance();
                    let index = self.parse_expr()?;
                    self.expect(Token::RBracket)?;
                    expr = Expr::Index(Box::new(expr), Box::new(index));
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        match self.advance_with_token() {
            Some(Token::Int(i)) => Ok(Expr::Int(i)),
            Some(Token::Float(f)) => Ok(Expr::Float(f)),
            Some(Token::Bool(b)) => Ok(Expr::Bool(b)),
            Some(Token::StringLit(s)) => Ok(Expr::String(s)),
            Some(Token::Ident(s)) => Ok(Expr::Var(s)),
            Some(Token::SelfLower) => Ok(Expr::Var("self".to_string())),
            Some(Token::LParen) => {
                let expr = self.parse_expr()?;
                self.expect(Token::RParen)?;
                Ok(expr)
            }
            Some(Token::LBracket) => {
                let mut items = Vec::new();
                if self.peek() != Some(&Token::RBracket) {
                    loop {
                        items.push(self.parse_expr()?);
                        if self.peek() == Some(&Token::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(Token::RBracket)?;
                Ok(Expr::List(items))
            }
            Some(Token::Ampersand) => {
                // For now handle &var as Index or just a placeholder?
                // Actually, & is usually a unary operator. Let's redirect to parse_expr?
                // No, & is handled in unary. But parse_primary should handle it if it starts there.
                // Wait, &var is not primary. Primary is literals, vars, groups.
                // & is a prefix operator.
                Err(anyhow!(
                    "References (&) should be handled in unary/prefix parsing"
                ))
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
                "list" => {
                    self.expect(Token::LBracket)?;
                    let inner = self.parse_type()?;
                    self.expect(Token::RBracket)?;
                    Ok(Type::List(Box::new(inner)))
                }
                other => Ok(Type::Custom(other.to_string())),
            },
            Some(Token::Ampersand) => {
                let mut is_mut = false;
                if self.peek() == Some(&Token::Mut) {
                    self.advance();
                    is_mut = true;
                }
                let inner = self.parse_type()?;
                if is_mut {
                    Ok(Type::MutRef(Box::new(inner)))
                } else {
                    Ok(Type::Ref(Box::new(inner)))
                }
            }
            Some(Token::SelfLower) => Ok(Type::Custom("self".to_string())),
            _ => Err(anyhow!("Expected type name")),
        }
    }

    fn advance_with_token(&mut self) -> Option<Token> {
        let t = self.current_token.clone();
        self.advance();
        t
    }
}
