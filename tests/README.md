# Resource Limits Tests

Simple unit tests for V8 heap limits and execution timeouts with isolate termination.

## Running Tests

```bash
# Run all resource limit tests
cargo test --test resource_limits

# Run with output
cargo test --test resource_limits -- --nocapture

# Run specific test
cargo test --test resource_limits test_synchronous_infinite_loop_termination
```

## Tests

### 1. `test_heap_limits_configured`

Verifies that custom heap limits can be set via `RuntimeLimits`.

**What it tests**: Worker creation with custom heap limits (1MB → 64MB)

### 2. `test_normal_execution_works`

Verifies that normal workers execute successfully without hitting limits.

**What it tests**: Simple "Hello World" worker completes normally with default 50ms timeout

### 3. `test_synchronous_infinite_loop_termination`

**NEW**: Verifies that `TimeoutGuard` can terminate synchronous infinite loops using V8's `IsolateHandle::terminate_execution()`.

**What it tests**: Worker with `while(true)` loop is terminated after 100ms

**How it works**:
- `TimeoutGuard` spawns a watchdog thread
- Watchdog waits for timeout using `mpsc::recv_timeout()`
- On timeout, calls `isolate_handle.terminate_execution()`
- Worker stops executing within ~100ms

## What Works ✅

✅ Heap limits configured via `v8::CreateParams::heap_limits()`
✅ **Synchronous infinite loop termination** via `TimeoutGuard` + `IsolateHandle::terminate_execution()`
✅ Watchdog thread pattern (inspired by Deno)
✅ RAII guard automatically cancels watchdog on normal completion
✅ Normal execution succeeds without interference

## Limitations ⚠️

⚠️ **Async operations** (setTimeout, fetch) are **NOT** interrupted by `terminate_execution()` because they run in tokio event loop, not V8
⚠️ CPU time measurement not implemented (only wall-clock time)
⚠️ No soft/hard limit distinction (single termination point)

To handle async timeouts, use `tokio::time::timeout()` wrapper in the runner layer.

## Architecture

### TimeoutGuard (RAII pattern)

```rust
// In src/timeout.rs
pub struct TimeoutGuard {
    cancel_tx: Option<mpsc::Sender<()>>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl TimeoutGuard {
    pub fn new(isolate_handle: v8::IsolateHandle, timeout_ms: u64) -> Self {
        // Spawns watchdog thread
        // Thread waits for timeout using recv_timeout()
        // On timeout, calls isolate_handle.terminate_execution()
    }
}

impl Drop for TimeoutGuard {
    fn drop(&mut self) {
        // Sends cancellation signal
        // Waits for thread to finish
    }
}
```

### Usage in Worker

```rust
// In src/runtime.rs
pub async fn exec(&mut self, mut task: Task) -> Result<(), CoreError> {
    // Start watchdog before execution
    let _guard = TimeoutGuard::new(
        self.isolate_handle.clone(),
        self.limits.max_execution_time_ms,
    );

    crate::util::exec_task(self, &mut task);
    self.js_runtime.run_event_loop(opts).await
    // Guard dropped here, watchdog cancelled
}
```

## Configuration

Defaults in `src/runtime.rs`:

```rust
impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            heap_initial_mb: 1,                // Initial heap
            heap_max_mb: 128,                  // Max heap (OOM if exceeded)
            max_execution_time_ms: 50,         // Execution timeout (0 = disabled)
        }
    }
}
```

## Inspiration

This implementation is inspired by:
- **Deno**: Watchdog thread pattern with `recv_timeout()`
- **Supabase edge-runtime**: `IsolateHandle::request_interrupt()` pattern (we use simpler direct `terminate_execution()` call)

## Future Improvements

- [ ] CPU time tracking (using `getrusage()` on Linux/Mac, `GetProcessTimes` on Windows)
- [ ] Soft/hard limit distinction (warn before terminate)
- [ ] Metrics collection (execution time, termination reason)
- [ ] POSIX timers for more accurate CPU-only measurement (advanced)
