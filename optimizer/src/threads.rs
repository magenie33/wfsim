// SPDX-License-Identifier: AGPL-3.0-or-later
//! The worker-thread budget, and the wasm heartbeat that stands in for
//! threads in a single-threaded Web Worker.

use std::sync::atomic::{AtomicUsize, Ordering};

/// Worker-thread budget for [`evaluate_batch`] and the search. 0 =
/// auto: ALL CORES MINUS TWO — the optimizer must not freeze the machine
/// it runs on (the full-core default made the whole
/// system stutter). Set per request via [`set_worker_threads`]; the seeds
/// never depend on the thread count, so any setting reproduces the same
/// numbers.
static WORKER_THREADS: AtomicUsize = AtomicUsize::new(0);

pub fn set_worker_threads(n: usize) {
    WORKER_THREADS.store(n, Ordering::Relaxed);
}

/// How many proposals the search evaluates before it looks at the results.
/// Wide enough to keep every worker fed, small enough that the neighbourhood
/// still gets to steer often — a batch is the granularity of the feedback
/// loop, so making it huge would turn the climb back into sampling.
pub(crate) fn batch_width() -> usize {
    #[cfg(not(target_arch = "wasm32"))]
    {
        (worker_threads() * 4).clamp(16, 256)
    }
    #[cfg(target_arch = "wasm32")]
    {
        16
    }
}

#[cfg(not(target_arch = "wasm32"))] // wasm is single-threaded; the budget is native-only
pub(crate) fn worker_threads() -> usize {
    let n = WORKER_THREADS.load(Ordering::Relaxed);
    if n > 0 {
        return n;
    }
    std::thread::available_parallelism()
        .map(|c| c.get())
        .unwrap_or(8)
        .saturating_sub(2)
        .max(1)
}

/// Drop the CURRENT thread to below-normal scheduling priority (Windows;
/// no-op elsewhere): the optimizer soaks idle cycles at full speed but
/// yields the moment the user does anything interactive.
pub fn deprioritize_current_thread() {
    #[cfg(windows)]
    unsafe {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentThread() -> *mut core::ffi::c_void;
            fn SetThreadPriority(h: *mut core::ffi::c_void, p: i32) -> i32;
        }
        const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

// ---- wasm busy-loop progress hook --------------------------------------
// A single-threaded Web Worker cannot be polled while it computes — before
// this hook, the whole enumeration/screen phase was SILENT and a big scope
// looked dead. The hot loops
// call `tick()`; the wasm host installs a throttled hook that posts live
// status out of the worker. Native builds compile `tick()` to a no-op —
// the status endpoint polls `FunnelState` instead.
#[cfg(target_arch = "wasm32")]
thread_local! {
    static TICK_HOOK: std::cell::RefCell<Option<Box<dyn Fn()>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
pub fn set_tick_hook(hook: Option<Box<dyn Fn()>>) {
    TICK_HOOK.with(|h| *h.borrow_mut() = hook);
}

/// Native no-op twin — lets the wasm host crate compile for the host
/// target too (the workspace builds it there for tests/clippy).
#[cfg(not(target_arch = "wasm32"))]
pub fn set_tick_hook(_hook: Option<Box<dyn Fn()>>) {}

#[inline]
pub fn tick() {
    #[cfg(target_arch = "wasm32")]
    TICK_HOOK.with(|h| {
        if let Some(f) = h.borrow().as_ref() {
            f();
        }
    });
}
