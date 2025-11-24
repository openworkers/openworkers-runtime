use deno_core::OpState;
use deno_core::serde::Serialize;

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

/// Log level for console output
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Log,
    Debug,
    Trace,
}

impl LogLevel {
    /// Parse log level from string
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "error" => LogLevel::Error,
            "warn" => LogLevel::Warn,
            "info" => LogLevel::Info,
            "log" => LogLevel::Log,
            "debug" => LogLevel::Debug,
            "trace" => LogLevel::Trace,
            _ => LogLevel::Info, // Default to Info for unknown levels
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Error => write!(f, "ERROR"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Log => write!(f, "LOG"),
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Trace => write!(f, "TRACE"),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LogEvent {
    pub level: LogLevel,
    pub message: String,
}

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
