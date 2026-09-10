# Rython Runtime Design

## 1. Design Decisions

Rython is a Rust implementation of a Python virtual machine. The first
implementation follows these decisions:

- Rust owns the VM, object heap, bytecode decoder, and execution loop.
- The Python frontend is not implemented in Rython.
- CPython is used as the frontend. Rython consumes CPython's compiled
  bytecode artifact.
- Python objects are stored in a VM-owned heap.
- Object references use handles (`ObjId`), not long-lived Rust references or
  raw pointers.
- Memory management uses a tracing garbage collector.
- The first collector is a stop-the-world, non-moving mark-and-sweep
  collector.
- The input format is tied to one exact CPython minor version. Bytecode from
  an unsupported version is rejected instead of being guessed at runtime.

The non-moving collector is an intentionally small first step. Handles keep
the object representation independent from physical addresses, so a
generational or compacting collector can be added later.

## 2. Scope

### In scope

- Reading a CPython `.pyc` file.
- Validating its version-dependent header.
- Decoding its `marshal` payload.
- Converting CPython code objects into Rython code objects.
- Executing the supported subset of CPython bytecode.
- Managing Python object graphs, including cycles.

### Out of scope for the first version

- A Python lexer, parser, AST, or compiler.
- CPython's C extension ABI.
- Compatibility with every CPython minor version.
- Full support for finalizers, weak references, generators, async execution,
  and all built-in types.

These features can be added after the bytecode pipeline and object model are
stable.

## 3. What the Frontend Produces

The term "Python bytecode file" hides three related objects:

| Layer | Name | Meaning |
| --- | --- | --- |
| In memory | `code object` / `PyCodeObject` | The result of CPython compilation |
| In the code object | `co_code` | The version-specific instruction bytes |
| On disk | `.pyc` | A header plus a `marshal`-serialized top-level code object |

The compilation pipeline is:

```text
Python source
    |
    v
CPython parser/compiler
    |
    v
code object (PyCodeObject)
    |
    v
marshal serialization
    |
    v
.pyc file
```

For example, CPython can generate a cache file with:

```bash
python3.12 -m py_compile example.py
```

The result is normally placed under `__pycache__`, with a name similar to
`example.cpython-312.pyc`.

Therefore, `.pyc` is not a stable, language-independent intermediate
representation and it is not just a raw `co_code` byte array. Rython must
decode the complete serialized code object, including constants, names,
function metadata, and exception metadata.

## 4. `.pyc` Input Contract

The Rython loader accepts a `.pyc` file produced by the supported CPython
minor version.

At a high level, the file contains:

```text
magic number (4 bytes)
flags      (4 bytes)
timestamp/source-size or source-hash (8 bytes)
marshal payload containing the top-level code object
```

The exact header and marshal representation are CPython-version-specific.
The loader must:

1. Read and validate the magic number.
2. Read the flags and consume the corresponding eight-byte header payload.
3. Decode the marshal stream.
4. Verify that the root value is a code object.
5. Recursively convert nested code objects found in the constants.
6. Return a descriptive error for malformed, truncated, or unsupported input.

The loader must parse serialized data rather than read the in-memory layout of
`PyCodeObject`. The latter contains CPython pointers and implementation details
that are not a stable file format or ABI.

`marshal` is an internal CPython serialization format. Its representation can
change between Python versions, and it must not be treated as a general
purpose persistence format. The Rython implementation should have a
version-specific marshal reader, with the supported version visible in the
module name or configuration.

The marshal reader should maintain its own reference table because marshal
streams can encode shared objects. It should also enforce limits on nesting
depth, container sizes, and total input size so that an untrusted `.pyc` file
cannot cause unbounded resource use.

### Current implementation

The first implementation is fixed to CPython 3.12's magic number
`cb 0d 0d 0a`. Its public library entry points are:

```rust
parse_header(bytes) -> Result<(PycHeader, usize)>
parse_pyc(bytes) -> Result<PycFile>
parse_marshal(bytes) -> Result<MarshalValue>
parse_code_object(bytes) -> Result<CodeObject>
```

The implementation currently lives in:

```text
src/pyc.rs      .pyc header validation and top-level loading
src/marshal.rs  CPython 3.12 marshal tags, references, and limits
src/code.rs     structured CodeObject representation
src/bytecode.rs CPython 3.12 wordcode decoder
src/value.rs    temporary owned VM value representation
src/vm.rs       frame, stack, name, operation, and dispatch logic
```

