use openworkers_core::{HttpRequest, RuntimeLimits, Script, Task};
use openworkers_runtime_deno::Worker;

// Wall-clock timeout tests are Linux-only because they spin CPU waiting for timeout.
// On macOS without CPU enforcement, these tests would burn CPU unnecessarily.

#[cfg(target_os = "linux")]
#[tokio::test]
#[ntest::timeout(3000)] // 3s max - test should complete in ~500ms
async fn test_wall_clock_timeout_infinite_loop() {
    let limits = RuntimeLimits {
        heap_initial_mb: 16,
        heap_max_mb: 64,
        max_cpu_time_ms: 0, // Disabled
        max_wall_clock_time_ms: 500,
    };

    let code = r#"
        addEventListener('fetch', (event) => {
            while (true) {
                // Busy loop
            }
            event.respondWith(new Response('Should never reach here'));
        });
    "#;

    let script = Script::new(code);

    let mut worker = Worker::new(script, None, Some(limits)).await.unwrap();

    let req = HttpRequest {
        method: "GET".to_string(),
        url: "http://localhost/".to_string(),
        headers: Default::default(),
        body: None,
    };

    let (task, _rx) = Task::fetch(req);
    let result = worker.exec(task).await;

    // Should terminate due to wall-clock timeout
    assert_eq!(
        result,
        Err(TerminationReason::WallClockTimeout),
        "Expected Err(TerminationReason::WallClockTimeout), got: {:?}",
        result
    );
}

#[cfg(target_os = "linux")]
#[tokio::test]
#[ntest::timeout(3000)] // 3s max - test should complete in ~500ms
async fn test_wall_clock_timeout_async_loop() {
    let limits = RuntimeLimits {
        heap_initial_mb: 16,
        heap_max_mb: 64,
        max_cpu_time_ms: 0,
        max_wall_clock_time_ms: 500,
    };

    // Long-running async operation - should be terminated by wall-clock timeout
    let code = r#"
        addEventListener('fetch', async (event) => {
            // Sleep for 10 seconds (way longer than our 500ms timeout)
            await new Promise(resolve => setTimeout(resolve, 10000));
            event.respondWith(new Response('Should never reach here'));
        });
    "#;

    let script = Script::new(code);

    let mut worker = Worker::new(script, None, Some(limits)).await.unwrap();

    let req = HttpRequest {
        method: "GET".to_string(),
        url: "http://localhost/".to_string(),
        headers: Default::default(),
        body: None,
    };

    let (task, _rx) = Task::fetch(req);
    let result = worker.exec(task).await;

    // Should terminate due to wall-clock timeout
    assert_eq!(
        result,
        Err(TerminationReason::WallClockTimeout),
        "Expected Err(TerminationReason::WallClockTimeout), got: {:?}",
        result
    );
}

#[tokio::test]
async fn test_fast_execution_no_timeout() {
    let limits = RuntimeLimits {
        heap_initial_mb: 16,
        heap_max_mb: 64,
        max_cpu_time_ms: 0,
        max_wall_clock_time_ms: 5000,
    };

    let code = r#"
        addEventListener('fetch', (event) => {
            let sum = 0;
            for (let i = 0; i < 1000; i++) {
                sum += i;
            }
            event.respondWith(new Response('Sum: ' + sum, { status: 200 }));
        });
    "#;

    let script = Script::new(code);

    let mut worker = Worker::new(script, None, Some(limits)).await.unwrap();

    let req = HttpRequest {
        method: "GET".to_string(),
        url: "http://localhost/".to_string(),
        headers: Default::default(),
        body: None,
    };

    let (task, rx) = Task::fetch(req);
    let result = worker.exec(task).await;

    assert!(result.is_ok(), "Expected Ok(), got: {:?}", result);

    let response = rx.await.unwrap();
    assert_eq!(response.status, 200);
}

#[tokio::test]
async fn test_disabled_timeout_allows_long_execution() {
    let limits = RuntimeLimits {
        heap_initial_mb: 16,
        heap_max_mb: 64,
        max_cpu_time_ms: 0,
        max_wall_clock_time_ms: 0, // Disabled
    };

    let code = r#"
        addEventListener('fetch', (event) => {
            let sum = 0;
            for (let i = 0; i < 100000; i++) {
                sum += Math.sqrt(i);
            }
            event.respondWith(new Response('Completed: ' + sum.toFixed(2), { status: 200 }));
        });
    "#;

    let script = Script::new(code);

    let mut worker = Worker::new(script, None, Some(limits)).await.unwrap();

    let req = HttpRequest {
        method: "GET".to_string(),
        url: "http://localhost/".to_string(),
        headers: Default::default(),
        body: None,
    };

    let (task, rx) = Task::fetch(req);
    let result = worker.exec(task).await;

    assert!(result.is_ok(), "Expected Ok(), got: {:?}", result);

    let response = rx.await.unwrap();
    assert_eq!(response.status, 200);
}

