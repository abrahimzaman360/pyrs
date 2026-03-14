# Lexer

The Lexer in PyRS is implemented in `src/lexer.rs` using the `logos` crate. It is responsible for converting the raw source code into a stream of tokens while handling Python-style indentation and mandatory semicolons.

## Indentation Sensitivity

PyRS uses indentation to define block structure, similar to Python. The lexer tracks indentation levels using a stack and emits `Indent` and `Dedent` tokens appropriately.

### How it works

1. **Line Start**: At the beginning of each line, the lexer scans the leading whitespace.
2. **Comparison**: The current indentation width is compared against the top of the `indent_stack`.
3. **Token Generation**:
    * If `new_width > current_width`: Push `new_width` to the stack and emit an `Indent` token.
    * If `new_width < current_width`: Pop widths from the stack until a matching width is found, emitting a `Dedent` token for each pop.
4. **Blank Lines**: Lines containing only whitespace are skipped.

## Token Types

The lexer defines two sets of tokens:

* `RawToken`: The direct output of the `logos` tokenizer (includes keywords, symbols, and literals).
* `Token`: The final token stream processed for indentation and newlines.

## Semicolons

While PyRS uses indentation for blocks, it requires semicolons (`;`) to terminate statements. This combines Rust's explicitness with Python's visual structure.

```python
def example():
    let x: int = 5; // Semicolon required
    if x > 0:
        return x;   // Semicolon required
```

## Comments

PyRS supports single-line comments starting with `//`. The lexer skips these during tokenization.

```python
// This is a comment
let x: int = 10;
```
