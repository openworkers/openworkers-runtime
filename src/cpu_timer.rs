use std::time::Duration;

/// Get current thread's CPU time (time actually spent executing on CPU).
///
/// This excludes time spent waiting for I/O, sleeping, or blocked on locks.
/// Uses `clock_gettime(CLOCK_THREAD_CPUTIME_ID)` on Unix and `GetThreadTimes` on Windows.
///
/// # Returns
///
/// CPU time as a Duration, or None if measurement failed.
pub fn get_thread_cpu_time() -> Option<Duration> {
    #[cfg(unix)]
    {
        get_thread_cpu_time_unix()
    }

    #[cfg(windows)]
    {
        get_thread_cpu_time_windows()
    }

    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

#[cfg(unix)]
fn get_thread_cpu_time_unix() -> Option<Duration> {
    use std::mem::MaybeUninit;

    let mut time = MaybeUninit::<libc::timespec>::uninit();

    // SAFETY: clock_gettime is safe to call with valid pointers
    let ret = unsafe {
        libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, time.as_mut_ptr())
    };

    if ret == 0 {
        let time = unsafe { time.assume_init() };

        // Convert timespec to Duration
        let secs = time.tv_sec as u64;
        let nanos = time.tv_nsec as u32;

        Some(Duration::new(secs, nanos))
    } else {
        None
    }
}

#[cfg(windows)]
fn get_thread_cpu_time_windows() -> Option<Duration> {
    use winapi::shared::minwindef::FILETIME;
    use winapi::um::processthreadsapi::{GetCurrentThread, GetThreadTimes};

    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();

    // SAFETY: GetThreadTimes is safe with valid pointers
    let ret = unsafe {
        GetThreadTimes(
            GetCurrentThread(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    };

    if ret != 0 {
        // Convert FILETIME to u64 (100-nanosecond intervals since 1601-01-01)
        let kernel_time = ((kernel.dwHighDateTime as u64) << 32) | (kernel.dwLowDateTime as u64);
        let user_time = ((user.dwHighDateTime as u64) << 32) | (user.dwLowDateTime as u64);

        // Total CPU time = kernel time + user time
        let total_100ns = kernel_time + user_time;

        // Convert to nanoseconds
        let total_ns = total_100ns * 100;

        Some(Duration::from_nanos(total_ns))
    } else {
        None
    }
}

/// RAII guard that measures CPU time spent in a block of code.
///
/// # Example
///
/// ```rust,ignore
/// let timer = CpuTimer::start();
/// // ... do work ...
/// let elapsed = timer.elapsed();
/// println!("CPU time: {:?}", elapsed);
/// ```
pub struct CpuTimer {
    start: Duration,
}

impl CpuTimer {
    /// Start measuring CPU time.
    pub fn start() -> Self {
        Self {
            start: get_thread_cpu_time().unwrap_or_default(),
        }
    }

    /// Get elapsed CPU time since timer was started.
    pub fn elapsed(&self) -> Duration {
        get_thread_cpu_time()
            .unwrap_or(self.start)
            .saturating_sub(self.start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_get_thread_cpu_time() {
        let time = get_thread_cpu_time();
        assert!(time.is_some(), "Should be able to get CPU time");

        let time = time.unwrap();
        assert!(time.as_nanos() > 0, "CPU time should be non-zero");
    }

    #[test]
    fn test_cpu_timer_measures_computation() {
        let timer = CpuTimer::start();

        // Do some CPU-intensive work
        let mut sum = 0u64;
        for i in 0..1_000_000 {
            sum = sum.wrapping_add(i);
        }

        let elapsed = timer.elapsed();

        // Prevent optimization
        assert!(sum > 0);

        // Should have measured some CPU time
        assert!(
            elapsed.as_micros() > 0,
            "Should measure CPU time for computation"
        );
    }

    #[test]
    fn test_cpu_timer_ignores_sleep() {
        let timer = CpuTimer::start();

        // Sleep doesn't consume CPU time
        thread::sleep(Duration::from_millis(10));

        let elapsed = timer.elapsed();

        // CPU time should be very small (< 1ms) despite 10ms sleep
        assert!(
            elapsed.as_millis() < 5,
            "Sleep should not count as CPU time, got {:?}",
            elapsed
        );
    }
}
