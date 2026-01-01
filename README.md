# OpenWorkers Runtime - Deno

The original JavaScript runtime for OpenWorkers based on [deno_core](https://github.com/denoland/deno_core) - featuring V8 with selected Deno extensions for Web API support.

## Features

- ✅ **Deno Extensions** - Lightweight selection of deno\_\* extensions
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
Worker::new(): avg=7.4ms, min=4.6ms, max=17.1ms
exec():        avg=1.5ms, min=1.07ms, max=2.7ms
Total:         avg=9ms, min=5.8ms, max=20ms
```

### Runtime Comparison (v0.5.0)

| Runtime | Engine | Worker::new() | exec_simple | exec_json | Tests |
|---------|--------|---------------|-------------|-----------|-------|
| **[QuickJS](https://github.com/openworkers/openworkers-runtime-quickjs)** | QuickJS | 738µs | **12.4µs** ⚡ | **13.7µs** | 16/17 |
| **[V8](https://github.com/openworkers/openworkers-runtime-v8)** | V8 | 790µs | 32.3µs | 34.3µs | **17/17** |
| **[JSC](https://github.com/openworkers/openworkers-runtime-jsc)** | JavaScriptCore | 1.07ms | 30.3µs | 28.3µs | 15/17 |
| **[Deno](https://github.com/openworkers/openworkers-runtime-deno)** | V8 + Deno | 2.56ms | 46.8µs | 38.7µs | **17/17** |
| **[Boa](https://github.com/openworkers/openworkers-runtime-boa)** | Boa | 738µs | 12.4µs | 13.7µs | 13/17 |

**Deno provides the most complete Web API compatibility** (17/17 tests) with rich Deno extensions.

## Installation

```toml
[dependencies]
openworkers-runtime-deno = "0.5"
```

## Usage

```rust
use openworkers_core::{Script, Task, HttpRequest, FetchInit};
use openworkers_runtime_deno::Worker;

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
