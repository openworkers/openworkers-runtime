use crate::LogEvent;
use crate::LogSender;
use crate::Task;
use crate::TerminationReason;
use crate::env::ToJsonString;
use crate::ext::Permissions;
use crate::ext::fetch_event_ext;
use crate::ext::noop_ext;
use crate::ext::permissions_ext;
use crate::ext::runtime_ext;
use crate::ext::scheduled_event_ext;
use crate::security::{CpuEnforcer, CpuTimer, CustomAllocator, TimeoutGuard};

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use deno_core::JsRuntime;
use deno_core::url::Url;
use deno_core::v8;

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
        deno_webidl::deno_webidl::init(),
        deno_console::deno_console::init(),
        deno_url::deno_url::init(),
        deno_web::deno_web::init::<Permissions>(
            std::sync::Arc::new(deno_web::BlobStore::default()),
            None,
        ),
        deno_crypto::deno_crypto::init(None),
        deno_fetch::deno_fetch::init::<Permissions>(deno_fetch::Options {
            user_agent: user_agent(),
            ..Default::default()
        }),
        // OpenWorkers extensions
        noop_ext::init(),
        fetch_event_ext::init(),
        scheduled_event_ext::init(),
        runtime_ext::init(),
        permissions_ext::init(),
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

use crate::RuntimeLimits;
use crate::Script;

pub struct Worker {
    pub(crate) js_runtime: deno_core::JsRuntime,
    pub(crate) trigger_fetch: deno_core::v8::Global<deno_core::v8::Function>,
    pub(crate) trigger_scheduled: deno_core::v8::Global<deno_core::v8::Function>,
    pub(crate) isolate_handle: v8::IsolateHandle,
    pub(crate) limits: RuntimeLimits,
    pub(crate) memory_limit_hit_flag: Arc<AtomicBool>,
    aborted: Arc<AtomicBool>,
}

impl Worker {
    pub async fn new(
        script: Script,
        log_tx: Option<LogSender>,
        limits: Option<RuntimeLimits>,
    ) -> Result<Self, TerminationReason> {
        // Initialize rustls CryptoProvider (required for rustls 0.23+)
        // This is needed for HTTPS fetch requests from workers
        // We use a once_cell to ensure it's only initialized once
        static CRYPTO_INIT: std::sync::Once = std::sync::Once::new();
        CRYPTO_INIT.call_once(|| {
            // Install the default provider (ring-based crypto)
            // Ignore error if already installed (can happen in tests)
            let _ = rustls::crypto::ring::default_provider().install_default();
        });

        let startup_snapshot = runtime_snapshot();
        let snapshot_is_some = startup_snapshot.is_some();

        let limits = limits.unwrap_or_default();

        // Convert heap limits from MB to bytes
        let heap_initial = limits.heap_initial_mb * 1024 * 1024;
        let heap_max = limits.heap_max_mb * 1024 * 1024;

        // Create custom ArrayBuffer allocator to enforce memory limits on external memory
        // This is critical: V8 heap limits don't cover ArrayBuffers, Uint8Array, etc.
        let memory_limit_hit_flag = Arc::new(AtomicBool::new(false));
        let array_buffer_allocator =
            CustomAllocator::new(heap_max, Arc::clone(&memory_limit_hit_flag));

        let mut js_runtime = JsRuntime::new(deno_core::RuntimeOptions {
            is_main: true,
            extensions: extensions(snapshot_is_some),
            module_loader: Some(Rc::new(deno_core::FsModuleLoader)),
            startup_snapshot,
            extension_transpiler: None,
            create_params: Some(
                v8::CreateParams::default()
                    .heap_limits(heap_initial, heap_max)
                    .array_buffer_allocator(array_buffer_allocator.into_v8_allocator()),
            ),
            ..Default::default()
        });

        debug!(
            "runtime created ({} snapshot, heap: {}MB-{}MB), bootstrapping...",
            if snapshot_is_some { "with" } else { "without" },
            limits.heap_initial_mb,
            limits.heap_max_mb
        );

        // Capture isolate handle for termination support
        let isolate_handle = js_runtime.v8_isolate().thread_safe_handle();

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

            let triggers = js_runtime
                .execute_script(deno_core::located_script_name!(), script)
                .map_err(|e| {
                    TerminationReason::InitializationError(format!("Bootstrap failed: {}", e))
                })?;

            let context = js_runtime.main_context();
            let isolate = js_runtime.v8_isolate();
            v8::scope!(scope, isolate);
            let context = v8::Local::new(scope, &context);
            let scope = &mut v8::ContextScope::new(scope, context);

            let triggers = v8::Local::new(scope, triggers);

            debug!("bootstrap succeeded with triggers: {:?}", triggers);

            let object: v8::Local<v8::Object> = triggers.try_into().map_err(|e| {
                TerminationReason::InitializationError(format!(
                    "Failed to convert triggers to object: {:?}",
                    e
                ))
            })?;

            trigger_fetch =
                crate::util::extract_trigger("fetch", scope, object).ok_or_else(|| {
                    TerminationReason::InitializationError(
                        "Fetch trigger not found in bootstrap response".to_string(),
                    )
                })?;
            trigger_scheduled = crate::util::extract_trigger("scheduled", scope, object)
                .ok_or_else(|| {
                    TerminationReason::InitializationError(
                        "Scheduled trigger not found in bootstrap response".to_string(),
                    )
                })?;
        };

