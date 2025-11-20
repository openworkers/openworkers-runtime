use log::debug;
use log::error;
use openworkers_runtime::ScheduledInit;
use openworkers_runtime::Script;
use openworkers_runtime::Task;
use openworkers_runtime::Worker;
use tokio::sync::oneshot;

fn get_path() -> String {
    std::env::args()
        .nth(1)
        .unwrap_or_else(|| String::from("examples/scheduled.js"))
}

#[tokio::main]
async fn main() -> Result<(), ()> {
    if !std::env::var("RUST_LOG").is_ok() {
        std::env::set_var("RUST_LOG", "debug");
    }

    env_logger::init();

    debug!("start main");

    // Check that the path is correct
    let file_path = {
        let path = get_path();
        if !std::path::Path::new(&path).is_file() {
            eprintln!("file not found: {}", path);
            std::process::exit(1);
        }
        path
    };

    let (res_tx, res_rx) = oneshot::channel::<()>();
    let (end_tx, end_rx) =  oneshot::channel::<()>();

    let script = Script {
        code: std::fs::read_to_string(file_path).unwrap(),
        env: None
    };

    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();

    let handle = std::thread::spawn(move || {
        let local = tokio::task::LocalSet::new();

        local.spawn_local(async move {
            let mut worker = Worker::new(script, None, None).await.unwrap();

            match worker
                .exec(Task::Scheduled(Some(ScheduledInit::new(res_tx, time))))
                .await
            {
                Ok(()) => debug!("exec completed"),
                Err(err) => error!("exec did not complete: {err}"),
            }
        });

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        match local.block_on(&rt, async { end_rx.await }) {
            Ok(()) => {},
            Err(err) => error!("failed to wait for end: {err}"),
        }
    });

    debug!("worker started");

    tokio::select! {
        _ = tokio::signal::ctrl_c() => debug!("ctrl-c received"),
        // wait for task completion signal
        done = res_rx => match done {
            Ok(()) => debug!("task completed"),
            Err(err) => error!("task did not complete: {err}"),
        }
    }

    end_tx.send(()).unwrap();

    handle.join().unwrap();

    Ok(())
}
