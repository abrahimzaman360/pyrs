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

const RUNTIME_C: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/runtime.c");

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

        /// Garbage Collection mode
        #[arg(long, value_enum, default_value_t = GcMode::Off)]
        gc: GcMode,
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
        #[arg(long, default_value = "cc")]
        cc: String,

        /// Garbage Collection mode
        #[arg(long, value_enum, default_value_t = GcMode::Off)]
        gc: GcMode,
    },
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq)]
pub enum GcMode {
    On,
    Off,
    Dyn,
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
        // Try relative to current file first
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
            return Ok(path);
        }

        // Try stdlib directory
        let std_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib");
        let mut std_path = std_root;
        for (i, part) in module_path.iter().enumerate() {
            std_path.push(part);
            if i == module_path.len() - 1 {
                std_path.set_extension("pyrs");
            }
        }
        if std_path.exists() {
            return Ok(std_path);
        }

        Err(anyhow::anyhow!(
            "Module not found: {} (searched relative to '{}' and stdlib)",
            module_path.join("."),
            current_file.display()
        ))
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
            gc,
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
            let mut analyzer = Analyzer::new(gc);
            analyzer.analyze_multi_module(&modules)?;

            if emit_llvm {
                let context = Context::create();
                for (name, module) in &modules {
                    println!("--- Module: {} ---", name);
                    let mut codegen = Codegen::new(&context, name, gc);
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
            gc,
        } => {
            let entry_path = Path::new(&input);
            let mut loader = ModuleLoader::new();
            let modules = loader.load_all(entry_path)?;

            let mut analyzer = Analyzer::new(gc);
            analyzer.analyze_multi_module(&modules)?;

            let context = Context::create();
            let mut obj_files = Vec::new();

            // Ensure build directories exist
            fs::create_dir_all(".buildout")?;
            fs::create_dir_all("bin")?;

            for (name, module) in &modules {
                let mut codegen = Codegen::new(&context, name, gc);
                codegen.gen_program(module.program.clone(), &analyzer.module_symbols)?;

                if optimize {
                    codegen.optimize()?;
                }

                let obj_path = format!(".buildout/{}.o", name.replace(".", "_"));
                codegen.write_obj(Path::new(&obj_path))?;
                obj_files.push(obj_path);
            }

            // Compile runtime.c (use a gc-mode-specific object file to avoid collisions)
            let runtime_o = match gc {
                GcMode::On => ".buildout/runtime_gc_on.o",
                GcMode::Dyn => ".buildout/runtime_gc_dyn.o",
                GcMode::Off => ".buildout/runtime_gc_off.o",
            };
            let mut runtime_cc = std::process::Command::new(&cc);
            runtime_cc
                .arg("-c")
                .arg(RUNTIME_C)
                .arg("-o")
                .arg(runtime_o);
            let mut use_gc_link = false;
            if gc == GcMode::On || gc == GcMode::Dyn {
                runtime_cc.arg("-DUSE_GC");
            }
            let runtime_status = runtime_cc.status()?;
            if runtime_status.success() {
                if gc == GcMode::On || gc == GcMode::Dyn {
                    use_gc_link = true;
                }
            } else {
                // If it fails, maybe it's because libgc is missing. Try without it if dyn.
                if gc == GcMode::Dyn {
                    let mut fallback_cc = std::process::Command::new(&cc);
                    fallback_cc
                        .arg("-c")
                        .arg(RUNTIME_C)
                        .arg("-o")
                        .arg(runtime_o);
                    let fallback_status = fallback_cc.status()?;
                    if !fallback_status.success() {
                        return Err(anyhow::anyhow!("Failed to compile runtime.c"));
                    }
                    use_gc_link = false;
                } else {
                    return Err(anyhow::anyhow!(
                        "Failed to compile runtime.c (try --gc dyn or install libgc-dev)"
                    ));
                }
            }
            obj_files.push(runtime_o.to_string());

            let bin_name = output.unwrap_or_else(|| "a.out".to_string());
            let bin_path = format!("bin/{}", bin_name);

            // Link with the specified compiler
            let mut cmd = std::process::Command::new(&cc);
            for obj in &obj_files {
                cmd.arg(obj);
            }
            if use_gc_link {
                cmd.arg("-lgc");
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
                let _ = fs::remove_file(obj);
            }

            std::process::exit(run_status.code().unwrap_or(-1));
        }
    }

    Ok(())
}
