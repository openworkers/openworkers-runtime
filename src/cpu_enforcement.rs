//! CPU time enforcement using POSIX timers and signal-based interruption.
//!
//! This module implements CPU time limits by:
//! 1. Creating a per-thread POSIX timer with `timer_create(CLOCK_THREAD_CPUTIME_ID)`
//! 2. Setting up a signal handler for SIGALRM
//! 3. Calling `IsolateHandle::terminate_execution()` from the signal handler
//!
//! NOTE: Currently only supported on Linux. macOS/BSD don't support timer_create.
//! On unsupported platforms, returns None and falls back to wall-clock enforcement.

#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "linux")]
use std::sync::Arc;

#[cfg(target_os = "linux")]
pub struct CpuEnforcer {
    timer_id: libc::timer_t,
    isolate_handle: deno_core::v8::IsolateHandle,
    terminated: Arc<AtomicBool>,
}

#[cfg(target_os = "linux")]
impl CpuEnforcer {
    /// Create a new CPU enforcer with the given timeout in milliseconds.
    ///
    /// Returns None if CPU enforcement is not available on this platform.
    pub fn new(isolate_handle: deno_core::v8::IsolateHandle, timeout_ms: u64) -> Option<Self> {
        if timeout_ms == 0 {
            return None;
        }

        // Create a POSIX timer that tracks thread CPU time
        let mut timer_id: libc::timer_t = std::ptr::null_mut();
        let terminated = Arc::new(AtomicBool::new(false));

        // Setup signal event
        let mut sigev: libc::sigevent = unsafe { std::mem::zeroed() };
        sigev.sigev_notify = libc::SIGEV_SIGNAL;
        sigev.sigev_signo = libc::SIGALRM;

        // Store pointer to terminated flag + isolate handle in signal value
        // We'll use a global registry instead of passing through sigval
        sigev.sigev_value.sival_ptr = std::ptr::null_mut();

        unsafe {
            let ret = libc::timer_create(libc::CLOCK_THREAD_CPUTIME_ID, &mut sigev, &mut timer_id);

            if ret != 0 {
                log::error!(
                    "Failed to create CPU timer: {}",
                    std::io::Error::last_os_error()
                );
                return None;
            }
        }

        // Register this enforcer globally so signal handler can find it
        register_enforcer(
            std::thread::current().id(),
            isolate_handle.clone(),
            terminated.clone(),
        );

        // Set the timer to expire after timeout_ms of CPU time
        let timeout_secs = timeout_ms / 1000;
        let timeout_nsecs = (timeout_ms % 1000) * 1_000_000;

        let mut timer_spec: libc::itimerspec = unsafe { std::mem::zeroed() };
        timer_spec.it_value.tv_sec = timeout_secs as i64;
        timer_spec.it_value.tv_nsec = timeout_nsecs as i64;

        unsafe {
            let ret = libc::timer_settime(timer_id, 0, &timer_spec, std::ptr::null_mut());

            if ret != 0 {
                log::error!(
                    "Failed to arm CPU timer: {}",
                    std::io::Error::last_os_error()
                );
                libc::timer_delete(timer_id);
                unregister_enforcer(std::thread::current().id());
                return None;
            }
        }

        log::debug!(
            "CPU enforcer created: {}ms CPU time limit on thread {:?}",
            timeout_ms,
            std::thread::current().id()
        );

        Some(Self {
            timer_id,
            isolate_handle,
            terminated,
        })
    }

    /// Check if the CPU limit was exceeded and termination occurred.
    #[allow(dead_code)]
    pub fn was_terminated(&self) -> bool {
        self.terminated.load(Ordering::Relaxed)
    }
}

#[cfg(target_os = "linux")]
impl Drop for CpuEnforcer {
    fn drop(&mut self) {
        // Delete the timer
        unsafe {
            libc::timer_delete(self.timer_id);
        }

        // Unregister from global registry
        unregister_enforcer(std::thread::current().id());

        log::debug!("CPU enforcer dropped");
    }
}

#[cfg(not(target_os = "linux"))]
pub struct CpuEnforcer;

#[cfg(not(target_os = "linux"))]
impl CpuEnforcer {
    pub fn new(_: deno_core::v8::IsolateHandle, _: u64) -> Option<Self> {
        // CPU enforcement not available on this platform (Linux-only)
        log::warn!("CPU time enforcement not available on this platform (Linux-only)");
        None
    }

    pub fn was_terminated(&self) -> bool {
        false
    }
}

// Global registry for mapping thread IDs to isolate handles
#[cfg(target_os = "linux")]
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::sync::Mutex;
#[cfg(target_os = "linux")]
use std::thread::ThreadId;

#[cfg(target_os = "linux")]
struct EnforcerRegistry {
    map: Mutex<HashMap<ThreadId, (deno_core::v8::IsolateHandle, Arc<AtomicBool>)>>,
}

#[cfg(target_os = "linux")]
static ENFORCER_REGISTRY: once_cell::sync::Lazy<EnforcerRegistry> =
    once_cell::sync::Lazy::new(|| {
        // Register signal handler on first use
        register_signal_handler();

        EnforcerRegistry {
            map: Mutex::new(HashMap::new()),
        }
    });

#[cfg(target_os = "linux")]
fn register_enforcer(
    thread_id: ThreadId,
    isolate_handle: deno_core::v8::IsolateHandle,
    terminated: Arc<AtomicBool>,
) {
    let mut map = ENFORCER_REGISTRY.map.lock().unwrap();
    map.insert(thread_id, (isolate_handle, terminated));
}

#[cfg(target_os = "linux")]
fn unregister_enforcer(thread_id: ThreadId) {
    let mut map = ENFORCER_REGISTRY.map.lock().unwrap();
    map.remove(&thread_id);
}

#[cfg(target_os = "linux")]
fn register_signal_handler() {
    use std::sync::Once;

    static SIGNAL_HANDLER_REGISTERED: Once = Once::new();

    SIGNAL_HANDLER_REGISTERED.call_once(|| {
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = sigalrm_handler as usize;
            sa.sa_flags = libc::SA_SIGINFO;

            libc::sigaction(libc::SIGALRM, &sa, std::ptr::null_mut());
        }

        log::debug!("SIGALRM handler registered for CPU enforcement");
    });
}

#[cfg(target_os = "linux")]
extern "C" fn sigalrm_handler(
    _sig: libc::c_int,
    _info: *mut libc::siginfo_t,
    _context: *mut libc::c_void,
) {
    // Get current thread ID
    let thread_id = std::thread::current().id();

    // Look up isolate handle in registry
    // Note: We can't safely lock a Mutex from a signal handler
    // This is a limitation - in production, edge-runtime uses a lock-free channel
    // For now, we'll try to lock with a timeout and skip if locked
    if let Ok(map) = ENFORCER_REGISTRY.map.try_lock() {
        if let Some((isolate_handle, terminated)) = map.get(&thread_id) {
            // Mark as terminated
            terminated.store(true, Ordering::Relaxed);

            // Request interrupt
            // This is safe to call from signal handler
            isolate_handle.terminate_execution();

            // Can't use log macros in signal handler (not async-signal-safe)
            // Would need to use write() syscall directly
        }
    }
}
