use bytes::Bytes;

use log::debug;
use log::error;
use openworkers_runtime::FetchInit;
use openworkers_runtime::Script;
use openworkers_runtime::Task;
use openworkers_runtime::Worker;

use tokio::sync::oneshot::channel;

use actix_web::{App, HttpServer};

use actix_web::web;
use actix_web::web::Data;
use actix_web::HttpRequest;
use actix_web::HttpResponse;

struct AppState {
    task_tx: tokio::sync::mpsc::Sender<Task>,
}

async fn handle_request(data: Data<AppState>, req: HttpRequest, body: Bytes) -> HttpResponse {
    debug!(
        "handle_request: {} {} in thread {:?}",
        req.method(),
        req.uri(),
        std::thread::current().id()
    );

    let start = tokio::time::Instant::now();

    let (res_tx, res_rx) = channel::<http_v02::Response<Bytes>>();

    let req = http_v02::Request::builder()
        .uri(format!(
            "{}://{}{}",
            req.connection_info().scheme(),
            req.connection_info().host(),
            req.uri()
        ))
        .method(req.method())
        .body(body)
        .unwrap();

    match data
        .task_tx
        .send(Task::Fetch(Some(FetchInit::new(req, res_tx))))
        .await
    {
        Ok(()) => debug!("fetch task sent"),
        Err(err) => {
            error!("failed to send fetch task: {}", err);
            return HttpResponse::InternalServerError().body(err.to_string());
        }
    }

    let response = {
        match res_rx.await {
            Ok(res) => {
                let mut rb = HttpResponse::build(res.status());

                for (k, v) in res.headers() {
                    rb.append_header((k, v));
                }

                rb.body(res.body().clone())
            }
            Err(err) => {
                error!("worker fetch error: {}, ensure the worker registered a listener for the 'fetch' event", err);
                HttpResponse::InternalServerError().body(err.to_string())
            }
        }
    };

    debug!("handle_request done in {}ms", start.elapsed().as_millis());

    response
}

fn get_path() -> String {
    std::env::args()
        .nth(1)
        .unwrap_or_else(|| String::from("examples/serve.js"))
}

fn get_code() -> String {
    std::fs::read_to_string(get_path()).unwrap()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    if !std::env::var("RUST_LOG").is_ok() {
        std::env::set_var("RUST_LOG", "info");
    }

    env_logger::init();

    debug!("start main");

    // Check that the path is correct
    {
        let path = get_path();
        if !std::path::Path::new(&path).is_file() {
            eprintln!("file not found: {}", path);
            std::process::exit(1);
        }
    }

    println!("Listening on http://localhost:8080");

    HttpServer::new(move || {
        App::new()
            .app_data(Data::new({
                let script = Script {
                    code: get_code(),
                    env: None,
                };

                let (task_tx, mut task_rx) = tokio::sync::mpsc::channel(1);

                let _tread = std::thread::spawn(move || {
                    let local = tokio::task::LocalSet::new();

                    let tasks = local.spawn_local(async move {
                        let mut worker = Worker::new(script, None, None).await.unwrap();

                        loop {
                            match task_rx.recv().await {
                                Some(task) => match worker.exec(task).await {
                                    Ok(_reason) => debug!("exec completed"),
                                    Err(err) => error!("exec did not complete: {err}"),
                                },
                                None => {
                                    debug!("task_rx closed");
                                    break;
                                }
                            }
                        }
                    });

                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();

                    match local.block_on(&rt, tasks) {
                        Ok(()) => {}
                        Err(err) => error!("failed to wait for end: {err}"),
                    }
                });

                AppState { task_tx }
            }))
            .default_service(web::to(handle_request))
    })
    .bind(("127.0.0.1", 8080))?
    .workers(4)
    .run()
    .await
}
