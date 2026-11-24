//! Observe the largest allocation request of one test thread, for byte-limit controls.
//!
//! A test binary that includes this file with `#[path]` gets a system-backed
//! global allocator. It records sizes only while [`largest_allocation`] runs.

/// Observe allocation requests on the enabled test thread while forwarding to System.
struct ObservedAllocator;
thread_local! {
    /// Largest allocation request while this thread enables observation, otherwise disabled.
    static ALLOCATION_MAX: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}
unsafe impl std::alloc::GlobalAlloc for ObservedAllocator {
    /// Record the requested size and allocate through the system allocator.
    ///
    /// # Safety
    /// The caller must supply the valid nonzero layout required by `GlobalAlloc::alloc`.
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        ALLOCATION_MAX.with(|max| {
            if let Some(previous) = max.get() {
                max.set(Some(previous.max(layout.size())));
            }
        });
        // SAFETY: The GlobalAlloc caller supplies a valid layout, forwarded unchanged.
        // Observation uses a const, no-Drop thread-local Cell and only get/set/max;
        // it neither allocates nor unwinds before forwarding to System.
        unsafe { std::alloc::System.alloc(layout) }
    }
    /// Release a forwarded system allocation without changing its layout.
    ///
    /// # Safety
    /// The pointer must be live from this allocator and match the supplied layout.
    unsafe fn dealloc(&self, pointer: *mut u8, layout: std::alloc::Layout) {
        // SAFETY: All allocations are forwarded to System. The GlobalAlloc caller
        // supplies its live allocation pointer and matching layout, unchanged here.
        unsafe { std::alloc::System.dealloc(pointer, layout) }
    }
    /// Record the replacement size and resize a forwarded system allocation.
    ///
    /// # Safety
    /// The pointer and layout must describe a live allocation; size must satisfy
    /// the nonzero and layout bounds required by `GlobalAlloc::realloc`.
    unsafe fn realloc(&self, pointer: *mut u8, layout: std::alloc::Layout, size: usize) -> *mut u8 {
        ALLOCATION_MAX.with(|max| {
            if let Some(previous) = max.get() {
                max.set(Some(previous.max(size)));
            }
        });
        // SAFETY: The GlobalAlloc caller supplies a live System allocation, its
        // matching layout and a valid new size; all are forwarded unchanged.
        // Observation uses a const, no-Drop thread-local Cell and only get/set/max;
        // it neither allocates nor unwinds before forwarding to System.
        unsafe { std::alloc::System.realloc(pointer, layout, size) }
    }
}
/// System-backed allocator that observes only explicitly enabled test threads.
#[global_allocator]
static ALLOCATOR: ObservedAllocator = ObservedAllocator;

/// Run `work` and return its result with the largest allocation request it made on this thread.
///
/// # Arguments
/// * `work` - Code whose allocation requests are observed.
///
/// # Returns
/// The result of `work` and the largest requested size in bytes, or 0 when it made none.
pub fn largest_allocation<T>(work: impl FnOnce() -> T) -> (T, usize) {
    ALLOCATION_MAX.with(|max| max.set(Some(0)));
    let result = work();
    let largest = ALLOCATION_MAX.with(|max| max.replace(None).unwrap_or(0));
    (result, largest)
}
