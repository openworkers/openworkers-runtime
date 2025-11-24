use std::rc::Rc;

use bytes::Bytes;
use deno_core::OpState;
use deno_core::ResourceId;
use deno_core::error::ResourceError;
use deno_core::op2;
use deno_core::serde::Deserialize;
use deno_core::serde::Serialize;
use log::debug;

// Use our shared types
type HttpRequest = crate::HttpRequest;
type HttpResponse = crate::HttpResponse;
type ResponseSender = tokio::sync::oneshot::Sender<HttpResponse>;

/// FetchResponse is a struct that represents the response
/// from a fetch request that comes from js realm.
#[derive(Debug, Deserialize)]
pub struct FetchResponse {
    status: u16,

    #[serde(rename = "headerList")]
    headers: Vec<(String, String)>,

    body: Option<Bytes>,
}

impl Into<crate::HttpResponse> for FetchResponse {
    fn into(self) -> crate::HttpResponse {
        crate::HttpResponse {
            status: self.status,
            headers: self.headers,
            body: self.body,
        }
    }
}

#[derive(Debug)]
pub struct FetchInit {
    pub(crate) req: crate::HttpRequest,
    pub(crate) res_tx: ResponseSender,
}

impl FetchInit {
    pub fn new(req: crate::HttpRequest, res_tx: ResponseSender) -> Self {
        FetchInit { req, res_tx }
    }
}

impl deno_core::Resource for FetchInit {
    fn close(self: Rc<Self>) {
        println!("TODO Resource.close impl for FetchInit"); // TODO
    }
}

#[derive(Debug)]
struct FetchTx(ResponseSender);

impl deno_core::Resource for FetchTx {
    fn close(self: Rc<Self>) {
        println!("TODO Resource.close impl for FetchTx"); // TODO
    }
}

impl FetchTx {
    pub fn send(self, res: FetchResponse) -> Result<(), HttpResponse> {
        self.0.send(res.into())
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

impl From<HttpRequest> for InnerRequest {
    fn from(req: HttpRequest) -> Self {
        InnerRequest {
            method: req.method,
            url: req.url,
            headers: req.headers.into_iter().collect(),
            body: req.body,
        }
    }
}

deno_core::extension!(
    fetch_event,
    deps = [deno_web, deno_fetch],
    ops = [op_fetch_init, op_fetch_respond],
    esm = ["ext:event_fetch.js" = "src/ext/event_fetch.js",]
);

#[op2]
#[serde]
fn op_fetch_init(state: &mut OpState, #[smi] rid: ResourceId) -> Result<FetchEvent, ResourceError> {
    debug!("op_fetch_init {rid}");

    let evt = state.resource_table.take::<FetchInit>(rid).unwrap();

    let evt = Rc::try_unwrap(evt).unwrap();

    let req = InnerRequest::from(evt.req);

    let rid = state.resource_table.add(FetchTx(evt.res_tx));

    Ok(FetchEvent { req, rid })
}

#[op2]
#[serde]
fn op_fetch_respond(
    state: &mut OpState,
    #[smi] rid: ResourceId,
    #[serde] res: FetchResponse,
) -> Result<(), ResourceError> {
    debug!("op_fetch_respond with status {}", res.status);

    let tx = match state.resource_table.take::<FetchTx>(rid) {
        Ok(tx) => tx,
        Err(err) => return Err(err),
    };

    let tx = Rc::try_unwrap(tx).unwrap();

    let tx = tx.send(res);
    debug!("op_fetch_respond tx {:?}", tx);

    Ok(())
}
