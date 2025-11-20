use openworkers_runtime::{RuntimeLimits, Script, Task, Worker};
use std::time::Duration;

#[tokio::test]
async fn test_heap_limits_configured() {
    env_logger::try_init().ok();

    let code = r#"
        addEventListener('fetch', (event) => {
            event.respondWith(new Response('OK'));
        });
    "#;

    let script = Script {
        code: code.to_string(),
        env: None,
    };

    let limits = RuntimeLimits {
        heap_initial_mb: 1,
        heap_max_mb: 64,
        max_execution_time_ms: 0, // Disabled for this test
    };

    println!("\n🧪 Testing heap limits are configured...\n");

    // Create worker with custom limits
    let result = Worker::new(script, None, Some(limits)).await;

    println!("✅ Worker created with custom heap limits (1MB-64MB)\n");

    assert!(result.is_ok(), "Worker creation should succeed with custom limits");
}

#[tokio::test]
async fn test_normal_execution_works() {
    env_logger::try_init().ok();

    let code = r#"
        addEventListener('fetch', (event) => {
            event.respondWith(handleRequest());
        });

        async function handleRequest() {
            console.log('Handling normal request...');
            return new Response('Hello, World!');
        }
    "#;

    let script = Script {
        code: code.to_string(),
        env: None,
    };

    println!("\n🧪 Testing normal execution (should succeed)...\n");

    // Create worker with default limits
    let mut worker = Worker::new(script, None, None).await.unwrap();

    // Create a dummy fetch task
    let (res_tx, res_rx) = tokio::sync::oneshot::channel();
    let req = http_v02::Request::builder()
        .uri("http://localhost/")
        .body(bytes::Bytes::new())
        .unwrap();

    let task = Task::Fetch(Some(openworkers_runtime::FetchInit::new(req, res_tx)));

    // Execute with timeout
    let timeout = Duration::from_millis(50);
    let result = tokio::time::timeout(timeout, worker.exec(task)).await;

    println!("\n✅ Test complete. Worker executed successfully.\n");

    // Should succeed
    assert!(result.is_ok(), "Normal execution should succeed");
    assert!(result.unwrap().is_ok(), "Worker should not error");

    // Check response
    let response = res_rx.await.unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn test_synchronous_infinite_loop_termination() {
    env_logger::try_init().ok();

    let code = r#"
        addEventListener('fetch', (event) => {
            event.respondWith(handleRequest());
        });

        function handleRequest() {
            console.log('Starting synchronous infinite loop...');

            // Synchronous infinite loop - should be terminated by watchdog
            while (true) {
                // Busy loop
            }

            return new Response('Should never reach here');
        }
    "#;

    let script = Script {
        code: code.to_string(),
        env: None,
    };

    let limits = RuntimeLimits {
        heap_initial_mb: 1,
        heap_max_mb: 128,
        max_execution_time_ms: 100, // 100ms timeout
    };

    println!("\n🧪 Testing synchronous infinite loop termination...\n");

    let mut worker = Worker::new(script, None, Some(limits)).await.unwrap();

    let (res_tx, _res_rx) = tokio::sync::oneshot::channel();
    let req = http_v02::Request::builder()
        .uri("http://localhost/")
        .body(bytes::Bytes::new())
        .unwrap();

    let task = Task::Fetch(Some(openworkers_runtime::FetchInit::new(req, res_tx)));

    let start = std::time::Instant::now();
    let result = worker.exec(task).await;
    let elapsed = start.elapsed();

    println!("✅ Worker terminated after {:?} with result: {:?}\n", elapsed, result);

    // TimeoutGuard should terminate execution around 100ms
    // V8 may or may not return an error after termination
    assert!(
        elapsed >= Duration::from_millis(90),
        "Should run at least 90ms (near the 100ms timeout)"
    );
    assert!(
        elapsed < Duration::from_millis(200),
        "Should terminate quickly after timeout (within 200ms)"
    );

    // The worker was terminated, either with error or Ok (both are valid)
    // The important thing is it didn't run forever
}
