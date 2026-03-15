use crate::ast::*;
use anyhow::{Result, anyhow};
use inkwell::FloatPredicate;
use inkwell::IntPredicate;
use inkwell::OptimizationLevel;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::FileType;
use inkwell::targets::InitializationConfig;
use inkwell::targets::Target;
use inkwell::targets::TargetMachine;
use inkwell::types::{BasicType, BasicTypeEnum, StructType};
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, PointerValue};
use std::collections::HashMap;

struct LoopContext<'ctx> {
    cond_bb: inkwell::basic_block::BasicBlock<'ctx>,
    end_bb: inkwell::basic_block::BasicBlock<'ctx>,
}

pub struct Codegen<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,
    scopes: Vec<HashMap<String, (PointerValue<'ctx>, Type)>>,
    fn_value_opt: Option<FunctionValue<'ctx>>,
    loop_stack: Vec<LoopContext<'ctx>>,
    struct_types: HashMap<String, StructType<'ctx>>,
    struct_metadata: HashMap<String, Struct>,
    last_type: Type,
    fn_name_map: HashMap<String, String>, // internal_name -> external_name
}

use crate::semantic::ModuleSymbols;

impl<'ctx> Codegen<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);

        let _ = Target::initialize_native(&InitializationConfig::default());
        let triple = TargetMachine::get_default_triple();
        module.set_triple(&triple);

        if let Ok(target) = Target::from_triple(&triple) {
            if let Some(machine) = target.create_target_machine(
                &triple,
                "generic",
                "",
                OptimizationLevel::Aggressive,
                inkwell::targets::RelocMode::PIC,
                inkwell::targets::CodeModel::Default,
            ) {
                module.set_data_layout(&machine.get_target_data().get_data_layout());
            }
        }

        let builder = context.create_builder();
        Self {
            context,
            module,
            builder,
            scopes: vec![HashMap::new()],
            fn_value_opt: None,
            loop_stack: Vec::new(),
            struct_types: HashMap::new(),
            struct_metadata: HashMap::new(),
            last_type: Type::Void,
            fn_name_map: HashMap::new(),
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn insert_variable(&mut self, name: String, ptr: PointerValue<'ctx>, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, (ptr, ty));
        }
    }

    fn lookup_variable(&self, name: &str) -> Option<&(PointerValue<'ctx>, Type)> {
        for scope in self.scopes.iter().rev() {
            if let Some(entry) = scope.get(name) {
                return Some(entry);
            }
        }
        None
    }

    fn current_block_has_terminator(&self) -> bool {
        self.builder
            .get_insert_block()
            .and_then(|bb| bb.get_terminator())
            .is_some()
    }

    pub fn optimize(&self) -> Result<()> {
        Target::initialize_native(&InitializationConfig::default())
            .map_err(|e| anyhow!(e.to_string()))?;

        let triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&triple).map_err(|e| anyhow!(e.to_string()))?;
        let machine = target
            .create_target_machine(
                &triple,
                "generic",
                "",
                OptimizationLevel::Aggressive,
                inkwell::targets::RelocMode::PIC,
                inkwell::targets::CodeModel::Default,
            )
            .ok_or_else(|| anyhow!("Could not create target machine"))?;

        let options = PassBuilderOptions::create();
        options.set_verify_each(true);

        self.module
            .run_passes("default<O3>", &machine, options)
            .map_err(|e| anyhow!(e.to_string()))?;

        Ok(())
    }

    pub fn write_obj(&self, path: &std::path::Path) -> Result<()> {
        let triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&triple).map_err(|e| anyhow!(e.to_string()))?;
        let machine = target
            .create_target_machine(
                &triple,
                "generic",
                "",
                OptimizationLevel::Aggressive,
                inkwell::targets::RelocMode::PIC,
                inkwell::targets::CodeModel::Default,
            )
            .ok_or_else(|| anyhow!("Could not create target machine"))?;

        machine
            .write_to_file(&self.module, FileType::Object, path)
            .map_err(|e| anyhow!(e.to_string()))?;
        Ok(())
    }

    pub fn gen_program(
        &mut self,
        program: Program,
        module_symbols: &HashMap<String, ModuleSymbols>,
    ) -> Result<()> {
        for item in &program.items {
            match item {
                TopLevel::Import(_) => {
                    // TODO: Support module objects
                }
                TopLevel::FromImport(from) => {
                    self.gen_from_import(from, module_symbols)?;
                }
                TopLevel::Struct(s) => {
                    self.gen_struct_type(s)?;
                }
                _ => {}
            }
        }

        for item in program.items {
            match item {
                TopLevel::Function(f) => {
                    self.gen_function(f)?;
                }
                TopLevel::Extern(e) => {
                    self.gen_extern(e)?;
                }
                TopLevel::Impl(im) => {
                    self.gen_impl(&im)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn gen_from_import(
        &mut self,
        from: &FromImport,
        module_symbols: &HashMap<String, ModuleSymbols>,
    ) -> Result<()> {
        let mod_name = from.module_path.join(".");
        let other_mod = module_symbols
            .get(&mod_name)
            .ok_or_else(|| anyhow!("Module '{}' not found", mod_name))?;

        for (name, alias) in &from.names {
            let external_name = name;
            let internal_name = alias.as_ref().unwrap_or(name);

            if let Some((params, ret, variadic)) = other_mod.functions.get(external_name) {
                let param_types: Vec<inkwell::types::BasicMetadataTypeEnum> = params
                    .iter()
                    .map(|ty| self.get_llvm_type(ty).into())
                    .collect();

                let fn_type = if *ret == Type::Void {
                    self.context.void_type().fn_type(&param_types, *variadic)
                } else {
                    self.get_llvm_type(ret).fn_type(&param_types, *variadic)
                };

                // We add the function with its TRUE external name for linking
                self.module.add_function(external_name, fn_type, None);
                // And map the internal name to it
                self.fn_name_map
                    .insert(internal_name.clone(), external_name.clone());
            }

            if let Some(s) = other_mod.structs.get(external_name) {
                // For structs, we just need to know their layout if we use them
                self.gen_struct_type(s)?;
                // If there's an alias, we should probably map it too
                if let Some(a) = alias {
                    self.struct_types
                        .insert(a.clone(), *self.struct_types.get(external_name).unwrap());
                    self.struct_metadata.insert(a.clone(), s.clone());
                }
            }
        }
        Ok(())
    }

    fn gen_struct_type(&mut self, s: &Struct) -> Result<()> {
        let field_types: Vec<BasicTypeEnum> = s
            .fields
            .iter()
            .map(|(_, ty)| self.get_llvm_type(ty))
            .collect();
        let struct_type = self.context.opaque_struct_type(&s.name);
        struct_type.set_body(&field_types, false);
        self.struct_types.insert(s.name.clone(), struct_type);
        self.struct_metadata.insert(s.name.clone(), s.clone());
        Ok(())
    }

    fn gen_impl(&mut self, im: &Impl) -> Result<()> {
        for method in &im.methods {
            // Mangle name: StructName::MethodName
            let mangled_name = format!("{}::{}", im.target, method.name);
            let mut methods_item = method.clone();
            methods_item.name = mangled_name;
            self.gen_function(methods_item)?;
        }
        Ok(())
    }

    fn gen_extern(&mut self, e: ExternDecl) -> Result<FunctionValue<'ctx>> {
        let param_types: Vec<inkwell::types::BasicMetadataTypeEnum> = e
            .params
            .iter()
            .map(|(_, ty)| self.get_llvm_type(ty).into())
            .collect();

        let fn_type = if e.return_type == Type::Void {
            self.context
                .void_type()
                .fn_type(&param_types, e.is_variadic)
        } else {
            self.get_llvm_type(&e.return_type)
                .fn_type(&param_types, e.is_variadic)
        };

        Ok(self.module.add_function(&e.name, fn_type, None))
    }

    fn gen_function(&mut self, f: Function) -> Result<FunctionValue<'ctx>> {
        let param_types: Vec<inkwell::types::BasicMetadataTypeEnum> = f
            .params
            .iter()
            .map(|(_, ty)| self.get_llvm_type(ty).into())
            .collect();

        let fn_type = if f.return_type == Type::Void {
            self.context.void_type().fn_type(&param_types, false)
        } else {
            self.get_llvm_type(&f.return_type)
                .fn_type(&param_types, false)
        };

        let fn_val = self.module.add_function(&f.name, fn_type, None);
        let entry = self.context.append_basic_block(fn_val, "entry");
        self.builder.position_at_end(entry);

        self.fn_value_opt = Some(fn_val);
        self.scopes = vec![HashMap::new()];

        for (i, arg) in fn_val.get_param_iter().enumerate() {
            let (name, ty) = &f.params[i];
            let alloca = self.create_entry_block_alloca(&f.name, name, ty);
            self.builder.build_store(alloca, arg)?;
            self.insert_variable(name.clone(), alloca, ty.clone());
        }

        for stmt in &f.body {
            if self.current_block_has_terminator() {
                break;
            }
            self.gen_stmt(stmt)?;
        }

        // Add implicit return if the current block has no terminator
        if !self.current_block_has_terminator() {
            if f.return_type == Type::Void {
                self.builder.build_return(None)?;
            } else {
                let default_val = match f.return_type {
                    Type::Int => self
                        .context
                        .i64_type()
                        .const_int(0, false)
                        .as_basic_value_enum(),
                    Type::Float => self
                        .context
                        .f64_type()
                        .const_float(0.0)
                        .as_basic_value_enum(),
                    Type::Bool => self
                        .context
                        .bool_type()
                        .const_int(0, false)
                        .as_basic_value_enum(),
                    _ => return Err(anyhow!("Missing return in non-void function {}", f.name)),
                };
                self.builder.build_return(Some(&default_val))?;
            }
        }

        if fn_val.verify(true) {
            Ok(fn_val)
        } else {
            unsafe {
                fn_val.delete();
            }
            Err(anyhow!("Function verification failed for {}", f.name))
        }
    }

    fn gen_stmt(&mut self, stmt: &Stmt) -> Result<()> {
        if self.current_block_has_terminator() {
            return Ok(());
        }

        match stmt {
            Stmt::Let(name, ty, value) => {
                let alloca = self.create_entry_block_alloca(
                    self.fn_value_opt.unwrap().get_name().to_str().unwrap(),
                    name,
                    ty,
                );
                if let Some(expr) = value {
                    let val = self.gen_expr(expr)?;
                    self.builder.build_store(alloca, val)?;
                }
                self.insert_variable(name.clone(), alloca, ty.clone());
                Ok(())
            }
            Stmt::Assign(name, expr) => {
                let (ptr, _) = self
                    .lookup_variable(name)
                    .ok_or_else(|| anyhow!("Undefined variable: {}", name))?;
                let ptr = *ptr;
                let val = self.gen_expr(expr)?;
                self.builder.build_store(ptr, val)?;
                Ok(())
            }
            Stmt::If(cond, then_body, elif_branches, else_body) => {
                self.gen_if(cond, then_body, elif_branches, else_body)
            }
            Stmt::While(cond, body) => self.gen_while(cond, body),
            Stmt::For(var_name, start, end, step, body) => {
                self.gen_for(var_name, start, end, step, body)
            }
            Stmt::Break => {
                if let Some(loop_ctx) = self.loop_stack.last() {
                    self.builder.build_unconditional_branch(loop_ctx.end_bb)?;
                }
                Ok(())
            }
            Stmt::Continue => {
                if let Some(loop_ctx) = self.loop_stack.last() {
                    self.builder.build_unconditional_branch(loop_ctx.cond_bb)?;
                }
                Ok(())
            }
            Stmt::Return(value) => {
                if let Some(expr) = value {
                    let val = self.gen_expr(expr)?;
                    self.builder.build_return(Some(&val))?;
                } else {
                    self.builder.build_return(None)?;
                }
                Ok(())
            }
            Stmt::Expr(expr) => {
                self.gen_expr_stmt(expr)?;
                Ok(())
            }
        }
    }

    fn gen_expr_stmt(&mut self, expr: &Expr) -> Result<()> {
        match expr {
            Expr::Call(name, args) => {
                let actual_name = self.fn_name_map.get(name).unwrap_or(name);
                let fn_val = self
                    .module
                    .get_function(actual_name)
                    .ok_or_else(|| anyhow!("Undefined function: {}", actual_name))?;
                let mut llvm_args = Vec::new();
                for arg in args {
                    llvm_args.push(self.gen_expr(arg)?.into());
                }
                self.builder.build_call(fn_val, &llvm_args, "calltmp")?;
                Ok(())
            }
            _ => {
                self.gen_expr(expr)?;
                Ok(())
            }
        }
    }

    fn gen_if(
        &mut self,
        cond: &Expr,
        then_body: &[Stmt],
        elif_branches: &[(Expr, Vec<Stmt>)],
        else_body: &Option<Vec<Stmt>>,
    ) -> Result<()> {
        let fn_val = self.fn_value_opt.unwrap();
        let merge_bb = self.context.append_basic_block(fn_val, "ifcont");

        // Generate the initial if condition
        let cond_val = self.gen_expr(cond)?;
        let cond_bool = self.builder.build_int_compare(
            IntPredicate::NE,
            cond_val.into_int_value(),
            self.context.bool_type().const_int(0, false),
            "ifcond",
        )?;

        let then_bb = self.context.append_basic_block(fn_val, "then");
        let next_bb = if !elif_branches.is_empty() || else_body.is_some() {
            self.context.append_basic_block(fn_val, "else")
        } else {
            merge_bb
        };

        self.builder
            .build_conditional_branch(cond_bool, then_bb, next_bb)?;

        // Then block
        self.builder.position_at_end(then_bb);
        self.push_scope();
        for stmt in then_body {
            if self.current_block_has_terminator() {
                break;
            }
            self.gen_stmt(stmt)?;
        }
        self.pop_scope();
        if !self.current_block_has_terminator() {
            self.builder.build_unconditional_branch(merge_bb)?;
        }

        // Elif branches
        let mut current_else_bb = next_bb;
        for (i, (elif_cond, elif_body)) in elif_branches.iter().enumerate() {
            self.builder.position_at_end(current_else_bb);
            let elif_cond_val = self.gen_expr(elif_cond)?;
            let elif_cond_bool = self.builder.build_int_compare(
                IntPredicate::NE,
                elif_cond_val.into_int_value(),
                self.context.bool_type().const_int(0, false),
                "elifcond",
            )?;

            let elif_then_bb = self.context.append_basic_block(fn_val, "elif_then");
            let elif_next_bb = if i + 1 < elif_branches.len() || else_body.is_some() {
                self.context.append_basic_block(fn_val, "elif_else")
            } else {
                merge_bb
            };

            self.builder
                .build_conditional_branch(elif_cond_bool, elif_then_bb, elif_next_bb)?;

            self.builder.position_at_end(elif_then_bb);
            self.push_scope();
            for stmt in elif_body {
                if self.current_block_has_terminator() {
                    break;
                }
                self.gen_stmt(stmt)?;
            }
            self.pop_scope();
            if !self.current_block_has_terminator() {
                self.builder.build_unconditional_branch(merge_bb)?;
            }

            current_else_bb = elif_next_bb;
        }

        // Else block
        if let Some(body) = else_body {
            self.builder.position_at_end(current_else_bb);
            self.push_scope();
            for stmt in body {
                if self.current_block_has_terminator() {
                    break;
                }
                self.gen_stmt(stmt)?;
            }
            self.pop_scope();
            if !self.current_block_has_terminator() {
                self.builder.build_unconditional_branch(merge_bb)?;
            }
        }

        self.builder.position_at_end(merge_bb);
        Ok(())
    }

    fn gen_while(&mut self, cond: &Expr, body: &[Stmt]) -> Result<()> {
        let fn_val = self.fn_value_opt.unwrap();
        let cond_bb = self.context.append_basic_block(fn_val, "whilecond");
        let body_bb = self.context.append_basic_block(fn_val, "whilebody");
        let end_bb = self.context.append_basic_block(fn_val, "whileend");

        self.builder.build_unconditional_branch(cond_bb)?;
        self.builder.position_at_end(cond_bb);
        let cond_val = self.gen_expr(cond)?;
        let cond_bool = self.builder.build_int_compare(
            IntPredicate::NE,
            cond_val.into_int_value(),
            self.context.bool_type().const_int(0, false),
            "whilecond",
        )?;
        self.builder
            .build_conditional_branch(cond_bool, body_bb, end_bb)?;

        self.builder.position_at_end(body_bb);
        self.loop_stack.push(LoopContext { cond_bb, end_bb });
        self.push_scope();
        for stmt in body {
            if self.current_block_has_terminator() {
                break;
            }
            self.gen_stmt(stmt)?;
        }
        self.pop_scope();
        self.loop_stack.pop();
        if !self.current_block_has_terminator() {
            self.builder.build_unconditional_branch(cond_bb)?;
        }

        self.builder.position_at_end(end_bb);
        Ok(())
    }

    fn gen_for(
        &mut self,
        var_name: &str,
        start: &Expr,
        end: &Expr,
        step: &Option<Expr>,
        body: &[Stmt],
    ) -> Result<()> {
        let fn_val = self.fn_value_opt.unwrap();

        // Evaluate start and end
        let start_val = self.gen_expr(start)?;
        let end_val = self.gen_expr(end)?;
        let step_val = if let Some(s) = step {
            self.gen_expr(s)?
        } else {
            self.context
                .i64_type()
                .const_int(1, false)
                .as_basic_value_enum()
        };

        // Alloca for loop variable
        let alloca = self.create_entry_block_alloca(
            fn_val.get_name().to_str().unwrap(),
            var_name,
            &Type::Int,
        );
        self.builder.build_store(alloca, start_val)?;

        let cond_bb = self.context.append_basic_block(fn_val, "forcond");
        let body_bb = self.context.append_basic_block(fn_val, "forbody");
        let step_bb = self.context.append_basic_block(fn_val, "forstep");
        let end_bb = self.context.append_basic_block(fn_val, "forend");

        self.builder.build_unconditional_branch(cond_bb)?;

        // Condition: i < end
        self.builder.position_at_end(cond_bb);
        let cur_val = self
            .builder
            .build_load(self.context.i64_type(), alloca, var_name)?;
        let cond_bool = self.builder.build_int_compare(
            IntPredicate::SLT,
            cur_val.into_int_value(),
            end_val.into_int_value(),
            "forcond",
        )?;
        self.builder
            .build_conditional_branch(cond_bool, body_bb, end_bb)?;

        // Body
        self.builder.position_at_end(body_bb);
        self.loop_stack.push(LoopContext {
            cond_bb: step_bb,
            end_bb,
        });
        self.push_scope();
        self.insert_variable(var_name.to_string(), alloca, Type::Int);
        for stmt in body {
            if self.current_block_has_terminator() {
                break;
            }
            self.gen_stmt(stmt)?;
        }
        self.pop_scope();
        self.loop_stack.pop();
        if !self.current_block_has_terminator() {
            self.builder.build_unconditional_branch(step_bb)?;
        }

        // Step: i = i + step
        self.builder.position_at_end(step_bb);
        let cur = self
            .builder
            .build_load(self.context.i64_type(), alloca, "cur")?;
        let next =
            self.builder
                .build_int_add(cur.into_int_value(), step_val.into_int_value(), "nexti")?;
        self.builder.build_store(alloca, next)?;
        self.builder.build_unconditional_branch(cond_bb)?;

        self.builder.position_at_end(end_bb);
        Ok(())
    }

    fn gen_expr(&mut self, expr: &Expr) -> Result<BasicValueEnum<'ctx>> {
        let val = match expr {
            Expr::Int(i) => {
                self.last_type = Type::Int;
                Ok(self
                    .context
                    .i64_type()
                    .const_int(*i as u64, true)
                    .as_basic_value_enum())
            }
            Expr::Float(f) => {
                self.last_type = Type::Float;
                Ok(self
                    .context
                    .f64_type()
                    .const_float(*f)
                    .as_basic_value_enum())
            }
            Expr::Bool(b) => {
                self.last_type = Type::Bool;
                Ok(self
                    .context
                    .bool_type()
                    .const_int(if *b { 1 } else { 0 }, false)
                    .as_basic_value_enum())
            }
            Expr::String(s) => {
                self.last_type = Type::String;
                // Remove quotes
                let s = &s[1..s.len() - 1];
                let s = s
                    .replace("\\n", "\n")
                    .replace("\\t", "\t")
                    .replace("\\\\", "\\")
                    .replace("\\\"", "\"")
                    .replace("\\0", "\0");
                let global_str = self.builder.build_global_string_ptr(&s, "str")?;
                Ok(global_str.as_basic_value_enum())
            }
            Expr::MemberAccess(expr, field_name) => {
                let val = self.gen_expr(expr)?;
                let ptr = val.into_pointer_value();
                let struct_name = match &self.last_type {
                    Type::Custom(name) => name.clone(),
                    _ => {
                        return Err(anyhow!(
                            "Cannot access member of non-struct type {:?}",
                            self.last_type
                        ));
                    }
                };

                let struct_type = self
                    .struct_types
                    .get(&struct_name)
                    .ok_or_else(|| anyhow!("Struct type not found for {}", struct_name))?;
                let metadata = self
                    .struct_metadata
                    .get(&struct_name)
                    .ok_or_else(|| anyhow!("Metadata not found for struct {}", struct_name))?;

                let field_idx = metadata
                    .fields
                    .iter()
                    .position(|f| &f.0 == field_name)
                    .ok_or_else(|| {
                        anyhow!("Field {} not found in struct {}", field_name, struct_name)
                    })?;

                let field_ptr = self.builder.build_struct_gep(
                    *struct_type,
                    ptr,
                    field_idx as u32,
                    field_name,
                )?;
                let pyrs_field_ty = metadata.fields[field_idx].1.clone();
                let llvm_field_ty = self.get_llvm_type(&pyrs_field_ty);

                self.last_type = pyrs_field_ty;
                Ok(self
                    .builder
                    .build_load(llvm_field_ty, field_ptr, field_name)?)
            }
            Expr::MethodCall(expr, method_name, args) => {
                let val = self.gen_expr(expr)?;
                let ptr = val.into_pointer_value();
                let struct_name = match &self.last_type {
                    Type::Custom(name) => name.clone(),
                    _ => {
                        return Err(anyhow!(
                            "Cannot call method on non-struct type {:?}",
                            self.last_type
                        ));
                    }
                };

                let mangled_name = format!("{}::{}", struct_name, method_name);
                let fn_val = self.module.get_function(&mangled_name).ok_or_else(|| {
                    anyhow!(
                        "Method {} not found for struct {}",
                        method_name,
                        struct_name
                    )
                })?;

                let mut llvm_args = Vec::new();
                llvm_args.push(ptr.into()); // self
                for arg in args {
                    llvm_args.push(self.gen_expr(arg)?.into());
                }

                let call = self.builder.build_call(fn_val, &llvm_args, "methodtmp")?;

                // For now, we don't have return type info here, so we set it to Void
                self.last_type = Type::Void;

                match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => Ok(v),
                    _ => Ok(self
                        .context
                        .i64_type()
                        .const_int(0, false)
                        .as_basic_value_enum()), // Default for void
                }
            }
            Expr::Var(name) => {
                let (ptr, ty) = self
                    .lookup_variable(name)
                    .ok_or_else(|| anyhow!("Undefined variable: {}", name))?;
                let ptr = *ptr;
                let ty = ty.clone();
                self.last_type = ty.clone();
                Ok(self
                    .builder
                    .build_load(self.get_llvm_type(&ty), ptr, name)?)
            }
            Expr::Binary(lhs, op, rhs) => {
                let left = self.gen_expr(lhs)?;
                let right = self.gen_expr(rhs)?;
                let res = self.gen_binary_op(left, op, right)?;
                if let BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::Lt
                | BinaryOp::Gt
                | BinaryOp::Le
                | BinaryOp::Ge
                | BinaryOp::And
                | BinaryOp::Or = op
                {
                    self.last_type = Type::Bool;
                }
                Ok(res)
            }
            Expr::Unary(op, inner) => {
                let val = self.gen_expr(inner)?;
                match op {
                    UnaryOp::Not => {
                        self.last_type = Type::Bool;
                        let bool_val = val.into_int_value();
                        let result = self.builder.build_xor(
                            bool_val,
                            self.context.bool_type().const_int(1, false),
                            "nottmp",
                        )?;
                        Ok(result.as_basic_value_enum())
                    }
                    UnaryOp::Neg => {
                        // last_type stays same
                        if val.is_int_value() {
                            Ok(self
                                .builder
                                .build_int_neg(val.into_int_value(), "negtmp")?
                                .as_basic_value_enum())
                        } else {
                            Ok(self
                                .builder
                                .build_float_neg(val.into_float_value(), "negtmp")?
                                .as_basic_value_enum())
                        }
                    }
                }
            }
            Expr::Call(name, args) => {
                if let Some(st) = self.struct_types.get(name).cloned() {
                    // Constructor
                    let alloca = self.create_entry_block_alloca(
                        self.fn_value_opt.unwrap().get_name().to_str().unwrap(),
                        "struct_tmp",
                        &Type::Custom(name.clone()),
                    );
                    for (i, arg) in args.iter().enumerate() {
                        let val = self.gen_expr(arg)?;
                        let field_ptr =
                            self.builder
                                .build_struct_gep(st, alloca, i as u32, "field_ptr")?;
                        self.builder.build_store(field_ptr, val)?;
                    }
                    self.last_type = Type::Custom(name.clone());
                    Ok(alloca.as_basic_value_enum())
                } else {
                    let fn_val = self
                        .module
                        .get_function(name)
                        .ok_or_else(|| anyhow!("Undefined function: {}", name))?;
                    let mut llvm_args = Vec::new();
                    for arg in args {
                        llvm_args.push(self.gen_expr(arg)?.into());
                    }
                    let call = self.builder.build_call(fn_val, &llvm_args, "calltmp")?;
                    match call.try_as_basic_value() {
                        inkwell::values::ValueKind::Basic(v) => Ok(v),
                        _ => Err(anyhow!(
                            "Call to '{}' returned void when value expected",
                            name
                        )),
                    }
                }
            }
        };
        val
    }

    fn gen_binary_op(
        &mut self,
        lhs: BasicValueEnum<'ctx>,
        op: &BinaryOp,
        rhs: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>> {
        // Determine if we're dealing with int or float
        let is_float = lhs.is_float_value();

        if is_float {
            let l = lhs.into_float_value();
            let r = rhs.into_float_value();
            match op {
                BinaryOp::Add => Ok(self
                    .builder
                    .build_float_add(l, r, "addtmp")?
                    .as_basic_value_enum()),
                BinaryOp::Sub => Ok(self
                    .builder
                    .build_float_sub(l, r, "subtmp")?
                    .as_basic_value_enum()),
                BinaryOp::Mul => Ok(self
                    .builder
                    .build_float_mul(l, r, "multmp")?
                    .as_basic_value_enum()),
                BinaryOp::Div => Ok(self
                    .builder
                    .build_float_div(l, r, "divtmp")?
                    .as_basic_value_enum()),
                BinaryOp::Eq => Ok(self
                    .builder
                    .build_float_compare(FloatPredicate::OEQ, l, r, "eqtmp")?
                    .as_basic_value_enum()),
                BinaryOp::Ne => Ok(self
                    .builder
                    .build_float_compare(FloatPredicate::ONE, l, r, "netmp")?
                    .as_basic_value_enum()),
                BinaryOp::Lt => Ok(self
                    .builder
                    .build_float_compare(FloatPredicate::OLT, l, r, "lttmp")?
                    .as_basic_value_enum()),
                BinaryOp::Gt => Ok(self
                    .builder
                    .build_float_compare(FloatPredicate::OGT, l, r, "gttmp")?
                    .as_basic_value_enum()),
                BinaryOp::Le => Ok(self
                    .builder
                    .build_float_compare(FloatPredicate::OLE, l, r, "letmp")?
                    .as_basic_value_enum()),
                BinaryOp::Ge => Ok(self
                    .builder
                    .build_float_compare(FloatPredicate::OGE, l, r, "getmp")?
                    .as_basic_value_enum()),
                BinaryOp::Mod => Err(anyhow!("Modulo not supported for float")),
                BinaryOp::And | BinaryOp::Or => {
                    Err(anyhow!("Logical operators not supported for float"))
                }
            }
        } else {
            let l = lhs.into_int_value();
            let r = rhs.into_int_value();
            match op {
                BinaryOp::Add => Ok(self
                    .builder
                    .build_int_add(l, r, "addtmp")?
                    .as_basic_value_enum()),
                BinaryOp::Sub => Ok(self
                    .builder
                    .build_int_sub(l, r, "subtmp")?
                    .as_basic_value_enum()),
                BinaryOp::Mul => Ok(self
                    .builder
                    .build_int_mul(l, r, "multmp")?
                    .as_basic_value_enum()),
                BinaryOp::Div => Ok(self
                    .builder
                    .build_int_signed_div(l, r, "divtmp")?
                    .as_basic_value_enum()),
                BinaryOp::Mod => Ok(self
                    .builder
                    .build_int_signed_rem(l, r, "modtmp")?
                    .as_basic_value_enum()),
                BinaryOp::Eq => Ok(self
                    .builder
                    .build_int_compare(IntPredicate::EQ, l, r, "eqtmp")?
                    .as_basic_value_enum()),
                BinaryOp::Ne => Ok(self
                    .builder
                    .build_int_compare(IntPredicate::NE, l, r, "netmp")?
                    .as_basic_value_enum()),
                BinaryOp::Lt => Ok(self
                    .builder
                    .build_int_compare(IntPredicate::SLT, l, r, "lttmp")?
                    .as_basic_value_enum()),
                BinaryOp::Gt => Ok(self
                    .builder
                    .build_int_compare(IntPredicate::SGT, l, r, "gttmp")?
                    .as_basic_value_enum()),
                BinaryOp::Le => Ok(self
                    .builder
                    .build_int_compare(IntPredicate::SLE, l, r, "letmp")?
                    .as_basic_value_enum()),
                BinaryOp::Ge => Ok(self
                    .builder
                    .build_int_compare(IntPredicate::SGE, l, r, "getmp")?
                    .as_basic_value_enum()),
                BinaryOp::And => Ok(self
                    .builder
                    .build_and(l, r, "andtmp")?
                    .as_basic_value_enum()),
                BinaryOp::Or => Ok(self.builder.build_or(l, r, "ortmp")?.as_basic_value_enum()),
            }
        }
    }

    fn get_llvm_type(&self, ty: &Type) -> BasicTypeEnum<'ctx> {
        match ty {
            Type::Int => self.context.i64_type().as_basic_type_enum(),
            Type::Float => self.context.f64_type().as_basic_type_enum(),
            Type::Bool => self.context.bool_type().as_basic_type_enum(),
            Type::String => self
                .context
                .ptr_type(inkwell::AddressSpace::from(0))
                .as_basic_type_enum(),
            Type::Custom(name) => {
                if let Some(_st) = self.struct_types.get(name) {
                    self.context
                        .ptr_type(inkwell::AddressSpace::from(0))
                        .as_basic_type_enum()
                } else {
                    panic!("Unknown custom type: {}", name);
                }
            }
            Type::Void => panic!("Void has no basic type"),
        }
    }

    fn create_entry_block_alloca(
        &self,
        fn_name: &str,
        var_name: &str,
        ty: &Type,
    ) -> PointerValue<'ctx> {
        let builder = self.context.create_builder();
        let entry = self
            .module
            .get_function(fn_name)
            .unwrap()
            .get_first_basic_block()
            .unwrap();

        match entry.get_first_instruction() {
            Some(first_instr) => builder.position_before(&first_instr),
            None => builder.position_at_end(entry),
        }

        builder
            .build_alloca(self.get_llvm_type(ty), var_name)
            .unwrap()
    }
}
