# Code Generation

Code Generation in PyRS is implemented in `src/codegen.rs` using the `inkwell` crate, which provides a safe wrapper around the LLVM C++ API.

## LLVM Context and Module

The `Codegen` struct holds the LLVM `Context`, `Module`, and `Builder`.

- `Context`: Owns types and constants.
- `Module`: Owns functions and global variables.
- `Builder`: Used to insert instructions into basic blocks.

## Opaque Pointers

PyRS is designed for LLVM 15+ and uses **Opaque Pointers**. This means:

- All pointer types are just `ptr`.
- Explicit types must be provided when generating `load` and `alloca` instructions.
- The `variables` map in PyRS specifically stores `(PointerValue, Type)` to support this.

## Function Generation

Each PyRS function is lowered to an LLVM function.

1. **Entry Block**: Parameters are allocated on the stack using `alloca` and initialized with the passed arguments.
2. **Statement Lowering**: Each AST statement generates a series of LLVM instructions.
3. **Terminators**: The generator ensures every basic block ends with a terminator (`br` or `ret`).

## Control Flow

`If` and `While` statements generate multiple basic blocks:

- **If**: `then`, `else`, and `merge` blocks.
- **While**: `cond`, `body`, and `end` blocks.

The generator handles the branching logic to ensure valid IR.

## String Literals

String literals are implemented as global constants. The `build_global_string_ptr` method creates a constant string in the global data segment and returns a pointer to it.

```rust
let global_str = self.builder.build_global_string_ptr(&s, "str")?;
Ok(global_str.as_basic_value_enum())
```

## Externals

External function declarations (via `extern def`) are added to the module as function prototypes without bodies. This allows the LLVM IR to refer to them, and the actual implementation is linked later (e.g., from libc).
