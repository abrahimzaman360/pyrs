# Semantic Analysis

Semantic Analysis is implemented in `src/semantic.rs`. This phase ensures that the program is logically sound before code generation. It specifically handles **Name Resolution** and **Type Checking**.

## Name Resolution

The `Analyzer` traverses the AST and maintains a `SymbolTable`. It ensures that:

- Variables are declared before they are used.
- Variables are not redefined in the same scope.
- Function calls refer to functions that have been defined or declared via `extern`.

### Scoping

PyRS uses lexical scoping. Each block (`if`, `while`, function body) creates a new scope. The `SymbolTable` uses a stack of HashMaps to manage these nested scopes.

## Type Checking

PyRS is statically typed. The `Analyzer` verifies that:

- The types in `let` bindings match the initialized values.
- Binary operations (like `+` or `==`) are performed on compatible types.
- Function arguments match the parameter types in the function signature.
- Return statements provide values that match the function's return type.

### Type Error Example

```python
def main():
    let x: int = 5;
    let y: float = 10.0;
    let z: int = x + y; // Error: Mismatched types Int and Float
```

## Implementation

The entry point is `Analyzer::analyze_program(&self, program: &Program)`. It performs a two-pass analysis:

1. **Global Collection**: Collects all function and extern signatures into a global map.
2. **Local Analysis**: Recursively analyzes each function's body for statements and expressions.
