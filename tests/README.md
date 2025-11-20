# Resource Limits Tests

Simple unit tests for V8 heap limits and execution timeouts.

## Running Tests

```bash
# Run all resource limit tests
cargo test --test resource_limits

# Run with output
cargo test --test resource_limits -- --nocapture

# Run specific test
cargo test --test resource_limits test_heap_limits_configured
```

## Tests

### 1. `test_heap_limits_configured`

Verifies that custom heap limits can be set via `RuntimeLimits`.

**What it tests**: Worker creation with custom heap limits (1MB → 64MB)

### 2. `test_timeout_wrapper_works`

Verifies that `tokio::time::timeout()` wrapper works for async operations.

**What it tests**: Worker that sleeps 500ms times out at 100ms

**Note**: This works for async operations (setTimeout, fetch, etc) but **NOT** for synchronous infinite loops. To interrupt sync loops, V8's `Isolate::TerminateExecution()` would be needed.

### 3. `test_normal_execution_works`

Verifies that normal workers execute successfully without hitting limits.

**What it tests**: Simple "Hello World" worker completes normally

## What Works

✅ Heap limits configured via `v8::CreateParams::heap_limits()`
✅ Timeout for async operations (setTimeout, fetch)
✅ Normal execution succeeds

## What Doesn't Work (Yet)

❌ Interrupting synchronous infinite loops
❌ Detecting actual OOM crashes (worker just terminates)
❌ CPU time measurement (only wall-clock time)

To properly interrupt sync loops, we'd need:
- Access to `v8::Isolate` handle
- Call `isolate.TerminateExecution()` from another thread
- More complex runtime architecture

## Configuration

Defaults in `src/runtime.rs`:

```rust
impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            heap_initial_mb: 1,    // Initial heap
            heap_max_mb: 128,      // Max heap (OOM if exceeded)
        }
    }
}
```

Timeout in `event_fetch.rs`:

```rust
const FETCH_TIMEOUT_MS: u64 = 50;  // Wall-clock timeout
```
