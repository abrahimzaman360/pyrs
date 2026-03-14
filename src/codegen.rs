use crate::ast::*;
use anyhow::{Result, anyhow};
use inkwell::IntPredicate;
use inkwell::OptimizationLevel;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::InitializationConfig;
use inkwell::targets::Target;
use inkwell::targets::TargetMachine;
use inkwell::targets::FileType;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, PointerValue};
use std::collections::HashMap;

pub struct Codegen<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,
    variables: HashMap<String, (PointerValue<'ctx>, Type)>,
    fn_value_opt: Option<FunctionValue<'ctx>>,
}

impl<'ctx> Codegen<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);

        // Initialize native target and set triple/data layout to avoid warnings
        let _ = Target::initialize_native(&InitializationConfig::default());
        let triple = TargetMachine::get_default_triple();
        module.set_triple(&triple);

        if let Ok(target) = Target::from_triple(&triple) {
            if let Some(machine) = target.create_target_machine(
                &triple,
                "generic",
                "",
                OptimizationLevel::Aggressive,
                inkwell::targets::RelocMode::Default,
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
            variables: HashMap::new(),
            fn_value_opt: None,
        }
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
                inkwell::targets::RelocMode::Default,
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
                inkwell::targets::RelocMode::Default,
                inkwell::targets::CodeModel::Default,
            )
            .ok_or_else(|| anyhow!("Could not create target machine"))?;

        machine
            .write_to_file(&self.module, FileType::Object, path)
            .map_err(|e| anyhow!(e.to_string()))?;
        Ok(())
    }

    pub fn gen_program(&mut self, program: Program) -> Result<()> {
        for item in program.items {
            match item {
                TopLevel::Function(f) => {
                    self.gen_function(f)?;
                }
                TopLevel::Extern(e) => {
                    self.gen_extern(e)?;
                }
            }
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
            self.context.void_type().fn_type(&param_types, false)
        } else {
            self.get_llvm_type(&e.return_type)
                .fn_type(&param_types, false)
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
        self.variables.clear();

        for (i, arg) in fn_val.get_param_iter().enumerate() {
            let (name, ty) = &f.params[i];
            let alloca = self.create_entry_block_alloca(&f.name, name, ty);
            self.builder.build_store(alloca, arg)?;
            self.variables.insert(name.clone(), (alloca, ty.clone()));
        }

        for stmt in f.body {
            self.gen_stmt(stmt)?;
        }

        // Add implicit return if missing
        if entry.get_terminator().is_none()
            || self
                .builder
                .get_insert_block()
                .unwrap()
                .get_terminator()
                .is_none()
        {
            if f.return_type == Type::Void {
                self.builder.build_return(None)?;
            } else {
                // This shouldn't happen in valid pyrs (all paths should return),
                // but for IR validity, we add a default.
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

    fn gen_stmt(&mut self, stmt: Stmt) -> Result<()> {
        match stmt {
            Stmt::Let(name, ty, value) => {
                let alloca = self.create_entry_block_alloca(
                    &self.fn_value_opt.unwrap().get_name().to_str().unwrap(),
                    &name,
                    &ty,
                );
                if let Some(expr) = value {
                    let val = self.gen_expr(expr)?;
                    self.builder.build_store(alloca, val)?;
                }
                self.variables.insert(name, (alloca, ty));
                Ok(())
            }
            Stmt::Assign(name, expr) => {
                let ptr = self
                    .variables
                    .get(&name)
                    .map(|(p, _)| *p)
                    .ok_or_else(|| anyhow!("Undefined variable: {}", name))?;
                let val = self.gen_expr(expr)?;
                self.builder.build_store(ptr, val)?;
                Ok(())
            }
            Stmt::If(cond, then_body, else_body) => self.gen_if(cond, then_body, else_body),
            Stmt::While(cond, body) => self.gen_while(cond, body),
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
                self.gen_expr(expr)?;
                Ok(())
            }
        }
    }

    fn gen_if(
        &mut self,
        cond: Expr,
        then_body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
    ) -> Result<()> {
        let cond_val = self.gen_expr(cond)?;
        // Handle boolean cond
        let cond_bool = self.builder.build_int_compare(
            IntPredicate::NE,
            cond_val.into_int_value(),
            self.context.bool_type().const_int(0, false),
            "ifcond",
        )?;

        let fn_val = self.fn_value_opt.unwrap();
        let then_bb = self.context.append_basic_block(fn_val, "then");
        let else_bb = self.context.append_basic_block(fn_val, "else");
        let merge_bb = self.context.append_basic_block(fn_val, "ifcont");

        self.builder
            .build_conditional_branch(cond_bool, then_bb, else_bb)?;

        // Then
        self.builder.position_at_end(then_bb);
        for stmt in then_body {
            self.gen_stmt(stmt)?;
        }
        if self.builder.get_insert_block().unwrap().get_terminator().is_none() {
            self.builder.build_unconditional_branch(merge_bb)?;
        }

        // Else
        self.builder.position_at_end(else_bb);
        if let Some(body) = else_body {
            for stmt in body {
                self.gen_stmt(stmt)?;
            }
        }
        if self.builder.get_insert_block().unwrap().get_terminator().is_none() {
            self.builder.build_unconditional_branch(merge_bb)?;
        }

        self.builder.position_at_end(merge_bb);
        Ok(())
    }

    fn gen_while(&mut self, cond: Expr, body: Vec<Stmt>) -> Result<()> {
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
        for stmt in body {
            self.gen_stmt(stmt)?;
        }
        if self.builder.get_insert_block().unwrap().get_terminator().is_none() {
            self.builder.build_unconditional_branch(cond_bb)?;
        }

        self.builder.position_at_end(end_bb);
        Ok(())
    }

    fn gen_expr(&mut self, expr: Expr) -> Result<BasicValueEnum<'ctx>> {
        match expr {
            Expr::Int(i) => Ok(self
                .context
                .i64_type()
                .const_int(i as u64, true)
                .as_basic_value_enum()),
            Expr::Float(f) => Ok(self.context.f64_type().const_float(f).as_basic_value_enum()),
            Expr::Bool(b) => Ok(self
                .context
                .bool_type()
                .const_int(if b { 1 } else { 0 }, false)
                .as_basic_value_enum()),
            Expr::String(s) => {
                // Remove quotes
                let s = &s[1..s.len() - 1];
                let s = s.replace("\\n", "\n").replace("\\t", "\t"); // Basic unescaping
                let global_str = self.builder.build_global_string_ptr(&s, "str")?;
                Ok(global_str.as_basic_value_enum())
            }
            Expr::Var(name) => {
                let (ptr, ty) = self
                    .variables
                    .get(&name)
                    .ok_or_else(|| anyhow!("Undefined variable: {}", name))?;
                Ok(self
                    .builder
                    .build_load(self.get_llvm_type(ty), *ptr, &name)?)
            }
            Expr::Binary(lhs, op, rhs) => {
                let left = self.gen_expr(*lhs)?;
                let right = self.gen_expr(*rhs)?;
                self.gen_binary_op(left, op, right)
            }
            Expr::Call(name, args) => {
                let fn_val = self
                    .module
                    .get_function(&name)
                    .ok_or_else(|| anyhow!("Undefined function: {}", name))?;
                let mut llvm_args = Vec::new();
                for arg in args {
                    llvm_args.push(self.gen_expr(arg)?.into());
                }
                let call = self.builder.build_call(fn_val, &llvm_args, "calltmp")?;
                match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => Ok(v),
                    _ => Err(anyhow!("Call returned void when value expected")),
                }
            }
            Expr::Unary(op, expr) => {
                let val = self.gen_expr(*expr)?;
                match op {
                    UnaryOp::Not => {
                        let bool_val = val.into_int_value();
                        Ok(self
                            .builder
                            .build_not(bool_val, "nottmp")?
                            .as_basic_value_enum())
                    }
                    UnaryOp::Neg => {
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
        }
    }

    fn gen_binary_op(
        &mut self,
        lhs: BasicValueEnum<'ctx>,
        op: BinaryOp,
        rhs: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>> {
        // For simplicity, assume i64 for now. In a real compiler, we'd handle types.
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

    fn get_llvm_type(&self, ty: &Type) -> BasicTypeEnum<'ctx> {
        match ty {
            Type::Int => self.context.i64_type().as_basic_type_enum(),
            Type::Float => self.context.f64_type().as_basic_type_enum(),
            Type::Bool => self.context.bool_type().as_basic_type_enum(),
            Type::String => self
                .context
                .ptr_type(inkwell::AddressSpace::from(0))
                .as_basic_type_enum(),
            Type::Void => panic!("Void has no basic type"),
            Type::Custom(_) => panic!("Custom types not implemented"),
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
