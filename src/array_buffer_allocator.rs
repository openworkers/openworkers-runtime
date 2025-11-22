use deno_core::v8;
use deno_core::v8::UniqueRef;
use std::ffi::c_void;
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
}

impl CustomAllocator {
    pub fn new(max_bytes: usize) -> Arc<Self> {
        Arc::new(Self {
            max: max_bytes,
            count: AtomicUsize::new(0),
        })
    }

    pub fn into_v8_allocator(self: Arc<Self>) -> UniqueRef<v8::Allocator> {
        let vtable: &'static v8::RustAllocatorVtable<CustomAllocator> =
            &v8::RustAllocatorVtable {
                allocate,
                allocate_uninitialized,
                free,
                reallocate,
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
        // Rollback the count since we're not actually allocating
        allocator.count.fetch_sub(n, Ordering::SeqCst);
        return std::ptr::null::<*mut [u8]>() as *mut c_void;
    }

    Box::into_raw(vec![0u8; n].into_boxed_slice()) as *mut [u8] as *mut c_void
}

#[allow(clippy::unnecessary_cast)]
#[allow(clippy::uninit_vec)]
unsafe extern "C" fn allocate_uninitialized(
    allocator: &CustomAllocator,
    n: usize,
) -> *mut c_void {
    allocator.count.fetch_add(n, Ordering::SeqCst);

    let count_loaded = allocator.count.load(Ordering::SeqCst);

    if count_loaded > allocator.max {
        log::warn!(
            "ArrayBuffer allocation denied: {}MB exceeds limit of {}MB",
            count_loaded / 1024 / 1024,
            allocator.max / 1024 / 1024
        );
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

#[allow(clippy::unnecessary_cast)]
unsafe extern "C" fn reallocate(
    allocator: &CustomAllocator,
    prev: *mut c_void,
    oldlen: usize,
    newlen: usize,
) -> *mut c_void {
    allocator
        .count
        .fetch_add(newlen.wrapping_sub(oldlen), Ordering::SeqCst);

    let count_loaded = allocator.count.load(Ordering::SeqCst);

    if count_loaded > allocator.max {
        log::warn!(
            "ArrayBuffer reallocation denied: {}MB exceeds limit of {}MB",
            count_loaded / 1024 / 1024,
            allocator.max / 1024 / 1024
        );
        // Rollback
        allocator
            .count
            .fetch_sub(newlen.wrapping_sub(oldlen), Ordering::SeqCst);
        return std::ptr::null::<*mut [u8]>() as *mut c_void;
    }

    let old_store = Box::from_raw(std::slice::from_raw_parts_mut(prev as *mut u8, oldlen));
    let mut new_store = Vec::with_capacity(newlen);
    let copy_len = oldlen.min(newlen);

    new_store.extend_from_slice(&old_store[..copy_len]);
    new_store.resize(newlen, 0u8);

    Box::into_raw(new_store.into_boxed_slice()) as *mut [u8] as *mut c_void
}

unsafe extern "C" fn drop(allocator: *const CustomAllocator) {
    Arc::from_raw(allocator);
}
