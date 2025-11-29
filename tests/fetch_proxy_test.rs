use openworkers_core::{HttpRequest, ResponseBody, Script, Task};
use openworkers_runtime_deno::Worker;
use std::collections::HashMap;

#[tokio::test]
async fn test_fetch_proxy() {
    // Test proxying a fetch response
    let script = r#"
        addEventListener('fetch', async (event) => {
            // Fetch from a real URL and proxy the response
            const response = await fetch('https://httpbin.org/get');
            event.respondWith(response);
        });
    "#;

    let script_obj = Script::new(script);
    let mut worker = Worker::new(script_obj, None, None)
        .await
        .expect("Worker should initialize");

    let request = HttpRequest {
        method: "GET".to_string(),
        url: "http://localhost/".to_string(),
        headers: HashMap::new(),
        body: None,
    };

    let (task, rx) = Task::fetch(request);
    worker.exec(task).await.expect("Task should execute");

    let response = rx.await.expect("Should receive response");
    assert_eq!(response.status, 200);

    // Check what type of body we got
    match &response.body {
        ResponseBody::Stream(_) => {
            println!("Got streaming response");
            // This is what we expect for a fetch() proxy
        }
        ResponseBody::Bytes(bytes) => {
            println!("Got buffered response: {} bytes", bytes.len());
            let body_str = String::from_utf8_lossy(bytes);
            assert!(
                body_str.contains("httpbin.org"),
                "Should contain httpbin.org"
            );
        }
        ResponseBody::None => {
            panic!("Expected body, got None");
        }
    }
}