// CPU time tests (Linux only)
#[cfg(target_os = "linux")]
mod cpu_tests {
    use super::*;

    #[tokio::test]
    #[ntest::timeout(3000)] // 3s max - test should complete in ~500ms
    async fn test_cpu_time_limit_infinite_loop() {
        let limits = RuntimeLimits {
            heap_initial_mb: 16,
            heap_max_mb: 64,
            max_cpu_time_ms: 500,
            max_wall_clock_time_ms: 0, // Disabled
        };

        let code = r#"
            addEventListener('fetch', (event) => {
                while (true) {
                    Math.sqrt(Math.random());
                }
                event.respondWith(new Response('Should never reach here'));
            });
        "#;

        let script = Script::new(code);

        let mut worker = Worker::new(script, None, Some(limits)).await.unwrap();

        let req = HttpRequest {
            method: "GET".to_string(),
            url: "http://localhost/".to_string(),
            headers: Default::default(),
            body: None,
        };

        let (task, _rx) = Task::fetch(req);
        let result = worker.exec(task).await;

        // Should terminate due to CPU time limit
        assert_eq!(
            result,
            Err(TerminationReason::CpuTimeLimit),
            "Expected Err(TerminationReason::CpuTimeLimit), got: {:?}",
            result
        );
    }

    #[tokio::test]
    #[ntest::timeout(3000)] // 3s max - test should complete in ~500ms
    async fn test_cpu_time_limit_expensive_regex() {
        let limits = RuntimeLimits {
            heap_initial_mb: 16,
            heap_max_mb: 64,
            max_cpu_time_ms: 500,
            max_wall_clock_time_ms: 10000,
        };

        let code = r#"
            addEventListener('fetch', (event) => {
                const regex = /^(a+)+$/;
                const input = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaab';
                regex.test(input);
                event.respondWith(new Response('Should never reach here'));
            });
        "#;

        let script = Script::new(code);

        let mut worker = Worker::new(script, None, Some(limits)).await.unwrap();

        let req = HttpRequest {
            method: "GET".to_string(),
            url: "http://localhost/".to_string(),
            headers: Default::default(),
            body: None,
        };

        let (task, _rx) = Task::fetch(req);
        let result = worker.exec(task).await;

        // Should terminate due to CPU time limit
        assert_eq!(
            result,
            Err(TerminationReason::CpuTimeLimit),
            "Expected Err(TerminationReason::CpuTimeLimit), got: {:?}",
            result
        );
    }

    #[tokio::test]
    #[ntest::timeout(3000)] // 3s max - test should complete in ~1s
    async fn test_cpu_time_not_charged_during_sleep() {
        let limits = RuntimeLimits {
            heap_initial_mb: 16,
            heap_max_mb: 64,
            max_cpu_time_ms: 100,
            max_wall_clock_time_ms: 2000,
        };

        let code = r#"
            addEventListener('fetch', async (event) => {
                await new Promise(resolve => setTimeout(resolve, 500));
                event.respondWith(new Response('Woke up after sleep', { status: 200 }));
            });
        "#;

        let script = Script::new(code);

        let mut worker = Worker::new(script, None, Some(limits)).await.unwrap();

        let req = HttpRequest {
            method: "GET".to_string(),
            url: "http://localhost/".to_string(),
            headers: Default::default(),
            body: None,
        };

        let (task, rx) = Task::fetch(req);
        let result = worker.exec(task).await;

        assert!(result.is_ok(), "Expected Ok(), got: {:?}", result);

        let response = rx.await.unwrap();
        assert_eq!(response.status, 200);
    }

    #[tokio::test]
    #[ntest::timeout(3000)] // 3s max - test should complete in ~200ms
    async fn test_cpu_limit_priority_over_wall_clock() {
        let limits = RuntimeLimits {
            heap_initial_mb: 16,
            heap_max_mb: 64,
            max_cpu_time_ms: 200,
            max_wall_clock_time_ms: 5000,
        };

        let code = r#"
            addEventListener('fetch', (event) => {
                while (true) {
                    Math.sqrt(Math.random());
                }
            });
        "#;

        let script = Script::new(code);

        let mut worker = Worker::new(script, None, Some(limits)).await.unwrap();

        let req = HttpRequest {
            method: "GET".to_string(),
            url: "http://localhost/".to_string(),
            headers: Default::default(),
            body: None,
        };

        let (task, _rx) = Task::fetch(req);
        let result = worker.exec(task).await;

        // Should terminate due to CPU limit (hit before wall-clock)
        assert_eq!(
            result,
            Err(TerminationReason::CpuTimeLimit),
            "Expected Err(TerminationReason::CpuTimeLimit), got: {:?}",
            result
        );
    }
}
