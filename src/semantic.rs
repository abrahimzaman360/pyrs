use crate::ast::*;
use anyhow::{Result, anyhow};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OwnershipState {
    Owned,
    Borrowed(usize), // Shared borrow count
    MutBorrowed,
    Moved,
}

pub struct SymbolTable {
    scopes: Vec<HashMap<String, (Type, bool, OwnershipState)>>, // (type, initialized, ownership)
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
            scope.insert(name, (ty, initialized, OwnershipState::Owned));
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

    pub fn lookup(&self, name: &str) -> Option<&(Type, bool, OwnershipState)> {
        for scope in self.scopes.iter().rev() {
            if let Some(entry) = scope.get(name) {
                return Some(entry);
            }
        }
        None
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut (Type, bool, OwnershipState)> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(entry) = scope.get_mut(name) {
                return Some(entry);
            }
        }
        None
    }
}

pub struct ModuleSymbols {
    pub functions: HashMap<String, (Vec<Type>, Type, bool)>, // (params, ret, is_variadic)
    pub structs: HashMap<String, Struct>,
}

impl ModuleSymbols {
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
            structs: HashMap::new(),
        }
    }
}

use crate::GcMode;

#[allow(unused)]
pub struct Analyzer {
    pub module_symbols: HashMap<String, ModuleSymbols>,
    pub current_module: String,
    symbols: SymbolTable,
    functions: HashMap<String, (Vec<Type>, Type, bool)>,
    structs: HashMap<String, Struct>,
    in_loop: bool,
    impls: HashMap<String, Vec<Impl>>,
    pub gc_mode: GcMode,
}

impl Analyzer {
    pub fn new(gc_mode: GcMode) -> Self {
        Self {
            module_symbols: HashMap::new(),
            current_module: String::new(),
            symbols: SymbolTable::new(),
            functions: HashMap::new(),
            structs: HashMap::new(),
            in_loop: false,
            impls: HashMap::new(),
            gc_mode,
        }
    }

    pub fn analyze_multi_module(&mut self, modules: &HashMap<String, crate::Module>) -> Result<()> {
        // Pass 1: Collect signatures from all modules
        for (name, module) in modules {
            self.current_module = name.clone();
            let mut symbols = ModuleSymbols::new();
            self.collect_signatures(&module.program, &mut symbols)?;
            self.module_symbols.insert(name.clone(), symbols);
        }

        // Pass 2: Analyze all module bodies
        for (name, module) in modules {
            self.current_module = name.clone();
            self.analyze_program(&module.program)?;
        }

        Ok(())
    }

