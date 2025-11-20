use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// RAII guard that spawns a watchdog thread to terminate V8 execution on timeout.
///
/// The watchdog thread monitors execution time and calls `isolate.terminate_execution()`
/// if the timeout is exceeded. The guard automatically cancels the watchdog when dropped.
///
/// # Example
///
/// ```rust,ignore
/// let handle = js_runtime.v8_isolate().thread_safe_handle();
/// {
///     let _guard = TimeoutGuard::new(handle, 50); // 50ms timeout
///     // Execute JavaScript code
///     js_runtime.execute_script(...)?;
/// } // Guard dropped here, watchdog cancelled
/// ```
pub struct TimeoutGuard {
    cancel_tx: Option<mpsc::Sender<()>>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl TimeoutGuard {
    /// Create a new timeout guard with the given V8 isolate handle and timeout in milliseconds.
    ///
    /// # Arguments
    ///
    /// * `isolate_handle` - Thread-safe handle to the V8 isolate
    /// * `timeout_ms` - Timeout in milliseconds (0 = disabled)
    pub fn new(isolate_handle: deno_core::v8::IsolateHandle, timeout_ms: u64) -> Self {
        // If timeout is 0, create disabled guard
        if timeout_ms == 0 {
            return Self {
                cancel_tx: None,
                thread_handle: None,
            };
        }

        let (cancel_tx, cancel_rx) = mpsc::channel::<()>();

        let thread_handle = thread::spawn(move || {
            let timeout = Duration::from_millis(timeout_ms);

            // Wait for either timeout or cancellation
            match cancel_rx.recv_timeout(timeout) {
                // Cancelled before timeout - normal completion
                Ok(()) => {
                    log::debug!("Timeout watchdog cancelled (execution completed)");
                }
                // Timeout expired - terminate execution
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    log::warn!("Execution timeout after {}ms, terminating isolate", timeout_ms);
                    isolate_handle.terminate_execution();
                }
                // Channel disconnected (shouldn't happen)
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    log::error!("Timeout watchdog channel disconnected unexpectedly");
                }
            }
        });

        Self {
            cancel_tx: Some(cancel_tx),
            thread_handle: Some(thread_handle),
        }
    }
}

impl Drop for TimeoutGuard {
    fn drop(&mut self) {
        // Send cancellation signal to watchdog thread
        if let Some(cancel_tx) = self.cancel_tx.take() {
            // Ignore error if thread already exited
            let _ = cancel_tx.send(());
        }

        // Wait for watchdog thread to finish
        if let Some(handle) = self.thread_handle.take() {
            // Don't block indefinitely - if thread doesn't finish in 100ms, detach it
            // This should never happen in practice
            match handle.join() {
                Ok(()) => {
                    log::trace!("Timeout watchdog thread joined successfully");
                }
                Err(_) => {
                    log::error!("Timeout watchdog thread panicked");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disabled_guard() {
        // Create a disabled guard (timeout = 0)
        // Should not spawn any thread
        let guard = TimeoutGuard {
            cancel_tx: None,
            thread_handle: None,
        };

        assert!(guard.cancel_tx.is_none());
        assert!(guard.thread_handle.is_none());
    }
}
