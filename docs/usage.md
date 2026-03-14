# Usage Guide

The PyRS compiler provides a command-line interface for compiling and inspecting source files.

## Command Line Arguments

```bash
pyrs <COMMAND> [OPTIONS] <INPUT>
```

### Commands

- `build`: Parses the file and optionally emits tokens, AST, or LLVM IR.
- `run`: Compiles the file, links it with `clang`, and executes the binary.

### Build Flags

- `-l, --lex`: Run the lexer and print the stream of tokens.
- `-a, --ast`: Run the parser and print the Abstract Syntax Tree (AST).
- `--emit-llvm`: Transports the AST through Semantic Analysis and Codegen, then prints the generated LLVM IR to stdout.

### Run Flags

- `-O, --optimize`: Run LLVM optimization passes (Aggressive/O3).
- `-o, --output <FILE>`: Specify the output binary name (defaults to `a.out`).

## Compilation Workflow

The `run` subcommand automates the entire process:

```bash
cargo run -- run examples/fibonacci.pyrs --optimize --output fib
```

1. **Generate LLVM IR**:

    ```bash
    cargo run -- build your_file.pyrs --emit-llvm > output.ll
    ```

2. **Compile and Link**:

    ```bash
    clang-20 output.ll -o your_binary
    ```

3. **Run**:

    ```bash
    ./your_binary
    ```

## Example: Hello World

`hello.pyrs`:

```python
extern def puts(s: str) -> int;

def main() -> int:
    puts("Hello World!");
    return 0;
```

Full command chain:

```bash
cargo build
./target/debug/pyrs build hello.pyrs --emit-llvm > hello.ll
clang-20 hello.ll -o hello
./hello
```
