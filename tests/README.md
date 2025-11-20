# Resource Limits Tests

Simple unit tests for V8 heap limits and execution timeouts with isolate termination.

## Running Tests

```bash
# Run all resource limit tests
cargo test --test resource_limits

# Run with output
cargo test --test resource_limits -- --nocapture

# Run specific test
cargo test --test resource_limits test_cpu_intensive_computation_termination
```

### Platform-Specific Behavior

**Linux**: Full CPU time enforcement via POSIX timers + SIGALRM
**macOS/BSD**: Falls back to wall-clock enforcement (CPU measurement works, enforcement doesn't)
**Windows**: CPU measurement works, enforcement not implemented yet

To test real CPU enforcement on Linux:

```bash
# Using Docker
docker run --rm -v $(pwd):/workspace -w /workspace rust:latest \
  cargo test --test resource_limits -- --nocapture
```

## Tests

### 1. `test_heap_limits_configured`

Verifies that custom heap limits can be set via `RuntimeLimits`.

**What it tests**: Worker creation with custom heap limits (1MB → 64MB)

### 2. `test_normal_execution_works`

Verifies that normal workers execute successfully without hitting limits.

**What it tests**: Simple "Hello World" worker completes normally with default 50ms timeout

### 3. `test_cpu_intensive_computation_termination`

**NEW**: Verifies that CPU-intensive computation is terminated when exceeding CPU time limit.

**What it tests**: Worker doing 100M iterations of `Math.sqrt()` with 50ms CPU limit → terminates quickly

**On Linux**: Uses POSIX timer + SIGALRM for real CPU time enforcement
**On macOS/others**: Falls back to wall-clock enforcement (still works, just different mechanism)

### 4. `test_cpu_time_ignores_sleep`

**NEW**: Verifies that CPU time measurement correctly ignores async operations like `setTimeout`.

**What it tests**: Worker sleeps 100ms with 10ms CPU limit → succeeds on Linux because sleep doesn't count as CPU time

**Why this matters**: Protection against DDoS attacks using sleeps to hold workers
- ❌ **Wall-clock mode**: `await sleep(1000)` counts as 1000ms → attacker can block workers
- ✅ **CPU time mode (Linux)**: `await sleep(1000)` counts as ~0ms → only real computation matters

**Platform note**: On macOS, this test demonstrates fallback to wall-clock enforcement

## What Works ✅

✅ Heap limits configured via `v8::CreateParams::heap_limits()`
✅ **Dual limit system**: CPU time (50ms) + Wall-clock time (30s) enforced simultaneously
✅ **CPU time measurement** using `clock_gettime(CLOCK_THREAD_CPUTIME_ID)` on Unix
✅ **CPU time ENFORCEMENT on Linux** using POSIX timers (`timer_create`) + SIGALRM signal handler
✅ **Wall-clock enforcement** on all platforms via watchdog thread
✅ CPU time correctly ignores sleeps, I/O, and async waits
✅ Protection against DDoS (CPU limit) AND hanging I/O (wall-clock limit)
✅ RAII guards automatically cancel on normal completion
✅ Normal execution succeeds without interference
✅ Automatic fallback: Linux uses both limits, macOS uses wall-clock only

## Platform Support

| Platform | CPU Measurement | CPU Enforcement | Wall-Clock Enforcement |
|----------|----------------|-----------------|----------------------|
| **Linux** | ✅ `clock_gettime` | ✅ POSIX timers (50ms) | ✅ Watchdog thread (30s) |
| **macOS** | ✅ `clock_gettime` | ❌ No enforcement | ✅ Watchdog thread (30s) |
| **Windows** | ❌ No support | ❌ No enforcement | ✅ Watchdog thread (30s) |

**Production deployment**: Use Linux for full CPU enforcement
**Local development**: macOS/Windows work fine with wall-clock protection

## Limitations ⚠️

⚠️ **Async operations** (setTimeout, fetch) are NOT interrupted by `terminate_execution()` - they run in tokio event loop
⚠️ **Wall-clock limit catches these** - if `await fetch()` takes > 30s, terminated
⚠️ No soft/hard limit distinction (single termination point)
⚠️ CPU enforcement Linux-only (macOS/BSD lack `timer_create`, Windows not implemented)

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

### CpuEnforcer (Signal-based enforcement, Linux-only)

**File**: `src/cpu_enforcement.rs`

POSIX timer-based CPU enforcement:

```rust
pub struct CpuEnforcer {
    timer_id: libc::timer_t,
    isolate_handle: v8::IsolateHandle,
    terminated: Arc<AtomicBool>,
}

impl CpuEnforcer {
    pub fn new(isolate_handle: v8::IsolateHandle, timeout_ms: u64) -> Option<Self> {
        // 1. Create POSIX timer with timer_create(CLOCK_THREAD_CPUTIME_ID)
        // 2. Register signal handler for SIGALRM
        // 3. Arm timer with timer_settime()
        // 4. Signal handler calls isolate.terminate_execution() on timeout
    }
}

impl Drop for CpuEnforcer {
    fn drop(&mut self) {
        // Delete timer and unregister from global registry
        libc::timer_delete(self.timer_id);
    }
}
```

**How it works**:
1. **POSIX timer** created per worker with `CLOCK_THREAD_CPUTIME_ID` (tracks CPU time only)
2. Timer configured to fire SIGALRM after N milliseconds of **actual CPU usage**
3. **signal-hook-tokio** handles SIGALRM in fully async-signal-safe manner (no locks, no allocations)
4. **Dedicated thread** receives signal via async stream, does registry lookup
5. Thread calls `isolate_handle.terminate_execution()` to stop worker
6. **Automatic cleanup** on drop - deletes timer and unregisters

**Async-signal-safe implementation**:
- Signal handler (managed by signal-hook) does NO locks, NO allocations
- Only forwards signal to async stream (using pipe internally - async-signal-safe)
- Dedicated thread "cpu-enforcer" processes signals safely
- All Mutex locks, HashMap lookups happen outside signal context
- Production-grade, no risk of deadlock

**Why Linux-only**: macOS/BSD don't support `timer_create()` - they use different APIs (kqueue/dispatch)

**Dependencies**:
- `signal-hook` - Async-signal-safe signal handling
- `signal-hook-tokio` - Async stream of signals for tokio
- `libc` - POSIX timer syscalls

## Configuration

Defaults in `src/runtime.rs`:

```rust
impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            heap_initial_mb: 1,              // Initial heap
            heap_max_mb: 128,                // Max heap (OOM if exceeded)
            max_cpu_time_ms: 50,             // 50ms CPU limit (anti-DDoS)
            max_wall_clock_time_ms: 30_000,  // 30s wall-clock limit (anti-hang)
        }
    }
}
```

### Dual Limit System

Both limits are enforced **simultaneously**. Whichever is hit first terminates execution:

**CPU time limit (50ms)**:
- Protection against DDoS via CPU-intensive loops
- Only counts actual computation (sleeps/I/O ignored)
- Linux: Enforced via POSIX timers
- macOS/Windows: Not enforced (measurement only)

**Wall-clock limit (30s)**:
- Protection against hanging on slow I/O (fetch, DB queries)
- Counts total elapsed time (computation + I/O + sleeps)
- All platforms: Enforced via watchdog thread

**Example scenarios**:
```javascript
// Hits CPU limit (50ms) first
while (true) { Math.sqrt(42); } // Terminated at 50ms

// Hits wall-clock limit (30s) first
await fetch('http://slow-api.com'); // Terminated at 30s

// Both within limits
await sleep(100); // OK: 0ms CPU, 100ms wall-clock
```

## Future Improvements

- [ ] **macOS/Windows CPU enforcement**: Implement platform-specific approaches
  - macOS: Grand Central Dispatch (dispatch_source) or kqueue
  - Windows: Waitable timers or thread-based monitoring
- [ ] **Async operation interruption**: Currently async ops (setTimeout, fetch) aren't interrupted
  - Would need tokio runtime integration or custom event loop
- [ ] Soft/hard limit distinction (warn before terminate)
- [ ] Metrics collection (execution time, termination reason)
- [ ] Per-request CPU time attribution (for Durable Objects style)
- [ ] Export CPU time metrics via trace/observability API
