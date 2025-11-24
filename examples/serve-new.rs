use bytes::Bytes;

use log::{debug, error};
use openworkers_runtime::{FetchInit, HttpRequest, HttpResponse, Script, Task, Worker};

use tokio::sync::oneshot::channel;

use actix_web::App;
use actix_web::HttpServer;
use actix_web::web;
use actix_web::web::Data;

struct AppState {
    code: String,
}

async fn handle_request(
    data: Data<AppState>,
    req: actix_web::HttpRequest,
    body: Bytes,
) -> actix_web::HttpResponse {
    debug!(
        "handle_request of: {} {} in thread {:?}",
        req.method(),
        req.uri(),
        std::thread::current().id()
    );

    let start = tokio::time::Instant::now();

    // Convert actix request to our HttpRequest type
    let req = HttpRequest::from_actix(&req, body);

    let script = Script {
        code: data.code.clone(),
        env: None,
    };

    let (res_tx, res_rx) = channel::<HttpResponse>();
    let task = Task::Fetch(Some(FetchInit::new(req, res_tx)));

    let handle = std::thread::spawn(move || {
        let local = tokio::task::LocalSet::new();

        let tasks = local.spawn_local(async move {
            debug!("create worker");
            let mut worker = Worker::new(script, None, None).await.unwrap();

            debug!("exec fetch task");
            match worker.exec(task).await {
                Ok(_reason) => debug!("exec completed"),
                Err(err) => error!("exec did not complete: {err}"),
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

    let response = match res_rx.await {
        Ok(res) => {
            // Convert our HttpResponse to actix_web::HttpResponse
            res.into()
        }
        Err(err) => {
            error!(
                "worker fetch error: {}, ensure the worker registered a listener for the 'fetch' event",
                err
            );
            actix_web::HttpResponse::InternalServerError().body(err.to_string())
        }
    };

    handle.join().unwrap();

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

    HttpServer::new(|| {
        App::new()
            .app_data(Data::new(AppState { code: get_code() }))
            .default_service(web::to(handle_request))
    })
    .bind(("127.0.0.1", 8080))?
    .workers(4)
    .run()
    .await
}
