# PyRS Compiler

PyRS is a toy compiler that implements a programming language combining the clean, indentation-based syntax of **Python** with the static typing and explicit statement termination of **Rust**. It uses **LLVM** as its backend via the `inkwell` crate.

## 🚀 Key Features

- **Python-like Syntax**: Significant indentation for block structure (no curly braces).
- **Rust-like Properties**: Mandatory semicolons (`;`) and explicit type annotations.
- **Static Typing**: Type checking at compile time to catch errors early.
- **LLVM Backend**: Generates efficient machine code via LLVM 20.0.
- **Extern Functions**: Support for calling C library functions (e.g., `puts`, `printf`).

## 🛠️ Quick Start

### Prerequisites

- Rust (latest stable)
- LLVM 20.0 (`llvm-20-dev`, `libpolly-20-dev` recommended)
- `clang-20` for linking

### Build

```bash
cargo build
```

## 📂 Examples

The `examples/` directory contains sample PyRS programs:

- `fibonacci.pyrs`: Recursive Fibonacci implementation.
- `loop.pyrs`: Using `while` loops and arithmetic.
- `factorial.pyrs`: Recursive factorial.
- `extern_demo.pyrs`: Calling external C functions.

### Running an Example

To run the Fibonacci example automatically:

```bash
cargo run -- run examples/fibonacci.pyrs
```

To build and link manually:

```bash
cargo run -- build examples/fibonacci.pyrs --emit-llvm > fib.ll
clang-20 fib.ll -o fib
./fib
echo $? # Should output 55
```

## 📖 Documentation

Detailed documentation on the compiler internals can be found in the `docs/` directory:

- [Lexer](docs/lexer.md): Indentation-sensitive tokenization.
- [Parser](docs/parser.md): Recursive descent parsing and AST generation.
- [Semantic Analysis](docs/semantic.md): Name resolution and type checking.
- [CodeGen](docs/codegen.md): LLVM IR generation using Inkwell.
- [Usage](docs/usage.md): Command line flags and compilation workflow.

## 📄 License

MIT
