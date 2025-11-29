use std::cell::RefCell;
use std::rc::Rc;

use bytes::Bytes;
use deno_core::JsBuffer;
use deno_core::OpState;
use deno_core::ResourceId;
use deno_core::error::ResourceError;
use deno_core::op2;
use deno_core::serde::Deserialize;
use deno_core::serde::Serialize;
use log::debug;
use openworkers_core::FetchInit;
use openworkers_core::HttpRequest;
use openworkers_core::ResponseSender;
use tokio::sync::mpsc;

/// Buffer size for streaming response channel
const STREAM_BUFFER_SIZE: usize = 16;

/// Response metadata (status + headers), used for both buffered and streaming responses
#[derive(Debug, Deserialize)]
pub struct ResponseMeta {
    status: u16,

    #[serde(rename = "headerList")]
    headers: Vec<(String, String)>,
}

#[derive(Debug)]
struct FetchTx(ResponseSender);

impl deno_core::Resource for FetchTx {
    fn close(self: Rc<Self>) {
        // Response sender dropped without sending - this is fine, request was likely cancelled
    }
}

/// Resource for streaming response body chunks
/// Holds the sender side of the mpsc channel
#[derive(Debug)]
struct FetchStreamTx(mpsc::Sender<Result<Bytes, String>>);

impl deno_core::Resource for FetchStreamTx {
    fn close(self: Rc<Self>) {
        // Sender dropped - stream ends
    }
}

#[derive(Debug, Serialize)]
struct InnerRequest {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<Bytes>,
}

#[derive(Debug, Serialize)]
struct FetchEvent {
    req: InnerRequest,
    rid: u32,
}

fn convert_request(req: HttpRequest, _state: &mut OpState) -> InnerRequest {
    use openworkers_core::RequestBody;
    let body = match req.body {
        RequestBody::Bytes(b) => Some(b),
        RequestBody::None => None,
    };
    InnerRequest {
        method: req.method.to_string(),
        url: req.url,
        headers: req.headers.into_iter().collect(),
        body,
    }
}

deno_core::extension!(
    fetch_event,
    deps = [deno_web, deno_fetch],
    ops = [
        op_fetch_init,
        op_fetch_respond,
        op_fetch_respond_stream_start,
        op_fetch_respond_stream_chunk,
        op_fetch_respond_stream_end,
    ],
    esm = ["ext:event_fetch.js" = "src/ext/event_fetch.js",]
);

#[op2]
#[serde]
fn op_fetch_init(state: &mut OpState, #[smi] rid: ResourceId) -> Result<FetchEvent, ResourceError> {
    debug!("op_fetch_init {rid}");

    let evt = state.resource_table.take::<FetchInit>(rid).unwrap();

    let evt = Rc::try_unwrap(evt).unwrap();

    let req = convert_request(evt.req, state);

    let rid = state.resource_table.add(FetchTx(evt.res_tx));

    Ok(FetchEvent { req, rid })
}

/// Send a complete (buffered) response
#[op2]
fn op_fetch_respond(
    state: &mut OpState,
    #[smi] rid: ResourceId,
    #[serde] meta: ResponseMeta,
    #[buffer] body: Option<JsBuffer>,
) -> Result<(), ResourceError> {
    debug!("op_fetch_respond with status {}", meta.status);

    let tx = state.resource_table.take::<FetchTx>(rid)?;
    let tx = Rc::try_unwrap(tx).unwrap();

    let response = crate::HttpResponse {
        status: meta.status,
        headers: meta.headers,
        body: match body {
            Some(buf) => crate::ResponseBody::Bytes(Bytes::copy_from_slice(&buf)),
            None => crate::ResponseBody::None,
        },
    };

    let _ = tx.0.send(response);

    Ok(())
}

/// Start a streaming response - sends headers and returns a stream rid for sending chunks
#[op2]
#[smi]
fn op_fetch_respond_stream_start(
    state: &mut OpState,
    #[smi] rid: ResourceId,
    #[serde] meta: ResponseMeta,
) -> Result<ResourceId, ResourceError> {
    debug!("op_fetch_respond_stream_start with status {}", meta.status);

    let tx = state.resource_table.take::<FetchTx>(rid)?;
    let tx = Rc::try_unwrap(tx).unwrap();

    // Create channel for streaming body
    let (body_tx, body_rx) = mpsc::channel(STREAM_BUFFER_SIZE);

    // Build response with streaming body
    let response = crate::HttpResponse {
        status: meta.status,
        headers: meta.headers,
        body: crate::ResponseBody::Stream(body_rx),
    };

    // Send response immediately (headers + stream receiver)
    let _ = tx.0.send(response);

    // Store sender for subsequent chunk ops
    let stream_rid = state.resource_table.add(FetchStreamTx(body_tx));

    debug!(
        "op_fetch_respond_stream_start created stream rid {}",
        stream_rid
    );

    Ok(stream_rid)
}

/// Send a chunk of data for a streaming response
/// Note: This op cannot fail in a way that needs error propagation to JS,
/// so we just log errors and return Ok(())
#[op2(async)]
async fn op_fetch_respond_stream_chunk(
    state: Rc<RefCell<OpState>>,
    #[smi] rid: ResourceId,
    #[buffer] chunk: JsBuffer,
) -> Result<(), ResourceError> {
    let tx = {
        let state = state.borrow();
        let resource = state.resource_table.get::<FetchStreamTx>(rid)?;
        resource.0.clone()
    };

    debug!(
        "op_fetch_respond_stream_chunk sending {} bytes",
        chunk.len()
    );

    if let Err(e) = tx.send(Ok(Bytes::copy_from_slice(&chunk))).await {
        log::error!("Failed to send stream chunk: {}", e);
    }

    Ok(())
}

/// End a streaming response
#[op2(fast)]
fn op_fetch_respond_stream_end(
    state: &mut OpState,
    #[smi] rid: ResourceId,
) -> Result<(), ResourceError> {
    debug!("op_fetch_respond_stream_end for rid {}", rid);

    // Take and drop the sender - this closes the channel
    let _ = state.resource_table.take::<FetchStreamTx>(rid)?;

    Ok(())
}
