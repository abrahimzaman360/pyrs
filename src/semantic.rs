use crate::ast::*;
use anyhow::{Result, anyhow};
use std::collections::HashMap;

pub struct SymbolTable {
    scopes: Vec<HashMap<String, (Type, bool)>>, // (type, initialized)
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn insert(&mut self, name: String, ty: Type, initialized: bool) -> Result<()> {
        if let Some(scope) = self.scopes.last_mut() {
            if scope.contains_key(&name) {
                return Err(anyhow!("Variable '{}' already defined in this scope", name));
            }
            scope.insert(name, (ty, initialized));
            Ok(())
        } else {
            Err(anyhow!("No active scope"))
        }
    }

    pub fn mark_initialized(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(entry) = scope.get_mut(name) {
                entry.1 = true;
                return;
            }
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&(Type, bool)> {
        for scope in self.scopes.iter().rev() {
            if let Some(entry) = scope.get(name) {
                return Some(entry);
            }
        }
        None
    }
}

pub struct Analyzer {
    symbols: SymbolTable,
    functions: HashMap<String, (Vec<Type>, Type)>,
    in_loop: bool,
}

impl Analyzer {
    pub fn new() -> Self {
        Self {
            symbols: SymbolTable::new(),
            functions: HashMap::new(),
            in_loop: false,
        }
    }

    pub fn analyze_program(&mut self, program: &Program) -> Result<()> {
        // First pass: collect function signatures
        for item in &program.items {
            match item {
                TopLevel::Function(f) => {
                    let param_types = f.params.iter().map(|(_, t)| t.clone()).collect();
                    if self.functions.contains_key(&f.name) {
                        return Err(anyhow!("Function '{}' already defined", f.name));
                    }
                    self.functions
                        .insert(f.name.clone(), (param_types, f.return_type.clone()));
                }
                TopLevel::Extern(e) => {
                    let param_types = e.params.iter().map(|(_, t)| t.clone()).collect();
                    if self.functions.contains_key(&e.name) {
                        return Err(anyhow!("Function '{}' already defined", e.name));
                    }
                    self.functions
                        .insert(e.name.clone(), (param_types, e.return_type.clone()));
                }
                TopLevel::Import(_) | TopLevel::FromImport(_) => {
                    // Imports are not yet implemented; silently ignore for now
                }
            }
        }

        // Second pass: analyze function bodies
        for item in &program.items {
            if let TopLevel::Function(f) = item {
                self.analyze_function(f)?;
            }
        }

        Ok(())
    }

    fn analyze_function(&mut self, f: &Function) -> Result<()> {
        self.symbols.push_scope();
        for (name, ty) in &f.params {
            self.symbols.insert(name.clone(), ty.clone(), true)?;
        }

        for stmt in &f.body {
            self.analyze_stmt(stmt, &f.return_type)?;
        }

        // Verify all paths return for non-void functions
        if f.return_type != Type::Void && !self.block_always_returns(&f.body) {
            return Err(anyhow!(
                "Function '{}' does not return a value on all paths",
                f.name
            ));
        }

        self.symbols.pop_scope();
        Ok(())
    }

    fn block_always_returns(&self, stmts: &[Stmt]) -> bool {
        if let Some(last) = stmts.last() {
            match last {
                Stmt::Return(_) => true,
                Stmt::If(_, then_body, elif_branches, else_body) => {
                    let then_returns = self.block_always_returns(then_body);
                    let elifs_return = elif_branches
                        .iter()
                        .all(|(_, body)| self.block_always_returns(body));
                    let else_returns = else_body
                        .as_ref()
                        .map(|b| self.block_always_returns(b))
                        .unwrap_or(false);
                    then_returns && elifs_return && else_returns
                }
                _ => false,
            }
        } else {
            false
        }
    }