The parser resolves marshal references into an owned value tree. Shared
objects are decoded correctly, while recursive marshal containers are rejected
by this first owned representation. That restriction does not affect ordinary
CPython code-object constants, which are acyclic; a later arena-based parser
can preserve arbitrary marshal object graphs if needed.

## 5. Code Object Conversion

The parser should first create an owned, parser-level representation. The VM
must not retain references into the input buffer:

```rust
struct RawCode {
    arg_count: u32,
    positional_only_arg_count: u32,
    keyword_only_arg_count: u32,
    stack_size: u32,
    flags: u32,
    bytecode: Vec<u8>,
    constants: Vec<RawValue>,
    names: Vec<String>,
    localsplus_names: Vec<String>,
    localsplus_kinds: Vec<u8>,
    exception_table: Vec<u8>,
    filename: String,
    name: String,
    qualified_name: String,
}
```

`RawCode` is a semantic representation. Its fields may be populated from
different CPython code-object fields depending on the selected Python
version. In particular, the exact locals-plus representation and exception
table format must be handled by the version-specific decoder.

The conversion pipeline is:

```text
PycReader
    -> MarshalReader
    -> RawCode tree
    -> VmCode tree / heap objects
    -> initial frame
    -> bytecode dispatch loop
```

Nested function bodies are normally code objects inside `co_consts`. They
should be converted recursively. A function object is created later by the
corresponding bytecode instruction and captures its globals and closure
environment at runtime.

The initial `VmCode` should preserve at least:

- argument and local-variable metadata;
- stack size and code flags;
- instruction bytes;
- constants;
- names and local/free/cell variable names;
- filename, function name, and qualified name;
- exception-handling metadata.

Line information can initially be retained for diagnostics and traceback
support without affecting execution.

## 6. Bytecode Decoder and Initial VM

CPython 3.12 uses two-byte wordcode instructions:

```text
byte 0: opcode
byte 1: argument low byte
```

`EXTENDED_ARG` prefixes are folded into the following instruction. `CACHE`
entries remain in the decoded stream with their original offsets, and the VM
executes them as no-ops. Keeping cache entries and offsets makes diagnostics
and jump targets correspond to CPython's `co_code`.

The current decoder exposes:

```rust
decode(bytes) -> BytecodeResult<Vec<Instruction>>
decode_code(code) -> BytecodeResult<Vec<Instruction>>
```

The initial dispatch loop supports:

- `RESUME`, `CACHE`, `NOP`, `POP_TOP`, and `RETURN_CONST`;
- `LOAD_CONST`, `LOAD_NAME`, `STORE_NAME`, `LOAD_GLOBAL`;
- `LOAD_FAST`, `STORE_FAST`, and `RETURN_VALUE`;
- integer and floating-point arithmetic through `BINARY_OP`;
- string, bytes, list, and tuple concatenation/repetition;
- `COMPARE_OP`, unary numeric operations, and truthiness;
- tuple/list/set/dictionary construction;
- `BINARY_SUBSCR` and basic list/dictionary item assignment;
- forward and backward conditional/unconditional jumps.

The public `VmValue` is an owned value tree for this bootstrap VM. It is
deliberately separate from the planned handle-based heap. Mutable container
assignment currently updates equal global container values after a load; this
is sufficient for the first instruction tests but is not full Python object
identity or aliasing semantics. The tracing heap must replace this behavior
before general Python programs are considered compatible.

Function creation/calls, closures, attribute access, exceptions, iterators,
and Python's object protocol are not supported by this dispatch loop yet.

## 7. VM Values and Object Handles

Python values are represented as immediate values where practical and heap
handles for objects:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ObjId {
    index: u32,
    generation: u32,
}

enum Value {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Obj(ObjId),
}
```

The exact integer representation can be extended later for arbitrary-precision
integers. Strings, tuples, lists, dictionaries, functions, code objects,
modules, classes, instances, cells, and exceptions are heap objects.

The heap owns all object storage:

```rust
struct HeapSlot {
    generation: u32,
    marked: bool,
    object: Option<Object>,
}

