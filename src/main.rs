mod ast;
mod codegen;
mod lexer;
mod parser;
mod semantic;

use clap::Parser as ClapParser;
use codegen::Codegen;
use inkwell::context::Context;
use lexer::Lexer;
use parser::Parser;
use semantic::Analyzer;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(ClapParser, Debug)]
#[command(
    name = "pyrs",
    author = "Ibrahim Zaman",
    version = "0.1.0",
    about = "PyRS: A Python-syntax compiler with Rust-like static typing and LLVM backend.",
    long_about = "PyRS is a toy compiler that implements a programming language combining the clean, \
                  indentation-based syntax of Python with the static typing and explicit statement \
                  termination of Rust. It uses LLVM as its backend via the inkwell crate."
)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Build a PyRS file
    Build {
        /// Input file
        input: String,

        /// Only run lexer and print tokens
        #[arg(short, long)]
        lex: bool,

        /// Only run parser and print AST
        #[arg(short, long)]
        ast: bool,

        /// Emit LLVM IR to stdout
        #[arg(long)]
        emit_llvm: bool,
    },
    /// Build and run a PyRS file
    Run {
        /// Input file
        input: String,

        /// Run LLVM optimization passes
        #[arg(short = 'O', long)]
        optimize: bool,

        /// Output binary name
        #[arg(short = 'o', long)]
        output: Option<String>,

        /// C compiler/linker to use
        #[arg(long, default_value = "clang-20")]
        cc: String,
    },
}

#[allow(unused)]
#[derive(Debug)]
struct Module {
    path: PathBuf,
    program: ast::Program,
}

struct ModuleLoader {
    modules: HashMap<String, Module>,
    pending: VecDeque<(String, PathBuf)>,
}

impl ModuleLoader {
    fn new() -> Self {
        Self {
            modules: HashMap::new(),
            pending: VecDeque::new(),
        }
    }

    fn load_all(&mut self, entry_path: &Path) -> anyhow::Result<HashMap<String, Module>> {
        let entry_name = entry_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("main")
            .to_string();

        self.pending
            .push_back((entry_name, entry_path.to_path_buf()));

        while let Some((module_name, path)) = self.pending.pop_front() {
            if self.modules.contains_key(&module_name) {
                continue;
            }

            let content = fs::read_to_string(&path)?;
            let lexer = Lexer::new(&content);
            let mut parser = Parser::new(lexer);
            let program = parser.parse_program()?;

            // Extract dependencies
            for item in &program.items {
                match item {
                    ast::TopLevel::Import(imp) => {
                        let dep_name = imp.path.join(".");
                        let dep_path = self.resolve_path(&path, &imp.path)?;
                        if !self.modules.contains_key(&dep_name) {
                            self.pending.push_back((dep_name, dep_path));
                        }
                    }
                    ast::TopLevel::FromImport(from) => {
                        let dep_name = from.module_path.join(".");
                        let dep_path = self.resolve_path(&path, &from.module_path)?;
                        if !self.modules.contains_key(&dep_name) {
                            self.pending.push_back((dep_name, dep_path));
                        }
                    }
                    _ => {}
                }
            }

            self.modules
                .insert(module_name.clone(), Module { path, program });
        }

        Ok(std::mem::take(&mut self.modules))
    }

    fn resolve_path(&self, current_file: &Path, module_path: &[String]) -> anyhow::Result<PathBuf> {
        let mut path = current_file
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        for (i, part) in module_path.iter().enumerate() {
            path.push(part);
            if i == module_path.len() - 1 {
                path.set_extension("pyrs");
            }
        }

        if path.exists() {
            Ok(path)
        } else {
            Err(anyhow::anyhow!(
                "Module not found: {}",
                module_path.join(".")
            ))
        }
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Build {
            input,
            lex,
            ast,
            emit_llvm,
        } => {
            let entry_path = Path::new(&input);
            let mut loader = ModuleLoader::new();
            let modules = loader.load_all(entry_path)?;

            if lex {
                for (name, module) in &modules {
                    println!("--- Module: {} ---", name);
                    let content = fs::read_to_string(&module.path)?;
                    let lexer = Lexer::new(&content);
                    for token in lexer {
                        println!("{:?}", token);
                    }
                }
                return Ok(());
            }

            if ast {
                for (name, module) in &modules {
                    println!("--- Module: {} ---", name);
                    println!("{:#?}", module.program);
                }
                return Ok(());
            }

            // Semantic analysis
            let mut analyzer = Analyzer::new();
            analyzer.analyze_multi_module(&modules)?;

            if emit_llvm {
                let context = Context::create();
                for (name, module) in &modules {
                    println!("--- Module: {} ---", name);
                    let mut codegen = Codegen::new(&context, name);
                    codegen.gen_program(module.program.clone(), &analyzer.module_symbols)?;
                    println!("{}", codegen.module.print_to_string().to_string());
                }
            }
        }
        Commands::Run {
            input,
            optimize,
            output,
            cc,
        } => {
            let entry_path = Path::new(&input);
            let mut loader = ModuleLoader::new();
            let modules = loader.load_all(entry_path)?;

            let mut analyzer = Analyzer::new();
            analyzer.analyze_multi_module(&modules)?;

            let context = Context::create();
            let mut obj_files = Vec::new();

            // Ensure build directories exist
            fs::create_dir_all(".buildout")?;
            fs::create_dir_all("bin")?;

            for (name, module) in &modules {
                let mut codegen = Codegen::new(&context, name);
                codegen.gen_program(module.program.clone(), &analyzer.module_symbols)?;

                if optimize {
                    codegen.optimize()?;
                }

                let obj_path = format!(".buildout/{}.o", name.replace(".", "_"));
                codegen.write_obj(Path::new(&obj_path))?;
                obj_files.push(obj_path);
            }

            let bin_name = output.unwrap_or_else(|| "a.out".to_string());
            let bin_path = format!("bin/{}", bin_name);

            // Link with the specified compiler
            let mut cmd = std::process::Command::new(&cc);
            for obj in &obj_files {
                cmd.arg(obj);
            }
            cmd.arg("-o").arg(&bin_path);

            let status = cmd.status()?;

            if !status.success() {
                return Err(anyhow::anyhow!("Linking failed"));
            }

            // Run
            let run_status = std::process::Command::new(format!("./{}", bin_path)).status()?;

            // Cleanup temp object files
            for obj in obj_files {
                fs::remove_file(obj)?;
            }

            println!(
                "Program exited with status: {}",
                run_status.code().unwrap_or(-1)
            );
        }
    }

    Ok(())
}
