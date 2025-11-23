use deno_core::v8;
use deno_core::v8::UniqueRef;
use std::ffi::c_void;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// Custom ArrayBuffer allocator that tracks and limits external memory
///
/// This allocator wraps V8's ArrayBuffer allocations (Uint8Array, Buffer, etc.)
/// and enforces a hard memory limit. When the limit is exceeded, allocations
/// return NULL, causing V8 to throw a RangeError.
///
/// This is critical for preventing memory bombs from ArrayBuffer allocations,
/// which are NOT covered by V8's heap limits.
pub struct CustomAllocator {
    max: usize,
    count: AtomicUsize,
    memory_limit_hit: Arc<AtomicBool>,
}

impl CustomAllocator {
    pub fn new(max_bytes: usize, memory_limit_hit: Arc<AtomicBool>) -> Arc<Self> {
        Arc::new(Self {
            max: max_bytes,
            count: AtomicUsize::new(0),
            memory_limit_hit,
        })
    }

    pub fn into_v8_allocator(self: Arc<Self>) -> UniqueRef<v8::Allocator> {
        let vtable: &'static v8::RustAllocatorVtable<CustomAllocator> = &v8::RustAllocatorVtable {
            allocate,
            allocate_uninitialized,
            free,
            drop,
        };

        unsafe { v8::new_rust_allocator(Arc::into_raw(self), vtable) }
    }

    #[allow(dead_code)]
    pub fn current_usage(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

#[allow(clippy::unnecessary_cast)]
unsafe extern "C" fn allocate(allocator: &CustomAllocator, n: usize) -> *mut c_void {
    allocator.count.fetch_add(n, Ordering::SeqCst);

    let count_loaded = allocator.count.load(Ordering::SeqCst);

    if count_loaded > allocator.max {
        log::warn!(
            "ArrayBuffer allocation denied: {}MB exceeds limit of {}MB",
            count_loaded / 1024 / 1024,
            allocator.max / 1024 / 1024
        );
        // Set the flag on the allocator instance to indicate memory limit was hit
        allocator.memory_limit_hit.store(true, Ordering::SeqCst);
        // Rollback the count since we're not actually allocating
        allocator.count.fetch_sub(n, Ordering::SeqCst);
        return std::ptr::null::<*mut [u8]>() as *mut c_void;
    }

    Box::into_raw(vec![0u8; n].into_boxed_slice()) as *mut [u8] as *mut c_void
}

#[allow(clippy::unnecessary_cast)]
#[allow(clippy::uninit_vec)]
unsafe extern "C" fn allocate_uninitialized(allocator: &CustomAllocator, n: usize) -> *mut c_void {
    allocator.count.fetch_add(n, Ordering::SeqCst);

    let count_loaded = allocator.count.load(Ordering::SeqCst);

    if count_loaded > allocator.max {
        log::warn!(
            "ArrayBuffer allocation denied: {}MB exceeds limit of {}MB",
            count_loaded / 1024 / 1024,
            allocator.max / 1024 / 1024
        );
        // Set the flag on the allocator instance to indicate memory limit was hit
        allocator.memory_limit_hit.store(true, Ordering::SeqCst);
        allocator.count.fetch_sub(n, Ordering::SeqCst);
        return std::ptr::null::<*mut [u8]>() as *mut c_void;
    }

    let mut store = Vec::with_capacity(n);
    store.set_len(n);

    Box::into_raw(store.into_boxed_slice()) as *mut [u8] as *mut c_void
}

unsafe extern "C" fn free(allocator: &CustomAllocator, data: *mut c_void, n: usize) {
    allocator.count.fetch_sub(n, Ordering::SeqCst);
    let _ = Box::from_raw(std::slice::from_raw_parts_mut(data as *mut u8, n));
}

unsafe extern "C" fn drop(allocator: *const CustomAllocator) {
    Arc::from_raw(allocator);
}