struct Heap {
    slots: Vec<HeapSlot>,
    free_list: Vec<u32>,
}
```

Object fields contain `Value` or `ObjId`, never a long-lived `&Object`,
`Rc<RefCell<Object>>`, or raw pointer. A generation number makes a stale
handle detectable after a slot is reclaimed and reused.

Short-lived borrows returned by `Heap::get` and `Heap::get_mut` are allowed,
but they must not escape the operation that owns the borrow. Code that may
allocate must not keep a mutable object borrow across the allocation.

This design keeps unsafe code out of the normal VM and confines any future
pointer optimization to the heap implementation.

## 8. Tracing Garbage Collection

The selected collector is tracing mark-and-sweep:

```text
roots
  -> mark all reachable handles
  -> sweep unmarked heap slots
  -> recycle their indices
```

A minimal tracing interface is:

```rust
trait Trace {
    fn trace(&self, visit: &mut dyn FnMut(ObjId));
}
```

Every heap object that can reference another object must implement `Trace`.
For example, a list traces each element, a dictionary traces keys and values,
and a function traces its code object, globals, and closure.

The root set must include all VM-owned references that can keep an object
alive:

- the operand stack;
- local variables in every live frame;
- the current frame and instruction state;
- globals and builtins;
- the current exception and traceback state;
- pending call arguments and temporary values;
- native/runtime roots registered by host code.

The implementation needs an explicit temporary-root mechanism for operations
that allocate. A value held only in a Rust local must be registered as a root
before an allocation can trigger collection.

The collector must handle cycles without special cases:

```python
a = []
a.append(a)
del a
```

After `a` is removed from the roots, the list is unmarked and can be swept.
The same applies to two or more objects that reference one another.

The first version does not need immediate destruction semantics. Support for
`__del__`, weak references, resurrection, and finalization ordering should be
specified separately because they complicate collection and error handling.

## 9. Runtime Modules

The initial source layout is expected to follow these ownership boundaries:

```text
src/
  pyc.rs       .pyc header and marshal decoding
  bytecode.rs  version-specific instruction definitions and decoding
  value.rs     temporary owned VM values
  vm.rs        frames, operand stack, and dispatch
  object/      future handle-based object layouts
  gc/          future heap, roots, tracing, and sweeping
```

The bytecode module must be version-specific. It should not silently reuse an
opcode table from another CPython minor version.

## 10. Implementation Order

1. Pin the first supported CPython minor version and record its magic number
   and opcode format.
2. Implement a bounded reader for the `.pyc` header.
3. Implement the version-specific marshal reader and its reference table.
4. Parse and recursively convert code objects.
5. Implement `Value`, handles, heap allocation, and generation checks.
6. Implement mark-and-sweep with explicit VM roots.
7. Implement frames, operand stack, and a small bytecode dispatch loop.
8. Add calls, closures, exceptions, containers, and built-ins incrementally.
9. Add compatibility tests generated by the pinned CPython interpreter.

## 11. Required Tests

### Loader tests

- valid timestamp-based and hash-based headers;
- wrong magic number;
- truncated header and marshal payload;
- malformed type tags;
- invalid reference-table entries;
- nested code objects in constants;
- unsupported CPython version;
- resource-limit failures.

### GC tests

- an unreachable self-cycle is collected;
- two unreachable objects referencing each other are collected;
- reachable cycles remain alive;
- a temporary root survives an allocation-triggered collection;
- stale handles fail after slot reuse.

### VM tests

- decode wordcode, inline caches, and extended arguments;
- execute a minimal top-level module;
- load constants, names, and locals;
- execute arithmetic, comparisons, and unary operations;
- construct and mutate simple containers;
- execute conditional and backward jumps;
- report an unsupported opcode with filename and instruction offset.

The reference result for bytecode fixtures should be produced by the pinned
CPython version. `.pyc` files generated by another minor version must not be
used as test fixtures.

## 12. Summary

The artifact Rython consumes is a CPython `.pyc` file. Its meaningful payload
is a `marshal`-serialized `code object`; the actual instruction stream is the
code object's `co_code` field. Rython therefore implements a
version-specific `.pyc` and marshal parser, then executes the resulting code
object tree.

The runtime uses a heap plus handles and a tracing mark-and-sweep collector.
This avoids representing Python's cyclic object graph with Rust ownership
references, while preserving a path toward generational collection and other
future optimizations.
