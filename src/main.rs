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
use std::fs;

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
    },
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
            let content = fs::read_to_string(&input)?;

            if lex {
                let lexer = Lexer::new(&content);
                for token in lexer {
                    println!("{:?}", token);
                }
                return Ok(());
            }

            let lexer = Lexer::new(&content);
            let mut parser = Parser::new(lexer);
            let program = parser.parse_program()?;

            if ast {
                println!("{:#?}", program);
                return Ok(());
            }

            // Semantic analysis
            let mut analyzer = Analyzer::new();
            analyzer.analyze_program(&program)?;

            if emit_llvm {
                let context = Context::create();
                let mut codegen = Codegen::new(&context, "pyrs_module");
                codegen.gen_program(program)?;
                println!("{}", codegen.module.print_to_string().to_string());
            }
        }
        Commands::Run {
            input,
            optimize,
            output,
        } => {
            let content = fs::read_to_string(&input)?;
            let lexer = Lexer::new(&content);
            let mut parser = Parser::new(lexer);
            let program = parser.parse_program()?;

            let mut analyzer = Analyzer::new();
            analyzer.analyze_program(&program)?;

            let context = Context::create();
            let mut codegen = Codegen::new(&context, "pyrs_module");
            codegen.gen_program(program)?;

            if optimize {
                codegen.optimize()?;
            }

            // Ensure build directories exist
            fs::create_dir_all(".buildout")?;
            fs::create_dir_all("bin")?;

            let bin_name = output.unwrap_or_else(|| "a.out".to_string());
            let bin_path = format!("bin/{}", bin_name);
            let ir_path = format!(".buildout/{}.ll", bin_name);

            fs::write(&ir_path, codegen.module.print_to_string().to_string())?;

            // Link with clang
            let status = std::process::Command::new("clang-20")
                .arg(&ir_path)
                .arg("-o")
                .arg(&bin_path)
                .status()?;

            if !status.success() {
                return Err(anyhow::anyhow!("Linking failed"));
            }

            // Run
            let run_status = std::process::Command::new(format!("./{}", bin_path)).status()?;

            // Cleanup temp IR file
            fs::remove_file(&ir_path)?;

            println!(
                "Program exited with status: {}",
                run_status.code().unwrap_or(-1)
            );
        }
    }

    Ok(())
}