    fn analyze_stmt(&mut self, stmt: &Stmt, ret_ty: &Type) -> Result<()> {
        match stmt {
            Stmt::Let(name, ty, value) => {
                let initialized = value.is_some();
                if let Some(expr) = value {
                    let val_ty = self.analyze_expr(expr)?;
                    if val_ty != *ty {
                        return Err(anyhow!(
                            "Type mismatch in 'let' binding for '{}': expected {:?}, found {:?}",
                            name,
                            ty,
                            val_ty
                        ));
                    }
                }
                self.symbols.insert(name.clone(), ty.clone(), initialized)?;
            }
            Stmt::Assign(name, expr) => {
                let entry = self
                    .symbols
                    .lookup(name)
                    .ok_or_else(|| anyhow!("Undefined variable: '{}'", name))?;
                let var_ty = entry.0.clone();
                let val_ty = self.analyze_expr(expr)?;
                if var_ty != val_ty {
                    return Err(anyhow!(
                        "Type mismatch in assignment to '{}': expected {:?}, found {:?}",
                        name,
                        var_ty,
                        val_ty
                    ));
                }
                self.symbols.mark_initialized(name);
            }
            Stmt::If(cond, then_body, elif_branches, else_body) => {
                let cond_ty = self.analyze_expr(cond)?;
                if cond_ty != Type::Bool {
                    return Err(anyhow!(
                        "Condition of 'if' must be bool, found {:?}",
                        cond_ty
                    ));
                }
                self.analyze_block(then_body, ret_ty)?;
                for (elif_cond, elif_body) in elif_branches {
                    let elif_ty = self.analyze_expr(elif_cond)?;
                    if elif_ty != Type::Bool {
                        return Err(anyhow!(
                            "Condition of 'elif' must be bool, found {:?}",
                            elif_ty
                        ));
                    }
                    self.analyze_block(elif_body, ret_ty)?;
                }
                if let Some(else_b) = else_body {
                    self.analyze_block(else_b, ret_ty)?;
                }
            }
            Stmt::While(cond, body) => {
                let cond_ty = self.analyze_expr(cond)?;
                if cond_ty != Type::Bool {
                    return Err(anyhow!(
                        "Condition of 'while' must be bool, found {:?}",
                        cond_ty
                    ));
                }
                let prev_in_loop = self.in_loop;
                self.in_loop = true;
                self.analyze_block(body, ret_ty)?;
                self.in_loop = prev_in_loop;
            }
            Stmt::For(var_name, start, end, step, body) => {
                let start_ty = self.analyze_expr(start)?;
                let end_ty = self.analyze_expr(end)?;
                if start_ty != Type::Int {
                    return Err(anyhow!(
                        "For loop start must be int, found {:?}",
                        start_ty
                    ));
                }
                if end_ty != Type::Int {
                    return Err(anyhow!("For loop end must be int, found {:?}", end_ty));
                }
                if let Some(s) = step {
                    let step_ty = self.analyze_expr(s)?;
                    if step_ty != Type::Int {
                        return Err(anyhow!(
                            "For loop step must be int, found {:?}",
                            step_ty
                        ));
                    }
                }
                self.symbols.push_scope();
                self.symbols.insert(var_name.clone(), Type::Int, true)?;
                let prev_in_loop = self.in_loop;
                self.in_loop = true;
                for stmt in body {
                    self.analyze_stmt(stmt, ret_ty)?;
                }
                self.in_loop = prev_in_loop;
                self.symbols.pop_scope();
            }
            Stmt::Break => {
                if !self.in_loop {
                    return Err(anyhow!("'break' outside of loop"));
                }
            }
            Stmt::Continue => {
                if !self.in_loop {
                    return Err(anyhow!("'continue' outside of loop"));
                }
            }
            Stmt::Return(value) => {
                let actual_ty = if let Some(expr) = value {
                    self.analyze_expr(expr)?
                } else {
                    Type::Void
                };
                if actual_ty != *ret_ty {
                    return Err(anyhow!(
                        "Return type mismatch: expected {:?}, found {:?}",
                        ret_ty,
                        actual_ty
                    ));
                }
            }
            Stmt::Expr(expr) => {
                self.analyze_expr(expr)?;
            }
        }
        Ok(())
    }

