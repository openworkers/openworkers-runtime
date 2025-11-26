use deno_core::OpState;
use openworkers_core::{LogEvent, LogLevel};

deno_core::extension!(
    runtime,
    deps = [
        deno_web,
        deno_crypto,
        deno_fetch,
        fetch_event,
        scheduled_event
    ],
    ops = [op_log],
    esm_entry_point = "ext:runtime.js",
    esm = ["ext:runtime.js" = "./src/ext/runtime.js",]
);

#[deno_core::op2(fast)]
fn op_log(state: &mut OpState, #[string] level: &str, #[string] message: &str) {
    let evt = LogEvent {
        level: LogLevel::from_str(level),
        message: message.to_string(),
    };

    log::debug!("op_log {:?}", evt);

    let tx = state.try_borrow_mut::<std::sync::mpsc::Sender<LogEvent>>();

    match tx {
        None => {}
        Some(tx) => match tx.send(evt) {
            Ok(_) => {}
            Err(_) => log::error!("failed to send log event"),
        },
    }
}
