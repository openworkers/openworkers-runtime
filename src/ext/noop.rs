// Noop implementation of various files
deno_core::extension!(
    noop_ext,
    esm = [
        "ext:deno_telemetry/telemetry.ts" = {
            source = r#"
                export const TRACING_ENABLED = false;
                export const METRICS_ENABLED = false;
                export const PROPAGATORS = {};

                export const builtinTracer = () => {};
                export const enterSpan = () => {};
                export const restoreContext = () => {};
                export const restoreSnapshot = () => {};
                export const ContextManager = class ContextManager {};
            "#
        },
        "ext:deno_telemetry/util.ts" = {
            source = r#"
                export const updateSpanFromResponse = () => {};
                export const updateSpanFromRequest = () => {};
                export const updateSpanFromError = () => {};
            "#
        },
        "ext:deno_fetch/22_http_client.js" = {
            source = r#"
                export const HttpClientPrototype = (class HttpClient {}).prototype;
                export const createHttpClient = () => {};
            "#
        },
    ]
);
