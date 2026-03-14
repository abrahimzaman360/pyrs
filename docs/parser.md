# Parser

The Parser in PyRS is a **Recursive Descent Parser** implemented in `src/parser.rs`. It consumes the stream of tokens provided by the Lexer and constructs an Abstract Syntax Tree (AST).

## Abstract Syntax Tree (AST)

The AST is defined in `src/ast.rs` using Rust enums and structs. The top-level structure is `Program`, which contains a vector of `TopLevel` items (Functions or Extern Declarations).

### Core Components

- `Expr`: Represents expressions (literals, variables, binary operations, calls).
- `Stmt`: Represents statements (`let`, `if`, `while`, `return`, expression-statements).
- `Function`: Represents an internal function definition.
- `ExternDecl`: Represents an external function prototype.

## Parsing Logic

The parser follows a strict hierarchy of precedence to handle expressions correctly.

### Expression Precedence (Bottom to Top)

1. **Primary**: Literals, identifiers, parenthesized expressions.
2. **Factor**: Multiplication (`*`) and Division (`/`).
3. **Term**: Addition (`+`) and Subtraction (`-`).
4. **Comparison**: `<`, `>`, `<=`, `>=`.
5. **Equality**: `==`, `!=`.

## Indentation Blocks

The parser handles blocks by expecting an `Indent` token, followed by a sequence of statements, and terminated by a `Dedent` token.

```rust
fn parse_block(&mut self) -> Result<Vec<Stmt>> {
    self.expect(Token::Indent)?;
    let mut stmts = Vec::new();
    while self.peek() != Some(&Token::Dedent) {
        stmts.push(self.parse_stmt()?);
    }
    self.expect(Token::Dedent)?;
    Ok(stmts)
}
```

## External Functions

External functions are parsed using the `extern` keyword. They do not have a body and must end with a semicolon.

```python
extern def puts(s: str) -> int;
```
