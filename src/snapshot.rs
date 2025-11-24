use deno_core::error::CoreError;
use deno_core::snapshot::CreateSnapshotOptions;
use deno_core::snapshot::CreateSnapshotOutput;
use deno_core::snapshot::create_snapshot;

use crate::extensions;

pub fn create_runtime_snapshot() -> Result<CreateSnapshotOutput, CoreError> {
    println!("Building snapshot");

    let options = CreateSnapshotOptions {
        cargo_manifest_dir: env!("CARGO_MANIFEST_DIR"),
        startup_snapshot: None,
        extensions: extensions(false),
        skip_op_registration: false,
        extension_transpiler: None,
        with_runtime_cb: None,
    };

    // Create the snapshot.
    let snapshot = create_snapshot(options, None)?;

    Ok(snapshot)
}
