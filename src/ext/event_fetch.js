import { core } from "ext:core/mod.js";
import { newSignal } from "ext:deno_web/03_abort_signal.js";
import {
  guardFromHeaders,
  headersFromHeaderList,
} from "ext:deno_fetch/20_headers.js";
import { InnerBody } from "ext:deno_fetch/22_body.js";
import {
  newInnerRequest,
  fromInnerRequest,
} from "ext:deno_fetch/23_request.js";
import { toInnerResponse, Response } from "ext:deno_fetch/23_response.js";

import {
  op_fetch_init,
  op_fetch_respond,
  op_fetch_respond_stream_start,
  op_fetch_respond_stream_chunk,
  op_fetch_respond_stream_end,
} from "ext:core/ops";

let fetchEventListener;

function registerFetchEventListener(listener) {
  if (typeof listener !== "function") {
    throw new TypeError("Listener must be a function");
  }

  fetchEventListener = listener;
}

function triggerFetchEvent(rid) {
  if (!fetchEventListener) {
    throw new Error("No fetch event listener registered");
  }

  const evt = op_fetch_init(rid);

  const signal = newSignal();

  // Request body is always buffered (streaming input not supported)
  const innerBody = evt.req.body
    ? new InnerBody({ body: evt.req.body, consumed: false })
    : null;

  const inner = newInnerRequest(
    evt.req.method,
    () => evt.req.url,
    () => evt.req.headers,
    innerBody
  );

  const guard = guardFromHeaders(headersFromHeaderList(inner.headerList));

  fetchEventListener({
    request: fromInnerRequest(inner, signal, guard),
    respondWith: async (resOrPromise) => {
      let response = core.isPromise(resOrPromise)
        ? await resOrPromise
        : resOrPromise;

      if (!(response instanceof Response)) {
        throw new TypeError("Response must be a Response object");
      }

      const inner = toInnerResponse(response);

      const meta = { status: inner.status, headerList: inner.headerList };

      if (response.body) {
        // Stream the response body
        const streamRid = op_fetch_respond_stream_start(evt.rid, meta);

        try {
          const reader = response.body.getReader();
          while (true) {
            const { value, done } = await reader.read();
            if (done) break;
            if (value && value.length > 0) {
              await op_fetch_respond_stream_chunk(streamRid, value);
            }
          }
        } finally {
          op_fetch_respond_stream_end(streamRid);
        }
      } else {
        // No body
        op_fetch_respond(evt.rid, meta, null);
      }
    },
  });
}

export { triggerFetchEvent, registerFetchEventListener };