    fn collect_signatures(&mut self, program: &Program, symbols: &mut ModuleSymbols) -> Result<()> {
        for item in &program.items {
            match item {
                TopLevel::Function(f) => {
                    let param_types = f.params.iter().map(|(_, t)| t.clone()).collect();
                    if symbols.functions.contains_key(&f.name) {
                        return Err(anyhow!(
                            "Function '{}' already defined in module '{}'",
                            f.name,
                            self.current_module
                        ));
                    }
                    symbols
                        .functions
                        .insert(f.name.clone(), (param_types, f.return_type.clone(), false));
                }
                TopLevel::Extern(e) => {
                    let params = e.params.iter().map(|p| p.1.clone()).collect();
                    if symbols.functions.contains_key(&e.name) {
                        return Err(anyhow!(
                            "Function '{}' already defined in module '{}'",
                            e.name,
                            self.current_module
                        ));
                    }
                    symbols.functions.insert(
                        e.name.clone(),
                        (params, e.return_type.clone(), e.is_variadic),
                    );
                }
                TopLevel::Struct(s) => {
                    if symbols.structs.contains_key(&s.name) {
                        return Err(anyhow!(
                            "Struct '{}' already defined in module '{}'",
                            s.name,
                            self.current_module
                        ));
                    }
                    symbols.structs.insert(s.name.clone(), s.clone());
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn analyze_program(&mut self, program: &Program) -> Result<()> {
        self.symbols = SymbolTable::new();
        self.functions.clear();
        self.structs.clear();

        // 1. Add local symbols
        if let Some(local_mod) = self.module_symbols.get(&self.current_module) {
            self.functions.extend(local_mod.functions.clone());
            self.structs.extend(local_mod.structs.clone());
        }

        // 2. Handle imports
        for item in &program.items {
            match item {
                TopLevel::Import(_imp) => {
                    // TODO: Support module-level access (e.g., math.sqrt)
                }
                TopLevel::FromImport(from) => {
                    let mod_name = from.module_path.join(".");
                    let other_mod = self
                        .module_symbols
                        .get(&mod_name)
                        .ok_or_else(|| anyhow!("Module '{}' not found", mod_name))?;

                    for (name, alias) in &from.names {
                        let local_name = alias.as_ref().unwrap_or(name);
                        let mut found = false;
                        if let Some((params, ret, variadic)) = other_mod.functions.get(name) {
                            self.functions.insert(
                                alias.clone().unwrap_or(name.clone()),
                                (params.clone(), ret.clone(), *variadic),
                            );
                            found = true;
                        }
                        if let Some(s) = other_mod.structs.get(name) {
                            self.structs.insert(local_name.clone(), s.clone());
                            found = true;
                        }
                        if !found {
                            return Err(anyhow!(
                                "Name '{}' not found in module '{}'",
                                name,
                                mod_name
                            ));
                        }
                    }
                }
                _ => {}
            }
        }

        // 3. Collect impl blocks
        self.impls.clear();
        for item in &program.items {
            if let TopLevel::Impl(im) = item {
                self.impls
                    .entry(im.target.clone())
                    .or_default()
                    .push(im.clone());
            }
        }

        // 4. Analyze function bodies
        for item in &program.items {
            if let TopLevel::Function(f) = item {
                self.analyze_function(f)?;
            }
        }

        // 5. Analyze impl methods
        for item in &program.items {
            if let TopLevel::Impl(im) = item {
                for method in &im.methods {
                    let mut patched = method.clone();
                    for (name, ty) in &mut patched.params {
                        if name == "self" {
                            *ty = Type::Custom(im.target.clone());
                        }
                    }
                    self.analyze_function(&patched)?;
                }
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

                    // If the expresson being assigned is a variable, and it's a move type, mark it as moved
                    if self.gc_mode == GcMode::On {
                        if let Expr::Var(src_name) = expr {
                            if self.is_move_type(&val_ty) {
                                if let Some(entry_mut) = self.symbols.get_mut(src_name) {
                                    entry_mut.2 = OwnershipState::Moved;
                                }
                            }
                        }
                    }
                }

                self.symbols.insert(name.clone(), ty.clone(), initialized)?;
            }
            Stmt::Assign(name, expr) => {
                let var_ty = {
                    let entry = self
                        .symbols
                        .lookup(name)
                        .ok_or_else(|| anyhow!("Undefined variable: '{}'", name))?;
                    entry.0.clone()
                };

                // If the expresson being assigned is a variable, and it's a move type, mark it as moved
                if self.gc_mode == GcMode::On {
                    if let Expr::Var(src_name) = expr {
                        let src_ty = self.analyze_expr(expr)?; // Get type first
                        if self.is_move_type(&src_ty) {
                            if let Some(entry_mut) = self.symbols.get_mut(src_name) {
                                entry_mut.2 = OwnershipState::Moved;
                            }
                        }
                    }
                }

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
            Stmt::IndexAssign(target, index, value) => {
                let target_ty = self.analyze_expr(target)?;
                let index_ty = self.analyze_expr(index)?;
                if index_ty != Type::Int {
                    return Err(anyhow!(
                        "List index must be an integer, found {:?}",
                        index_ty
                    ));
                }

                if self.gc_mode == GcMode::On {
                    if let Expr::Var(name) = target.as_ref() {
                        if let Some(entry) = self.symbols.lookup(name) {
                            match entry.2 {
                                OwnershipState::Borrowed(n) if n > 0 => {
                                    return Err(anyhow!(
                                        "Cannot mutate list '{}' because it is already borrowed",
                                        name
                                    ));
                                }
                                OwnershipState::MutBorrowed => {
                                    return Err(anyhow!(
                                        "Cannot mutate list '{}' because it is already borrowed as mutable",
                                        name
                                    ));
                                }
                                OwnershipState::Moved => {
                                    return Err(anyhow!("Cannot use moved value: '{}'", name));
                                }
                                _ => {}
                            }
                        }
                    }
                }

                let ele_ty = match target_ty {
                    Type::List(inner) => inner.as_ref().clone(),
                    Type::MutRef(inner) => match inner.as_ref() {
                        Type::List(ele) => ele.as_ref().clone(),
                        _ => return Err(anyhow!("Cannot index into non-list type")),
                    },
                    Type::Ref(_) => {
                        return Err(anyhow!(
                            "Cannot assign to elements through an immutable reference"
                        ));
                    }
                    _ => return Err(anyhow!("Cannot index into non-list type {:?}", target_ty)),
                };

                let val_ty = self.analyze_expr(value)?;
                if ele_ty != val_ty {
                    return Err(anyhow!(
                        "Type mismatch in indexing assignment: expected {:?}, found {:?}",
                        ele_ty,
                        val_ty
                    ));
                }
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
                    return Err(anyhow!("For loop start must be int, found {:?}", start_ty));
                }
                if end_ty != Type::Int {
                    return Err(anyhow!("For loop end must be int, found {:?}", end_ty));
                }
                if let Some(s) = step {
                    let step_ty = self.analyze_expr(s)?;
                    if step_ty != Type::Int {
                        return Err(anyhow!("For loop step must be int, found {:?}", step_ty));
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
            Stmt::MemberAssign(obj, field, value) => {
                let obj_ty = self.analyze_expr(obj)?;
                match obj_ty {
                    Type::Custom(struct_name) => {
                        let s = self
                            .structs
                            .get(&struct_name)
                            .ok_or_else(|| anyhow!("Unknown struct '{}'", struct_name))?;
                        let field_ty = s
                            .fields
                            .iter()
                            .find(|(name, _)| name == field)
                            .map(|(_, ty)| ty.clone())
                            .ok_or_else(|| {
                                anyhow!("Struct '{}' has no field '{}'", struct_name, field)
                            })?;
                        let val_ty = self.analyze_expr(value)?;
                        if val_ty != field_ty {
                            return Err(anyhow!(
                                "Type mismatch assigning to field '{}': expected {:?}, found {:?}",
                                field,
                                field_ty,
                                val_ty
                            ));
                        }
                    }
                    _ => {
                        return Err(anyhow!(
                            "Cannot assign to field of non-struct type {:?}",
                            obj_ty
                        ));
                    }
                }
            }
            Stmt::Expr(expr) => {
                self.analyze_expr(expr)?;
                // Primitive NLL: release all borrows after an expression statement
                // Since this is a toy compiler, we can just reset borrowed states if they aren't 'Owned' or 'Moved'
                // This is a bit coarse but works for the current test cases where borrows are per-call.
                self.release_all_borrows();
            }
        }
        Ok(())
    }

    fn release_all_borrows(&mut self) {
        for scope in &mut self.symbols.scopes {
            for (_, (_, _, ownership)) in scope.iter_mut() {
                if matches!(
                    ownership,
                    OwnershipState::Borrowed(_) | OwnershipState::MutBorrowed
                ) {
                    *ownership = OwnershipState::Owned;
                }
            }
        }
    }

    fn analyze_block(&mut self, body: &[Stmt], ret_ty: &Type) -> Result<()> {
        self.symbols.push_scope();
        for stmt in body {
            self.analyze_stmt(stmt, ret_ty)?;
        }
        self.symbols.pop_scope();
        Ok(())
    }

    fn is_move_type(&self, ty: &Type) -> bool {
        match ty {
            Type::List(_) | Type::Custom(_) => true,
            _ => false,
        }
    }

    fn analyze_expr(&mut self, expr: &Expr) -> Result<Type> {
        match expr {
            Expr::Int(_) => Ok(Type::Int),
            Expr::Float(_) => Ok(Type::Float),
            Expr::Bool(_) => Ok(Type::Bool),
            Expr::String(_) => Ok(Type::String),
            Expr::List(elements) => {
                if elements.is_empty() {
                    return Ok(Type::List(Box::new(Type::Void)));
                }
                let mut first_ty = None;
                for el in elements {
                    let ty = self.analyze_expr(el)?;
                    if let Some(ref fty) = first_ty {
                        if &ty != fty {
                            return Err(anyhow!(
                                "List elements must have the same type, found {:?} and {:?}",
                                fty,
                                ty
                            ));
                        }
                    } else {
                        first_ty = Some(ty);
                    }
                }
                Ok(Type::List(Box::new(first_ty.unwrap())))
            }
            Expr::Index(target, index) => {
                let target_ty = self.analyze_expr(target)?;
                let index_ty = self.analyze_expr(index)?;
                if index_ty != Type::Int {
                    return Err(anyhow!("Index must be an integer, found {:?}", index_ty));
                }
                match target_ty {
                    Type::List(inner) => Ok((*inner).clone()),
                    Type::Ref(ref inner) | Type::MutRef(ref inner) => match inner.as_ref() {
                        Type::List(ele_ty) => Ok(ele_ty.as_ref().clone()),
                        _ => Err(anyhow!("Cannot index into non-list type {:?}", target_ty)),
                    },
                    _ => Err(anyhow!("Cannot index into non-list type {:?}", target_ty)),
                }
            }
            Expr::Var(name) => {
                let (ty, init, ownership) = {
                    let entry = self
                        .symbols
                        .lookup(name)
                        .ok_or_else(|| anyhow!("Undefined variable: '{}'", name))?;
                    (entry.0.clone(), entry.1, entry.2)
                };

                if !init {
                    return Err(anyhow!("Variable '{}' used before initialization", name));
                }

                if self.gc_mode == GcMode::On && ownership == OwnershipState::Moved {
                    return Err(anyhow!("Use of moved value: '{}'", name));
                }

                Ok(ty)
            }
            Expr::Borrow(inner, is_mut) => {
                // To borrow a variable, it must be owned or already borrowed (if shared)
                let ty = self.analyze_expr(inner)?;
                if self.gc_mode == GcMode::On {
                    if let Expr::Var(name) = inner.as_ref() {
                        let (_, _, ownership) = {
                            let entry = self.symbols.lookup(name).unwrap();
                            (entry.0.clone(), entry.1, entry.2)
                        };

                        if ownership == OwnershipState::Moved {
                            return Err(anyhow!("Cannot borrow moved value: '{}'", name));
                        }

                        if *is_mut {
                            if ownership != OwnershipState::Owned {
                                return Err(anyhow!(
                                    "Cannot borrow '{}' as mutable because it is already borrowed",
                                    name
                                ));
                            }
                            if let Some(entry_mut) = self.symbols.get_mut(name) {
                                entry_mut.2 = OwnershipState::MutBorrowed;
                            }
                        } else {
                            if ownership == OwnershipState::MutBorrowed {
                                return Err(anyhow!(
                                    "Cannot borrow '{}' as shared because it is already borrowed as mutable",
                                    name
                                ));
                            }
                            if let Some(entry_mut) = self.symbols.get_mut(name) {
                                let current_count = match ownership {
                                    OwnershipState::Borrowed(n) => n,
                                    _ => 0,
                                };
                                entry_mut.2 = OwnershipState::Borrowed(current_count + 1);
                            }
                        }
                    }
                }

                if *is_mut {
                    Ok(Type::MutRef(Box::new(ty)))
                } else {
                    Ok(Type::Ref(Box::new(ty)))
                }
            }
            Expr::Deref(inner) => {
                let ty = self.analyze_expr(inner)?;
                match ty {
                    Type::Ref(inner_ty) | Type::MutRef(inner_ty) => Ok((*inner_ty).clone()),
                    _ => Err(anyhow!("Cannot dereference non-reference type {:?}", ty)),
                }
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
                if name == "print" {
                    if args.len() != 1 {
                        return Err(anyhow!(
                            "'print' takes exactly 1 argument, found {}",
                            args.len()
                        ));
                    }
                    let arg_ty = self.analyze_expr(&args[0])?;
                    return match arg_ty {
                        Type::Int | Type::Float | Type::Bool | Type::String => Ok(Type::Void),
                        _ => Err(anyhow!("'print' does not support type {:?}", arg_ty)),
                    };
                }
                // First, try to resolve as a function call
                if let Some((param_types, return_type, variadic)) =
                    self.functions.get(name).cloned()
                {
                    // Ownership check for args
                    if self.gc_mode == GcMode::On {
                        for (i, arg) in args.iter().enumerate() {
                            if i < param_types.len() {
                                let param_ty = &param_types[i];
                                let is_ref = matches!(param_ty, Type::Ref(_) | Type::MutRef(_));

                                if !is_ref {
                                    if let Expr::Var(arg_name) = arg {
                                        let arg_ty = self.analyze_expr(arg)?; // Analyze to get type
                                        if self.is_move_type(&arg_ty) {
                                            if let Some(entry_mut) = self.symbols.get_mut(arg_name)
                                            {
                                                if entry_mut.2 == OwnershipState::Moved {
                                                    return Err(anyhow!(
                                                        "Argument '{}' has already been moved",
                                                        arg_name
                                                    ));
                                                }
                                                entry_mut.2 = OwnershipState::Moved;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if args.len() < param_types.len()
                        || (!variadic && args.len() > param_types.len())
                    {
                        return Err(anyhow!(
                            "Function '{}' expected {} arguments, found {}",
                            name,
                            param_types.len(),
                            args.len()
                        ));
                    }
                    for (i, arg) in args.iter().enumerate() {
                        let arg_type = self.analyze_expr(arg)?;
                        if i < param_types.len() && arg_type != param_types[i] {
                            return Err(anyhow!(
                                "Function '{}' argument {} type mismatch: expected {:?}, found {:?}",
                                name,
                                i,
                                param_types[i],
                                arg_type
                            ));
                        }
                        // For variadic arguments, we don't have a specific type to check against
                        // They are typically handled at a lower level (e.g., C FFI)
                    }
                    return Ok(return_type);
                } else if let Some(s) = self.structs.get(name) {
                    // Constructor: params are field types, returns struct type
                    let param_types: Vec<Type> = s.fields.iter().map(|(_, t)| t.clone()).collect();
                    let return_type = Type::Custom(name.clone());

                    if args.len() != param_types.len() {
                        return Err(anyhow!(
                            "Constructor for '{}' expected {} arguments, found {}",
                            name,
                            param_types.len(),
                            args.len()
                        ));
                    }

                    for (idx, arg) in args.iter().enumerate() {
                        let arg_ty = self.analyze_expr(arg)?;
                        if arg_ty != param_types[idx] {
                            return Err(anyhow!(
                                "Type mismatch in argument {} for constructor '{}': expected {:?}, found {:?}",
                                idx + 1,
                                name,
                                param_types[idx],
                                arg_ty
                            ));
                        }
                    }
                    return Ok(return_type);
                } else {
                    Err(anyhow!("Undefined function or struct: '{}'", name))
                }
            }
            Expr::MemberAccess(expr, member) => {
                let ty = self.analyze_expr(expr)?;
                match ty {
                    Type::Custom(struct_name) => {
                        let s = self
                            .structs
                            .get(&struct_name)
                            .ok_or_else(|| anyhow!("Unknown struct '{}'", struct_name))?;
                        for (f_name, f_ty) in &s.fields {
                            if f_name == member {
                                return Ok(f_ty.clone());
                            }
                        }
                        Err(anyhow!(
                            "Struct '{}' has no field '{}'",
                            struct_name,
                            member
                        ))
                    }
                    _ => Err(anyhow!("Cannot access member of non-struct type {:?}", ty)),
                }
            }
            Expr::MethodCall(expr, method_name, args) => {
                let ty = self.analyze_expr(expr)?;
                match ty {
                    Type::Custom(struct_name) => {
                        // Look for methods in impls for this struct
                        let method_info = if let Some(impls) = self.impls.get(&struct_name) {
                            let mut found = None;
                            for im in impls {
                                for method in &im.methods {
                                    if method.name == *method_name {
                                        found = Some((
                                            method.params.clone(),
                                            method.return_type.clone(),
                                        ));
                                        break;
                                    }
                                }
                                if found.is_some() {
                                    break;
                                }
                            }
                            found
                        } else {
                            None
                        };

                        if let Some((params, return_type)) = method_info {
                            // Found the method. Verify arguments.
                            // Note: we skip 'self' which should be the first param.
                            let mut expected_params = Vec::new();
                            for (p_name, p_ty) in &params {
                                if p_name != "self" {
                                    expected_params.push(p_ty.clone());
                                }
                            }

                            if args.len() != expected_params.len() {
                                return Err(anyhow!(
                                    "Method '{}' on struct '{}' expected {} arguments, found {}",
                                    method_name,
                                    struct_name,
                                    expected_params.len(),
                                    args.len()
                                ));
                            }

                            for (idx, arg) in args.iter().enumerate() {
                                let arg_ty = self.analyze_expr(arg)?;
                                if arg_ty != expected_params[idx] {
                                    return Err(anyhow!(
                                        "Type mismatch in argument {} for call to '{}' on struct '{}': expected {:?}, found {:?}",
                                        idx + 1,
                                        method_name,
                                        struct_name,
                                        expected_params[idx],
                                        arg_ty
                                    ));
                                }
                            }

                            return Ok(return_type);
                        }
                        Err(anyhow!(
                            "No method '{}' found for struct '{}'",
                            method_name,
                            struct_name
                        ))
                    }
                    _ => Err(anyhow!("Cannot call method on non-struct type {:?}", ty)),
                }
            }
        }
    }
}
