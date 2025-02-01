# deno_ops

`proc_macro` for generating highly optimized V8 functions from Deno ops.

```rust
// Declare an op.
#[op]
pub fn op_add(_: &mut OpState, a: i32, b: i32) -> i32 {
  a + b
}

// Register with an extension.
Extension::builder()
  .ops(vec![op_add::decl()])
  .build();
```

### Wasm calls

The `#[op(wasm)]` attribute should be used for calls expected to be called from
Wasm. This allows seamless `WasmMemory` integration for calls.

```rust
#[op(wasm)]
pub fn op_args_get(
  offset: i32,
  buffer_offset: i32,
  memory: Option<&[u8]>, // Must be last parameter. Some(..) when entered from Wasm.
) {
  // ...
}
```