    fn analyze_block(&mut self, body: &[Stmt], ret_ty: &Type) -> Result<()> {
        self.symbols.push_scope();
        for stmt in body {
            self.analyze_stmt(stmt, ret_ty)?;
        }
        self.symbols.pop_scope();
        Ok(())
    }

    fn analyze_expr(&self, expr: &Expr) -> Result<Type> {
        match expr {
            Expr::Int(_) => Ok(Type::Int),
            Expr::Float(_) => Ok(Type::Float),
            Expr::Bool(_) => Ok(Type::Bool),
            Expr::String(_) => Ok(Type::String),
            Expr::Var(name) => {
                let entry = self
                    .symbols
                    .lookup(name)
                    .ok_or_else(|| anyhow!("Undefined variable: '{}'", name))?;
                if !entry.1 {
                    return Err(anyhow!(
                        "Variable '{}' used before initialization",
                        name
                    ));
                }
                Ok(entry.0.clone())
            }
            Expr::Binary(lhs, op, rhs) => {
                let lt = self.analyze_expr(lhs)?;
                let rt = self.analyze_expr(rhs)?;
                if lt != rt {
                    return Err(anyhow!(
                        "Binary operation {:?} on mismatched types {:?} and {:?}",
                        op,
                        lt,
                        rt
                    ));
                }
                match op {
                    BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => {
                        if lt != Type::Int && lt != Type::Float {
                            return Err(anyhow!(
                                "Arithmetic operator {:?} requires numeric operands, found {:?}",
                                op,
                                lt
                            ));
                        }
                        Ok(lt)
                    }
                    BinaryOp::Mod => {
                        if lt != Type::Int {
                            return Err(anyhow!(
                                "Modulo operator requires int operands, found {:?}",
                                lt
                            ));
                        }
                        Ok(Type::Int)
                    }
                    BinaryOp::Eq | BinaryOp::Ne => Ok(Type::Bool),
                    BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => {
                        if lt != Type::Int && lt != Type::Float {
                            return Err(anyhow!(
                                "Comparison operator {:?} requires numeric operands, found {:?}",
                                op,
                                lt
                            ));
                        }
                        Ok(Type::Bool)
                    }
                    BinaryOp::And | BinaryOp::Or => {
                        if lt != Type::Bool {
                            return Err(anyhow!(
                                "Logical operator {:?} requires bool operands, found {:?}",
                                op,
                                lt
                            ));
                        }
                        Ok(Type::Bool)
                    }
                }
            }
            Expr::Unary(op, expr) => {
                let ty = self.analyze_expr(expr)?;
                match op {
                    UnaryOp::Not => {
                        if ty != Type::Bool {
                            return Err(anyhow!(
                                "'not' operator requires bool operand, found {:?}",
                                ty
                            ));
                        }
                        Ok(Type::Bool)
                    }
                    UnaryOp::Neg => {
                        if ty != Type::Int && ty != Type::Float {
                            return Err(anyhow!(
                                "Unary '-' requires numeric operand, found {:?}",
                                ty
                            ));
                        }
                        Ok(ty)
                    }
                }
            }
            Expr::Call(name, args) => {
                let (param_types, return_type) = self
                    .functions
                    .get(name)
                    .ok_or_else(|| anyhow!("Undefined function: '{}'", name))?;

                if args.len() != param_types.len() {
                    return Err(anyhow!(
                        "Function '{}' expected {} arguments, found {}",
                        name,
                        param_types.len(),
                        args.len()
                    ));
                }

                for (idx, arg) in args.iter().enumerate() {
                    let arg_ty = self.analyze_expr(arg)?;
                    if arg_ty != param_types[idx] {
                        return Err(anyhow!(
                            "Type mismatch in argument {} for call to '{}': expected {:?}, found {:?}",
                            idx + 1,
                            name,
                            param_types[idx],
                            arg_ty
                        ));
                    }
                }

                Ok(return_type.clone())
            }
        }
    }
}
