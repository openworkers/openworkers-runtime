use deno_core::v8;

use crate::Task;
use crate::Worker;

pub(crate) fn extract_trigger<'a>(
    name: &str,
    scope: &mut v8::HandleScope<'a>,
    object: v8::Local<'a, v8::Object>,
) -> Option<v8::Global<v8::Function>> {
    let key = v8::String::new(scope, name).unwrap().into();

    let ret = match object.get(scope, key) {
        Some(fetch) => fetch,
        None => return None,
    };

    let ret: v8::Local<v8::Function> = match ret.try_into() {
        Ok(ret) => ret,
        Err(_) => return None,
    };

    Some(v8::Global::new(scope, ret))
}

/// Execute a task and return the exception message if one occurred
pub(crate) fn exec_task(worker: &mut Worker, task: &mut Task) -> Option<String> {
    let rid = {
        let op_state_rc = worker.js_runtime.op_state();
        let mut op_state = op_state_rc.borrow_mut();

        match task {
            Task::Fetch(data) => op_state.resource_table.add(data.take().unwrap()),
            Task::Scheduled(data) => op_state.resource_table.add(data.take().unwrap()),
        }
    };

    let scope = &mut worker.js_runtime.handle_scope();

    let trigger = v8::Local::new(
        scope,
        match task {
            Task::Fetch(_) => &worker.trigger_fetch,
            Task::Scheduled(_) => &worker.trigger_scheduled,
        },
    );

    let recv = v8::undefined(scope);

    let rid = v8::Integer::new(scope, rid as i32).into();

    // Use TryCatch to capture exception details
    let mut try_catch = v8::TryCatch::new(scope);

    match trigger.call(&mut try_catch, recv.into(), &[rid]) {
        Some(_) => {
            log::debug!("successfully called trigger");
            None
        }
        None => {
            // Get the exception message from TryCatch
            let exception_str = if try_catch.has_caught() {
                try_catch
                    .exception()
                    .and_then(|ex| ex.to_string(&mut try_catch))
                    .map(|s| s.to_rust_string_lossy(&mut try_catch))
                    .unwrap_or_else(|| "Unknown exception".to_string())
            } else {
                "Unknown exception".to_string()
            };

            log::error!("failed to call trigger: {}", exception_str);
            Some(exception_str)
        }
    }
}
