use openworkers_core::{HttpMethod, HttpRequest, RequestBody, RuntimeLimits, Script, Task};
use openworkers_runtime_deno::Worker;
use std::time::Duration;

#[tokio::test]
async fn test_heap_limits_configured() {
    env_logger::try_init().ok();

    let code = r#"
        addEventListener('fetch', (event) => {
            event.respondWith(new Response('OK'));
        });
    "#;

    let script = Script::new(code);

    let limits = RuntimeLimits {
        heap_initial_mb: 1,
        heap_max_mb: 64,
        max_cpu_time_ms: 0,        // Disabled for this test
        max_wall_clock_time_ms: 0, // Disabled for this test
    };

    println!("\n🧪 Testing heap limits are configured...\n");

    // Create worker with custom limits
    let result = Worker::new(script, None, Some(limits)).await;

    println!("✅ Worker created with custom heap limits (1MB-64MB)\n");

    assert!(
        result.is_ok(),
        "Worker creation should succeed with custom limits"
    );
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

    let script = Script::new(code);

    println!("\n🧪 Testing normal execution (should succeed)...\n");

    // Create worker with default limits
    let mut worker = Worker::new(script, None, None).await.unwrap();

    // Create a fetch task
    let req = HttpRequest {
        method: HttpMethod::Get,
        url: "http://localhost/".to_string(),
        headers: Default::default(),
        body: RequestBody::None,
    };

    let (task, rx) = Task::fetch(req);

    // Execute with timeout
    let timeout = Duration::from_millis(50);
    let result = tokio::time::timeout(timeout, worker.exec(task)).await;

    println!("\n✅ Test complete. Worker executed successfully.\n");

    // Should succeed
    assert!(result.is_ok(), "Normal execution should succeed");
    assert!(result.unwrap().is_ok(), "Worker should not error");

    // Check response
    let response = rx.await.unwrap();
    assert_eq!(response.status, 200);
}

#[tokio::test]
async fn test_cpu_intensive_computation_termination() {
    env_logger::try_init().ok();

    let code = r#"
        addEventListener('fetch', (event) => {
            event.respondWith(handleRequest());
        });

        function handleRequest() {
            console.log('Starting CPU-intensive computation...');

            let sum = 0;
            // CPU-intensive loop that would normally take ~500ms
            for (let i = 0; i < 100000000; i++) {
                sum += Math.sqrt(i * Math.random());
            }

            console.log('Computation done:', sum);
            return new Response(`Result: ${sum}`);
        }
    "#;

    let script = Script::new(code);

    let limits = RuntimeLimits {
        heap_initial_mb: 1,
        heap_max_mb: 128,
        max_cpu_time_ms: 50, // 50ms CPU limit (computation would take ~500ms)
        max_wall_clock_time_ms: 0, // Disabled - testing CPU enforcement only
    };

    println!("\n🧪 Testing CPU-intensive computation termination (50ms CPU limit)...\n");

    let mut worker = Worker::new(script, None, Some(limits)).await.unwrap();

    let req = HttpRequest {
        method: HttpMethod::Get,
        url: "http://localhost/".to_string(),
        headers: Default::default(),
        body: RequestBody::None,
    };

    let (task, _rx) = Task::fetch(req);

    let start = std::time::Instant::now();
    let _result = worker.exec(task).await;
    let elapsed = start.elapsed();

    println!("✅ CPU-intensive worker terminated after {:?}\n", elapsed);

    // Should terminate around 50ms on all platforms
    // - Linux: CPU enforcer terminates at 50ms CPU time
    // - macOS: No CPU enforcement, wall-clock disabled (0), so computation completes
    #[cfg(target_os = "linux")]
    {
        // On Linux, CPU enforcement should work
        assert!(
            elapsed < Duration::from_millis(150),
            "Should terminate quickly with CPU enforcement (got {:?})",
            elapsed
        );
        println!("✅ Linux: CPU enforcement terminated computation at 50ms");
    }

    #[cfg(not(target_os = "linux"))]
    {
        println!("⚠️  CPU enforcement not available on this platform (Linux-only)");
        println!("   Wall-clock disabled (0) - computation completed normally");
        // On macOS without enforcement, computation completes (may take longer)
    }
}

#[tokio::test]
async fn test_cpu_time_ignores_sleep() {
    env_logger::try_init().ok();

    let code = r#"
        addEventListener('fetch', (event) => {
            event.respondWith(handleRequest());
        });

        async function handleRequest() {
            console.log('Sleeping for 100ms...');

            // Sleep for 100ms (should NOT count as CPU time)
            await new Promise(resolve => setTimeout(resolve, 100));

            console.log('Done sleeping');
            return new Response('OK');
        }
    "#;

    let script = Script::new(code);

    let limits = RuntimeLimits {
        heap_initial_mb: 1,
        heap_max_mb: 128,
        max_cpu_time_ms: 10,         // 10ms CPU limit
        max_wall_clock_time_ms: 200, // 200ms wall-clock (sleep is 100ms, should succeed)
    };

    println!("\n🧪 Testing CPU time ignores sleep (100ms sleep with 10ms CPU limit)...\n");

    let mut worker = Worker::new(script, None, Some(limits)).await.unwrap();

    let req = HttpRequest {
        method: HttpMethod::Get,
        url: "http://localhost/".to_string(),
        headers: Default::default(),
        body: RequestBody::None,
    };

    let (task, rx) = Task::fetch(req);

    // Execute
    let result = worker.exec(task).await;

    println!("✅ Worker completed: {:?}\n", result);

    // Worker should succeed because:
    // - Linux: Sleep doesn't count as CPU time (10ms CPU limit not hit)
    // - macOS: Wall-clock is 200ms, sleep is 100ms (within limit)
    assert!(
        result.is_ok(),
        "Worker should succeed - sleep doesn't count as CPU time (Linux) or within wall-clock limit (macOS)"
    );

    // Check response
    let response = rx.await.unwrap();
    assert_eq!(response.status, 200);

    #[cfg(target_os = "linux")]
    println!("✅ Linux: CPU enforcement worked, sleep ignored");

    #[cfg(not(target_os = "linux"))]
    println!("✅ macOS: Wall-clock enforcement, sleep within 200ms limit");
}
