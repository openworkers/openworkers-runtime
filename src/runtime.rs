use crate::env::ToJsonString;
use crate::ext::fetch_event_ext;
use crate::ext::noop_ext;
use crate::ext::permissions_ext;
use crate::ext::runtime_ext;
use crate::ext::scheduled_event_ext;
use crate::ext::Permissions;
use crate::LogEvent;
use crate::Task;

use std::collections::HashMap;
use std::rc::Rc;

use deno_core::error::AnyError;
use deno_core::error::CoreError;
use deno_core::url::Url;
use deno_core::v8;
use deno_core::JsRuntime;

use log::debug;

const USER_AGENT: &str = concat!("OpenWorkers/", env!("CARGO_PKG_VERSION"));

const RUNTIME_SNAPSHOT: &[u8] = include_bytes!(env!("RUNTIME_SNAPSHOT_PATH"));

fn module_url(path_str: &str) -> Url {
    let current_dir = std::env::current_dir().unwrap();
    let current_dir = current_dir.as_path();
    deno_core::resolve_path(path_str, current_dir).unwrap()
}

pub(crate) fn user_agent() -> String {
    USER_AGENT.to_string()
}

pub(crate) fn runtime_snapshot() -> Option<&'static [u8]> {
    match RUNTIME_SNAPSHOT.len() {
        0 => None,
        _ => Some(RUNTIME_SNAPSHOT),
    }
}

pub(crate) fn extensions(skip_esm: bool) -> Vec<deno_core::Extension> {
    let mut exts = vec![
        deno_webidl::deno_webidl::init_ops_and_esm(),
        deno_console::deno_console::init_ops_and_esm(),
        deno_url::deno_url::init_ops_and_esm(),
        deno_web::deno_web::init_ops_and_esm::<Permissions>(
            std::sync::Arc::new(deno_web::BlobStore::default()),
            None,
        ),
        deno_crypto::deno_crypto::init_ops_and_esm(None),
        deno_fetch::deno_fetch::init_ops_and_esm::<Permissions>(deno_fetch::Options {
            user_agent: user_agent(),
            ..Default::default()
        }),
        // OpenWorkers extensions
        noop_ext::init_ops_and_esm(),
        fetch_event_ext::init_ops_and_esm(),
        scheduled_event_ext::init_ops_and_esm(),
        runtime_ext::init_ops_and_esm(),
        permissions_ext::init_ops(),
    ];

    if !skip_esm {
        return exts;
    }

    for ext in &mut exts {
        ext.js_files = std::borrow::Cow::Borrowed(&[]);
        ext.esm_files = std::borrow::Cow::Borrowed(&[]);
        ext.esm_entry_point = None;
    }

    exts
}

pub struct Script {
    pub code: String,
    pub env: Option<HashMap<String, String>>,
}

pub struct Worker {
    pub(crate) js_runtime: deno_core::JsRuntime,
    pub(crate) trigger_fetch: deno_core::v8::Global<deno_core::v8::Function>,
    pub(crate) trigger_scheduled: deno_core::v8::Global<deno_core::v8::Function>,
}

impl Worker {
    pub async fn new(
        script: Script,
        log_tx: Option<std::sync::mpsc::Sender<LogEvent>>,
    ) -> Result<Self, AnyError> {
        let startup_snapshot = runtime_snapshot();
        let snapshot_is_some = startup_snapshot.is_some();

        let mut js_runtime = JsRuntime::new(deno_core::RuntimeOptions {
            is_main: true,
            extensions: extensions(snapshot_is_some),
            module_loader: Some(Rc::new(deno_core::FsModuleLoader)),
            startup_snapshot,
            extension_transpiler: None,
            ..Default::default()
        });

        debug!(
            "runtime created ({} snapshot), bootstrapping...",
            if snapshot_is_some { "with" } else { "without" }
        );

        let trigger_fetch;
        let trigger_scheduled;

        // Log event sender
        {
            match log_tx {
                Some(tx) => js_runtime
                    .op_state()
                    .borrow_mut()
                    .put::<std::sync::mpsc::Sender<LogEvent>>(tx),
                None => {
                    log::warn!("no log event sender provided");
                }
            };
        }

        // Bootstrap
        {
            let script = format!(
                "globalThis.bootstrap('{}', {})",
                user_agent(),
                script.env.to_json_string()
            );
            let script = deno_core::ModuleCodeString::from(script);

            match js_runtime.execute_script(deno_core::located_script_name!(), script) {
                Ok(triggers) => {
                    let scope = &mut js_runtime.handle_scope();

                    let triggers = v8::Local::new(scope, triggers);

                    debug!("bootstrap succeeded with triggers: {:?}", triggers);

                    let object: v8::Local<v8::Object> = match triggers.try_into() {
                        Ok(object) => object,
                        Err(err) => panic!("failed to convert triggers to object: {:?}", err),
                    };

                    trigger_fetch = crate::util::extract_trigger("fetch", scope, object)
                        .expect("fetch trigger not found");
                    trigger_scheduled = crate::util::extract_trigger("scheduled", scope, object)
                        .expect("scheduled trigger not found");
                }
                Err(err) => panic!("bootstrap failed: {:?}", err),
            }
        };

        debug!("runtime bootstrapped, evaluating main module...");

        // Eval main module
        {
            let specifier = module_url("worker.js");

            let mod_id = js_runtime
                .load_main_es_module_from_code(&specifier, script.code)
                .await;

            let mod_id = match mod_id {
                Ok(mod_id) => mod_id,
                Err(err) => panic!("failed to load main module: {:?}", err),
            };

            let result = js_runtime.mod_evaluate(mod_id);

            let opts = deno_core::PollEventLoopOptions {
                wait_for_inspector: false,
                pump_v8_message_loop: true,
            };

            js_runtime.run_event_loop(opts).await?;

            result.await?;
        };

        debug!("main module evaluated");

        Ok(Self {
            js_runtime,
            trigger_fetch,
            trigger_scheduled,
        })
    }

    pub async fn exec(&mut self, mut task: Task) -> Result<(), CoreError> {
        debug!("executing task {:?}", task.task_type());

        crate::util::exec_task(self, &mut task);

        let opts = deno_core::PollEventLoopOptions {
            wait_for_inspector: false,
            pump_v8_message_loop: true,
        };

        self.js_runtime.run_event_loop(opts).await
    }
}