        debug!("runtime bootstrapped, evaluating main module...");

        // Eval main module
        {
            let specifier = module_url("worker.js");

            let mod_id = js_runtime
                .load_main_es_module_from_code(&specifier, script.code)
                .await
                .map_err(|e| {
                    TerminationReason::Exception(format!("Failed to load main module: {}", e))
                })?;

            let result = js_runtime.mod_evaluate(mod_id);

            let opts = deno_core::PollEventLoopOptions {
                wait_for_inspector: false,
                pump_v8_message_loop: true,
            };

            js_runtime
                .run_event_loop(opts)
                .await
                .map_err(|e| TerminationReason::Exception(format!("Event loop error: {}", e)))?;

            result.await.map_err(|e| {
                TerminationReason::Exception(format!("Module evaluation error: {}", e))
            })?;
        };

        debug!("main module evaluated");

        Ok(Self {
            js_runtime,
            trigger_fetch,
            trigger_scheduled,
            isolate_handle,
            limits,
            memory_limit_hit_flag,
            aborted: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Abort the worker execution
    pub fn abort(&mut self) {
        self.aborted.store(true, Ordering::SeqCst);
        self.isolate_handle.terminate_execution();
    }

    pub async fn exec(&mut self, mut task: Task) -> Result<(), TerminationReason> {
        // Check if aborted before starting
        if self.aborted.load(Ordering::SeqCst) {
            return Err(TerminationReason::Aborted);
        }
        debug!("executing task {:?}", task.task_type());

        // Start CPU time measurement
        let cpu_timer = CpuTimer::start();

        // Enforce BOTH CPU and wall-clock limits simultaneously
        // Whichever limit is hit first will terminate execution

        // 1. CPU time enforcement (Linux-only, via POSIX timer + SIGALRM)
        let cpu_enforcer =
            CpuEnforcer::new(self.isolate_handle.clone(), self.limits.max_cpu_time_ms);

        // 2. Wall-clock enforcement (all platforms, via watchdog thread)
        let wall_guard = TimeoutGuard::new(
            self.isolate_handle.clone(),
            self.limits.max_wall_clock_time_ms,
        );

        let trigger_exception = crate::util::exec_task(self, &mut task);

        let opts = deno_core::PollEventLoopOptions {
            wait_for_inspector: false,
            pump_v8_message_loop: true,
        };

        // Wrap event loop with tokio timeout if wall-clock limit is set
        // This ensures we stop even if Deno ops (like setTimeout, fetch) are pending
        // terminate_execution() only stops running JS, not pending async ops
        let result: Result<(), String> = if self.limits.max_wall_clock_time_ms > 0 {
            let timeout_duration =
                std::time::Duration::from_millis(self.limits.max_wall_clock_time_ms);
            match tokio::time::timeout(timeout_duration, self.js_runtime.run_event_loop(opts)).await
            {
                Ok(inner_result) => inner_result.map_err(|e| e.to_string()),
                Err(_elapsed) => {
                    // Timeout elapsed - terminate V8 execution
                    self.isolate_handle.terminate_execution();
                    Err("Wall-clock timeout exceeded".to_string())
                }
            }
        } else {
            self.js_runtime
                .run_event_loop(opts)
                .await
                .map_err(|e| e.to_string())
        };

        // Log CPU time metrics
        let cpu_time = cpu_timer.elapsed();
        debug!(
            "task completed: cpu_time={:?}, cpu_limit={}ms, wall_limit={}ms",
            cpu_time, self.limits.max_cpu_time_ms, self.limits.max_wall_clock_time_ms
        );

        // Check if memory limit was hit during execution
        let memory_limit_hit = self.memory_limit_hit_flag.swap(false, Ordering::SeqCst);

        // Check if aborted
        let was_aborted = self.aborted.load(Ordering::SeqCst);

        // Determine termination reason by inspecting guards FIRST, then trigger result and error
        // Guards must be checked first because terminate_execution() causes exceptions
        let cpu_limit_hit = cpu_enforcer
            .as_ref()
            .map(|e| e.was_terminated())
            .unwrap_or(false);
        let wall_clock_hit = wall_guard.was_triggered();

        // Also check if tokio timeout triggered (for async ops like setTimeout, fetch)
        let tokio_timeout_hit = result
            .as_ref()
            .err()
            .map(|e| e.to_string().contains("Wall-clock timeout exceeded"))
            .unwrap_or(false);

        // Determine termination reason and return appropriate Result
        if cpu_limit_hit {
            debug!("worker terminated: reason=CpuTimeLimit");
            return Err(TerminationReason::CpuTimeLimit);
        }

        if wall_clock_hit || tokio_timeout_hit {
            debug!("worker terminated: reason=WallClockTimeout");
            return Err(TerminationReason::WallClockTimeout);
        }

        if memory_limit_hit {
            debug!("worker terminated: reason=MemoryLimit");
            return Err(TerminationReason::MemoryLimit);
        }

        if was_aborted {
            debug!("worker terminated: reason=Aborted");
            return Err(TerminationReason::Aborted);
        }

        if let Some(exception_msg) = trigger_exception {
            // Trigger call failed (exception thrown during event dispatch)
            // Check if it's a memory-related exception
            if exception_msg.contains("Array buffer allocation failed")
                || exception_msg.contains("RangeError")
                || exception_msg.contains("out of memory")
            {
                debug!("worker terminated: reason=MemoryLimit (from exception)");
                return Err(TerminationReason::MemoryLimit);
            }
            debug!("worker terminated: reason=Exception");
            return Err(TerminationReason::Exception(exception_msg));
        }

        if let Err(error_msg) = result {
            // Check if it's a memory error by inspecting the error message
            if error_msg.contains("out of memory")
                || error_msg.contains("Array buffer allocation failed")
                || error_msg.contains("RangeError")
            {
                debug!("worker terminated: reason=MemoryLimit (from error)");
                return Err(TerminationReason::MemoryLimit);
            }
            debug!("worker terminated: reason=Exception");
            return Err(TerminationReason::Exception(error_msg));
        }

        debug!("worker completed successfully");
        Ok(())
    }
}

impl openworkers_core::Worker for Worker {
    async fn new(
        script: Script,
        log_tx: Option<LogSender>,
        limits: Option<RuntimeLimits>,
    ) -> Result<Self, TerminationReason> {
        Worker::new(script, log_tx, limits).await
    }

    async fn exec(&mut self, task: Task) -> Result<(), TerminationReason> {
        Worker::exec(self, task).await
    }

    fn abort(&mut self) {
        Worker::abort(self)
    }
}
