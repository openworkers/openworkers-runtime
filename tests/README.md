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

Verifies that `TimeoutGuard` can terminate synchronous infinite loops using V8's `IsolateHandle::terminate_execution()`.

**What it tests**: Worker with `while(true)` loop is terminated after 100ms (wall-clock mode)

**How it works**:
- `TimeoutGuard` spawns a watchdog thread
- Watchdog waits for timeout using `mpsc::recv_timeout()`
- On timeout, calls `isolate_handle.terminate_execution()`
- Worker stops executing within ~100ms

### 4. `test_cpu_time_ignores_sleep`

**NEW**: Verifies that CPU time measurement correctly ignores async operations like `setTimeout`.

**What it tests**: Worker sleeps 100ms with 10ms CPU limit → succeeds because sleep doesn't count as CPU time

**Why this matters**: Protection against DDoS attacks using sleeps to hold workers
- ❌ **Wall-clock mode**: `await sleep(1000)` counts as 1000ms → attacker can block workers
- ✅ **CPU time mode**: `await sleep(1000)` counts as ~0ms → only real computation matters

## What Works ✅

✅ Heap limits configured via `v8::CreateParams::heap_limits()`
✅ **Synchronous infinite loop termination** via `TimeoutGuard` + `IsolateHandle::terminate_execution()`
✅ **CPU time measurement** using `clock_gettime(CLOCK_THREAD_CPUTIME_ID)` on Unix, `GetThreadTimes` on Windows
✅ **CPU vs Wall-clock mode selection** via `TimeLimitMode` enum
✅ CPU time correctly ignores sleeps, I/O, and async waits
✅ Watchdog thread pattern (inspired by Deno)
✅ RAII guard automatically cancels watchdog on normal completion
✅ Normal execution succeeds without interference

## Limitations ⚠️

⚠️ **CPU time limits NOT enforced yet** - only measured and logged (signal-based enforcement planned)
⚠️ **Wall-clock mode only** for termination - CPU time mode disables TimeoutGuard
⚠️ **Async operations** (setTimeout, fetch) are NOT interrupted by `terminate_execution()` - they run in tokio event loop
⚠️ No soft/hard limit distinction (single termination point)

**Why CPU time enforcement is complex**: Need signal-based approach (like edge-runtime) to interrupt from another thread. Current implementation measures CPU time but doesn't enforce it yet.

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

### CPU Timer Module

**File**: `src/cpu_timer.rs`

Cross-platform CPU time measurement:

```rust
// Get current thread's CPU time
pub fn get_thread_cpu_time() -> Option<Duration>

// RAII timer
pub struct CpuTimer {
    fn start() -> Self
    fn elapsed(&self) -> Duration
}
```

**Unix**: Uses `clock_gettime(CLOCK_THREAD_CPUTIME_ID)`
**Windows**: Uses `GetThreadTimes` (kernel + user time)

## Configuration

Defaults in `src/runtime.rs`:

```rust
pub enum TimeLimitMode {
    WallClock,  // Total elapsed time (including I/O, sleeps)
    CpuTime,    // Actual CPU execution time only (default)
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            heap_initial_mb: 1,                  // Initial heap
            heap_max_mb: 128,                    // Max heap (OOM if exceeded)
            max_execution_time_ms: 50,           // Execution timeout (0 = disabled)
            time_limit_mode: TimeLimitMode::CpuTime,  // CPU time for DDoS protection
        }
    }
}
```

## Inspiration

This implementation is inspired by:
- **Deno**: Watchdog thread pattern with `recv_timeout()`
- **Supabase edge-runtime**: Signal-based CPU time enforcement with POSIX timers + `SIGALRM`
- **Cloudflare workerd**: `enterJs()`/`exitJs()` RAII pattern for time measurement

## Future Improvements

- [ ] **CPU time enforcement** using signal-based approach (like edge-runtime):
  - POSIX timers with `timer_create(CLOCK_THREAD_CPUTIME_ID)`
  - `SIGALRM` handler to catch CPU time limit
  - `request_interrupt()` to safely terminate from signal handler
- [ ] Soft/hard limit distinction (warn before terminate)
- [ ] Metrics collection (execution time, termination reason)
- [ ] Per-request CPU time attribution (for Durable Objects style)
- [ ] Export CPU time metrics via trace/observability API
