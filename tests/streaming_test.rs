use openworkers_core::{HttpRequest, ResponseBody, Script, Task};
use openworkers_runtime_deno::Worker;
use std::collections::HashMap;

#[tokio::test]
async fn test_streaming_response() {
    let script = r#"
        addEventListener('fetch', (event) => {
            const stream = new ReadableStream({
                start(controller) {
                    controller.enqueue(new TextEncoder().encode('chunk1'));
                    controller.enqueue(new TextEncoder().encode('chunk2'));
                    controller.enqueue(new TextEncoder().encode('chunk3'));
                    controller.close();
                }
            });

            event.respondWith(new Response(stream, {
                headers: { 'Content-Type': 'text/plain' }
            }));
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

    // The response should be a stream
    match response.body {
        ResponseBody::Stream(mut rx) => {
            let mut chunks = Vec::new();
            while let Some(result) = rx.recv().await {
                match result {
                    Ok(bytes) => chunks.push(bytes),
                    Err(e) => panic!("Stream error: {}", e),
                }
            }

            // Collect all chunks into a single string
            let body: Vec<u8> = chunks.iter().flat_map(|b| b.to_vec()).collect();
            let body_str = String::from_utf8_lossy(&body);
            assert_eq!(body_str, "chunk1chunk2chunk3");
        }
        ResponseBody::Bytes(bytes) => {
            // If the runtime buffers it, that's also acceptable
            let body_str = String::from_utf8_lossy(&bytes);
            assert_eq!(body_str, "chunk1chunk2chunk3");
        }
        ResponseBody::None => {
            panic!("Expected body, got None");
        }
    }
}

#[tokio::test]
async fn test_fetch_streaming_proxy() {
    // This test simulates proxying a streaming response from fetch()
    // Skip if no network available
    let script = r#"
        addEventListener('fetch', async (event) => {
            // Create a simple stream that mimics what fetch() would return
            const stream = new ReadableStream({
                async start(controller) {
                    controller.enqueue(new TextEncoder().encode('{"status":'));
                    controller.enqueue(new TextEncoder().encode('"ok"}'));
                    controller.close();
                }
            });

            event.respondWith(new Response(stream, {
                headers: { 'Content-Type': 'application/json' }
            }));
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

    // Collect body (works for both Stream and Bytes)
    let body_str = match response.body {
        ResponseBody::Stream(mut rx) => {
            let mut chunks = Vec::new();
            while let Some(result) = rx.recv().await {
                if let Ok(bytes) = result {
                    chunks.push(bytes);
                }
            }
            let body: Vec<u8> = chunks.iter().flat_map(|b| b.to_vec()).collect();
            String::from_utf8_lossy(&body).to_string()
        }
        ResponseBody::Bytes(bytes) => String::from_utf8_lossy(&bytes).to_string(),
        ResponseBody::None => String::new(),
    };

    assert_eq!(body_str, r#"{"status":"ok"}"#);
}
