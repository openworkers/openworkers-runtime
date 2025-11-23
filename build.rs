use std::fs::File;
use std::path::PathBuf;

const RUNTIME_SNAPSHOT_PATH: &str = env!("RUNTIME_SNAPSHOT_PATH");

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/ext/*");
    println!("cargo:rerun-if-changed=src/runtime.rs");
    println!("cargo:rerun-if-changed=src/extensions.rs");

    let path = PathBuf::from(RUNTIME_SNAPSHOT_PATH);

    // Create the file if it doesn't exist
    if !path.exists() {
        println!("Creating empty snapshot file: {:?}", path);
        File::create(&path).unwrap();
    }
}
