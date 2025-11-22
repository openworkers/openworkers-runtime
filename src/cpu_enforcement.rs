//! CPU time enforcement using POSIX timers and async-signal-safe communication.
//!
//! Architecture:
//! 1. Per-thread POSIX timer with `timer_create(CLOCK_THREAD_CPUTIME_ID)`
//! 2. signal-hook-tokio handles SIGALRM in async-signal-safe manner
//! 3. Dedicated thread processes signals and calls `terminate_execution()`
//!
//! The signal handler itself does NO locks, NO allocations - fully async-signal-safe.
//! All complex logic happens in the dedicated signal processing thread.
//!
//! NOTE: Linux-only. macOS/BSD lack timer_create, fallback to wall-clock enforcement.

#[cfg(target_os = "linux")]
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[cfg(target_os = "linux")]
use std::sync::{Arc, Mutex};

#[cfg(target_os = "linux")]
pub struct CpuEnforcer {
    timer_id: libc::timer_t,
    enforcer_id: usize,
    terminated: Arc<AtomicBool>,
}

#[cfg(target_os = "linux")]
impl CpuEnforcer {
    /// Create a new CPU enforcer with the given timeout in milliseconds.
    ///
    /// Returns None if CPU enforcement is not available or setup fails.
    pub fn new(isolate_handle: deno_core::v8::IsolateHandle, timeout_ms: u64) -> Option<Self> {
        if timeout_ms == 0 {
            return None;
        }

        // Generate unique enforcer ID
        static ENFORCER_COUNTER: AtomicUsize = AtomicUsize::new(1);
        let enforcer_id = ENFORCER_COUNTER.fetch_add(1, Ordering::Relaxed);

        // Create POSIX timer
        let mut timer_id: libc::timer_t = std::ptr::null_mut();
        let mut sigev: libc::sigevent = unsafe { std::mem::zeroed() };

        sigev.sigev_notify = libc::SIGEV_SIGNAL;
        sigev.sigev_signo = libc::SIGALRM;
        // Store enforcer_id in signal value (async-signal-safe - just an integer)
        sigev.sigev_value.sival_ptr = enforcer_id as *mut libc::c_void;

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

        let terminated = Arc::new(AtomicBool::new(false));

        // Register in global registry (signal processing thread will lookup here)
        register_enforcer(enforcer_id, isolate_handle, terminated.clone());

        // Arm the timer
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
                unregister_enforcer(enforcer_id);
                return None;
            }
        }

        log::debug!(
            "CPU enforcer #{} created: {}ms CPU time limit",
            enforcer_id,
            timeout_ms
        );

        Some(Self {
            timer_id,
            enforcer_id,
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
        unregister_enforcer(self.enforcer_id);

        log::debug!("CPU enforcer #{} dropped", self.enforcer_id);
    }
}

#[cfg(not(target_os = "linux"))]
pub struct CpuEnforcer;

#[cfg(not(target_os = "linux"))]
impl CpuEnforcer {
    pub fn new(_: deno_core::v8::IsolateHandle, _: u64) -> Option<Self> {
        None
    }

    #[allow(dead_code)]
    pub fn was_terminated(&self) -> bool {
        false
    }
}

// Global registry and signal processing thread (Linux-only)
#[cfg(target_os = "linux")]
struct EnforcerData {
    isolate_handle: deno_core::v8::IsolateHandle,
    terminated: Arc<AtomicBool>,
}

#[cfg(target_os = "linux")]
struct EnforcerRegistry {
    map: Mutex<HashMap<usize, EnforcerData>>,
}

#[cfg(target_os = "linux")]
static ENFORCER_REGISTRY: once_cell::sync::Lazy<EnforcerRegistry> =
    once_cell::sync::Lazy::new(|| {
        // Spawn signal processing thread on first use
        spawn_signal_handler_thread();

        EnforcerRegistry {
            map: Mutex::new(HashMap::new()),
        }
    });

#[cfg(target_os = "linux")]
fn register_enforcer(
    enforcer_id: usize,
    isolate_handle: deno_core::v8::IsolateHandle,
    terminated: Arc<AtomicBool>,
) {
    let mut map = ENFORCER_REGISTRY.map.lock().unwrap();
    map.insert(
        enforcer_id,
        EnforcerData {
            isolate_handle,
            terminated,
        },
    );
}

#[cfg(target_os = "linux")]
fn unregister_enforcer(enforcer_id: usize) {
    let mut map = ENFORCER_REGISTRY.map.lock().unwrap();
    map.remove(&enforcer_id);
}

#[cfg(target_os = "linux")]
fn spawn_signal_handler_thread() {
    use std::sync::Once;

    static SIGNAL_THREAD_SPAWNED: Once = Once::new();

    SIGNAL_THREAD_SPAWNED.call_once(|| {
        std::thread::Builder::new()
            .name("cpu-enforcer".into())
            .spawn(|| {
                signal_handler_thread();
            })
            .expect("Failed to spawn CPU enforcer signal handler thread");

        log::debug!("CPU enforcer signal handler thread spawned");
    });
}

#[cfg(target_os = "linux")]
fn signal_handler_thread() {
    use futures::StreamExt;
    use signal_hook::consts::signal;
    use signal_hook::iterator::exfiltrator::raw::WithRawSiginfo;
    use signal_hook_tokio::SignalsInfo;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime for signal handler");

    rt.block_on(async {
        // Setup async-signal-safe SIGALRM handler
        let mut signals = SignalsInfo::with_exfiltrator([signal::SIGALRM], WithRawSiginfo)
            .expect("Failed to register SIGALRM handler");

        log::debug!("CPU enforcer listening for SIGALRM signals");

        while let Some(siginfo) = signals.next().await {
            // Extract enforcer_id from signal value (async-signal-safe)
            let enforcer_id = unsafe { siginfo.si_value().sival_ptr as usize };

            log::debug!("SIGALRM received for enforcer #{}", enforcer_id);

            // Lookup enforcer in registry (safe here - we're in a dedicated thread)
            let data = {
                let map = ENFORCER_REGISTRY.map.lock().unwrap();
                map.get(&enforcer_id).cloned()
            };

            if let Some(EnforcerData {
                isolate_handle,
                terminated,
            }) = data
            {
                // Mark as terminated
                terminated.store(true, Ordering::Relaxed);

                // Terminate V8 execution
                isolate_handle.terminate_execution();

                log::warn!(
                    "CPU time limit exceeded for enforcer #{}, isolate terminated",
                    enforcer_id
                );
            } else {
                log::warn!("SIGALRM for unknown enforcer #{}", enforcer_id);
            }
        }
    });
}

// Clone impl for EnforcerData (needed for registry lookup)
#[cfg(target_os = "linux")]
impl Clone for EnforcerData {
    fn clone(&self) -> Self {
        Self {
            isolate_handle: self.isolate_handle.clone(),
            terminated: self.terminated.clone(),
        }
    }
}
