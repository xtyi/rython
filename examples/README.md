# Debug Examples

These scripts target the current CPython 3.12 bytecode path used by `rython`.
Run one with:

```text
cargo run -- examples/01_basics.py
```

The examples cover:

- primitive values and `print()`
- integer, float, comparison, and unary operations
- tuple, list, set, and dictionary construction
- subscript reads and writes
- `if` and `while` control flow
- zero-, one-, and multi-argument `print()` calls

The VM is still a debug implementation. In particular, it does not yet
implement user-defined functions, keyword arguments, exceptions, or the full
Python object protocol.
