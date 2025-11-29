use bytes::Bytes;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use openworkers_core::{HttpBody, HttpMethod, HttpRequest, Script, Task};
use openworkers_runtime_deno::Worker;
use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::mpsc;

const BODY_SIZE: usize = 10 * 1024; // 10KB

fn request_body_benchmarks(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let code = r#"
        addEventListener('fetch', async (event) => {
            const body = await event.request.text();
            event.respondWith(new Response('len:' + body.length));
        });
    "#;

    let body_data: Vec<u8> = (0..BODY_SIZE).map(|i| (i % 256) as u8).collect();

    let mut group = c.benchmark_group("RequestBody");
    group.throughput(Throughput::Bytes(BODY_SIZE as u64));

    // Benchmark buffered body
    group.bench_function("buffered", |b| {
        let script = Script::new(code);
        let mut worker = rt.block_on(Worker::new(script, None, None)).unwrap();

        b.iter(|| {
            rt.block_on(async {
                let req = HttpRequest {
                    method: HttpMethod::Post,
                    url: "http://localhost/".to_string(),
                    headers: Default::default(),
                    body: HttpBody::Bytes(Bytes::from(body_data.clone())),
                };

                let (task, rx) = Task::fetch(req);
                worker.exec(task).await.unwrap();
                let response = rx.await.unwrap();
                response.body.collect().await.unwrap()
            })
        });
    });

    // Benchmark streaming body (1 chunk)
    group.bench_function("stream_1_chunk", |b| {
        let script = Script::new(code);
        let worker = rt.block_on(async {
            Rc::new(RefCell::new(Worker::new(script, None, None).await.unwrap()))
        });

        b.iter(|| {
            let local = tokio::task::LocalSet::new();
            let worker = worker.clone();
            let body_data = body_data.clone();

            rt.block_on(local.run_until(async move {
                let (tx, rx) = mpsc::channel::<Result<Bytes, String>>(16);

                let req = HttpRequest {
                    method: HttpMethod::Post,
                    url: "http://localhost/".to_string(),
                    headers: Default::default(),
                    body: HttpBody::Stream(rx),
                };

                let (task, response_rx) = Task::fetch(req);
                let worker_clone = worker.clone();

                let exec_handle = tokio::task::spawn_local(async move {
                    worker_clone.borrow_mut().exec(task).await.unwrap();
                    response_rx.await.unwrap()
                });

                tx.send(Ok(Bytes::from(body_data))).await.unwrap();
                drop(tx);

                let response = exec_handle.await.unwrap();
                response.body.collect().await.unwrap()
            }))
        });
    });

    // Benchmark streaming body (10 chunks)
    group.bench_function("stream_10_chunks", |b| {
        let script = Script::new(code);
        let worker = rt.block_on(async {
            Rc::new(RefCell::new(Worker::new(script, None, None).await.unwrap()))
        });
        let chunk_size = BODY_SIZE / 10;

        b.iter(|| {
            let local = tokio::task::LocalSet::new();
            let worker = worker.clone();
            let body_data = body_data.clone();

            rt.block_on(local.run_until(async move {
                let (tx, rx) = mpsc::channel::<Result<Bytes, String>>(16);

                let req = HttpRequest {
                    method: HttpMethod::Post,
                    url: "http://localhost/".to_string(),
                    headers: Default::default(),
                    body: HttpBody::Stream(rx),
                };

                let (task, response_rx) = Task::fetch(req);
                let worker_clone = worker.clone();

                let exec_handle = tokio::task::spawn_local(async move {
                    worker_clone.borrow_mut().exec(task).await.unwrap();
                    response_rx.await.unwrap()
                });

                for i in 0..10 {
                    let chunk =
                        Bytes::from(body_data[i * chunk_size..(i + 1) * chunk_size].to_vec());
                    tx.send(Ok(chunk)).await.unwrap();
                }
                drop(tx);

                let response = exec_handle.await.unwrap();
                response.body.collect().await.unwrap()
            }))
        });
    });

    // Benchmark different body sizes
    group.finish();

    let mut size_group = c.benchmark_group("RequestBodySize");
    for size in [1024, 10 * 1024, 100 * 1024].iter() {
        let body_data: Vec<u8> = (0..*size).map(|i| (i % 256) as u8).collect();
        size_group.throughput(Throughput::Bytes(*size as u64));

        size_group.bench_with_input(BenchmarkId::new("buffered", size), size, |b, _| {
            let script = Script::new(code);
            let mut worker = rt.block_on(Worker::new(script, None, None)).unwrap();

            b.iter(|| {
                rt.block_on(async {
                    let req = HttpRequest {
                        method: HttpMethod::Post,
                        url: "http://localhost/".to_string(),
                        headers: Default::default(),
                        body: HttpBody::Bytes(Bytes::from(body_data.clone())),
                    };

                    let (task, rx) = Task::fetch(req);
                    worker.exec(task).await.unwrap();
                    let response = rx.await.unwrap();
                    response.body.collect().await.unwrap()
                })
            });
        });

        size_group.bench_with_input(BenchmarkId::new("stream", size), size, |b, _| {
            let script = Script::new(code);
            let worker = rt.block_on(async {
                Rc::new(RefCell::new(Worker::new(script, None, None).await.unwrap()))
            });

            b.iter(|| {
                let local = tokio::task::LocalSet::new();
                let worker = worker.clone();
                let body_data = body_data.clone();

                rt.block_on(local.run_until(async move {
                    let (tx, rx) = mpsc::channel::<Result<Bytes, String>>(16);

                    let req = HttpRequest {
                        method: HttpMethod::Post,
                        url: "http://localhost/".to_string(),
                        headers: Default::default(),
                        body: HttpBody::Stream(rx),
                    };

                    let (task, response_rx) = Task::fetch(req);
                    let worker_clone = worker.clone();

                    let exec_handle = tokio::task::spawn_local(async move {
                        worker_clone.borrow_mut().exec(task).await.unwrap();
                        response_rx.await.unwrap()
                    });

                    tx.send(Ok(Bytes::from(body_data))).await.unwrap();
                    drop(tx);

                    let response = exec_handle.await.unwrap();
                    response.body.collect().await.unwrap()
                }))
            });
        });
    }

    size_group.finish();
}

criterion_group!(benches, request_body_benchmarks);
criterion_main!(benches);
