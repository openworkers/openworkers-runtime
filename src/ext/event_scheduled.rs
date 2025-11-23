use std::rc::Rc;

use deno_core::error::ResourceError;
use deno_core::op2;
use deno_core::serde::Serialize;
use deno_core::OpState;
use deno_core::ResourceId;
use log::debug;

type ResponseSender = tokio::sync::oneshot::Sender<()>;

#[derive(Debug)]
pub struct ScheduledInit {
    pub(crate) res_tx: ResponseSender,
    pub(crate) time: u64,
}

impl ScheduledInit {
    pub fn new(res_tx: ResponseSender, time: u64) -> Self {
        ScheduledInit { res_tx, time }
    }
}

impl deno_core::Resource for ScheduledInit {
    fn close(self: Rc<Self>) {
        println!("TODO Resource.close impl for ScheduledInit"); // TODO
    }
}

#[derive(Debug, Serialize)]
struct ScheduledEvent {
    rid: u32,
    time: u64,
}

deno_core::extension!(
    scheduled_event,
    deps = [deno_web],
    ops = [op_scheduled_init, op_scheduled_respond],
    esm = ["ext:event_scheduled.js" = "./src/ext/event_scheduled.js",]
);

#[op2]
#[serde]
fn op_scheduled_init(
    state: &mut OpState,
    #[smi] rid: ResourceId,
) -> Result<ScheduledEvent, ResourceError> {
    debug!("op_scheduled_init {rid}");

    let evt = state.resource_table.get::<ScheduledInit>(rid).unwrap();

    let time = evt.time;

    Ok(ScheduledEvent { rid, time })
}

#[op2]
#[serde]
fn op_scheduled_respond(state: &mut OpState, #[smi] rid: ResourceId) -> Result<(), ResourceError> {
    debug!("op_scheduled_respond");

    match state.resource_table.take::<ScheduledInit>(rid) {
        Ok(tx) => Ok(Rc::try_unwrap(tx).unwrap().res_tx.send(()).unwrap()),
        Err(err) => Err(err),
    }
}
