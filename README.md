# OpenWorkers Runtime - Deno

The original JavaScript runtime for OpenWorkers based on [deno_core](https://github.com/denoland/deno_core) - featuring V8 with selected Deno extensions for Web API support.

## Features

- ✅ **Deno Extensions** - Lightweight selection of deno_* extensions
- ✅ **Complete Web APIs** - fetch, URL, crypto, console, and more
- ✅ **V8 Snapshots** - Fast startup with pre-compiled runtime
- ✅ **Resource Limits** - CPU time and memory enforcement
- ✅ **Event Handlers** - addEventListener('fetch'), addEventListener('scheduled')
- ✅ **Security** - Deno's permission system
- ✅ **Standards Compliant** - Maximum Web API compatibility

## Performance

Run benchmark:
```bash
cargo run --example benchmark --release
```

### Results (Apple Silicon, Release Mode)

```
Worker::new(): avg=21.9ms, min=15.3ms, max=36.6ms
exec():        avg=774µs, min=600µs, max=1.2ms
Total:         avg=22.7ms, min=15.9ms, max=37.9ms
```

### Runtime Comparison

| Runtime | Engine | Worker::new() | exec() | Total | Language |
|---------|--------|---------------|--------|-------|----------|
| **[V8](https://github.com/openworkers/openworkers-runtime-v8)** | V8 | 1.9ms | **96µs** ⚡ | 2.0ms | Rust + C++ |
| **[JSC](https://github.com/openworkers/openworkers-runtime-jsc)** | JavaScriptCore | 0.5ms* | 400µs | **0.9ms** 🏆 | Rust + C |
| **[Boa](https://github.com/openworkers/openworkers-runtime-boa)** | Boa | 1.1ms | 610µs | 1.7ms | 100% Rust |
| **[Deno](https://github.com/openworkers/openworkers-runtime)** | V8 + Deno | **21.9ms** | 774µs | 22.7ms | Rust + C++ |

*JSC has ~41ms warmup on first run, then stabilizes

**Deno provides the most complete Web API compatibility** at the cost of slower initialization.

## Installation

```toml
[dependencies]
openworkers-runtime = "0.2"
```

## Usage

```rust
use openworkers_runtime::{Worker, Script, Task, HttpRequest, FetchInit};

#[tokio::main]
async fn main() {
    let code = r#"
        addEventListener('fetch', async (event) => {
            // Full Deno Web APIs available
            const crypto = await crypto.subtle.digest('SHA-256', new TextEncoder().encode('hello'));
            event.respondWith(new Response('Hello from Deno!'));
        });
    "#;

    let script = Script {
        code: code.to_string(),
        env: None,
    };

    let mut worker = Worker::new(script, None, None).await.unwrap();

    let req = HttpRequest {
        method: "GET".to_string(),
        url: "http://localhost/".to_string(),
        headers: Default::default(),
        body: None,
    };

    let (res_tx, rx) = tokio::sync::oneshot::channel();
    let task = Task::Fetch(Some(FetchInit::new(req, res_tx)));

    worker.exec(task).await.unwrap();

    let response = rx.await.unwrap();
    println!("Status: {}", response.status);
}
```

## Testing

```bash
# Run all tests
cargo test

# Run resource limit tests
cargo test --test resource_limits
```

## Supported JavaScript APIs

### Deno Extensions
- `deno_console` - Full console API
- `deno_url` - Complete URL and URLSearchParams
- `deno_web` - Streams, TextEncoder/Decoder, crypto
- `deno_fetch` - Standards-compliant fetch
- `deno_crypto` - Web Crypto API

### Custom Extensions
- `addEventListener('fetch')` - HTTP request handler
- `addEventListener('scheduled')` - Scheduled event handler
- Resource limits (CPU time, memory)
- Custom permissions

## Building

```bash
# Build all examples
cargo build --release --examples

# Create snapshot
cargo run --bin snapshot

# Run demo server (new worker per request)
cargo run --example serve-new -- examples/serve.js

# Run demo server (same worker for all requests)
cargo run --example serve-same -- examples/serve.js

# Execute scheduled task
cargo run --example scheduled -- examples/scheduled.js
```

## Architecture

```
src/
├── lib.rs                  # Public API
├── runtime.rs              # Main runtime with Deno extensions
├── worker.rs               # Worker lifecycle
├── task.rs                 # Task types
├── termination.rs          # Termination reasons
├── snapshot.rs             # V8 snapshot support
├── timeout.rs              # Wall-clock timeout
├── cpu_timer.rs            # CPU time measurement
├── cpu_enforcement.rs      # CPU limit enforcement (Linux)
├── array_buffer_allocator.rs # Memory limit enforcement
└── ext/                    # Custom Deno extensions
    ├── fetch_event.rs
    ├── scheduled_event.rs
    ├── runtime.rs
    └── permissions.rs
```

## Key Advantages

- **Complete Web APIs** - Maximum compatibility with web standards
- **V8 Snapshots** - Fast subsequent startups after initial snapshot creation
- **Resource Enforcement** - CPU time limits (Linux), memory limits
- **Security** - Deno's permission system
- **Battle-tested** - Built on mature Deno extensions

## Trade-offs

- **Slower cold start** - ~22ms due to Deno extension initialization
- **More dependencies** - Uses deno_core + selected extensions (console, url, web, fetch, crypto)
- **Heavier runtime** - More features = more initialization overhead

## Other Runtime Implementations

OpenWorkers supports multiple JavaScript engines:

- **[openworkers-runtime](https://github.com/openworkers/openworkers-runtime)** - This runtime (Deno-based)
- **[openworkers-runtime-jsc](https://github.com/openworkers/openworkers-runtime-jsc)** - JavaScriptCore
- **[openworkers-runtime-boa](https://github.com/openworkers/openworkers-runtime-boa)** - Boa (100% Rust)
- **[openworkers-runtime-v8](https://github.com/openworkers/openworkers-runtime-v8)** - V8 via rusty_v8

## License

MIT License - See LICENSE file.

## Credits

Built on [Deno](https://deno.land) and the Deno extension ecosystem.
