// SPDX-License-Identifier: AGPL-3.0-or-later
//! wfsim-native: the engine as a library the desktop shell loads (docs/DESKTOP.md).
//!
//! ONE DOOR, THE WASM MODULE'S: [`wfsim_api`] is `api(path, body)`, JSON text
//! in and out, answered by `wfsim_webapi::route_text` exactly as the wasm's is,
//! so the two can only differ where the compiled arithmetic does — and the
//! engine's maths is one libm on both (`rules::math`). The shell holds this
//! file by a C ABI and nothing else, so it depends on nothing in the engine.

use std::panic::{catch_unwind, AssertUnwindSafe};

/// Faster than the process heap, and it moves no bit (native/Cargo.toml).
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// The answer to one call: JSON text the caller owns until it hands it to
/// [`wfsim_free`]. `len` is set to its length in bytes.
///
/// # Safety
/// `path` and `body` point to `path_len` and `body_len` readable bytes of
/// UTF-8, and `len` to a writable `usize`. A path or body that is not UTF-8
/// is answered with an error, as a panic inside the engine is.
#[no_mangle]
pub unsafe extern "C" fn wfsim_api(path: *const u8, path_len: usize, body: *const u8, body_len: usize, len: *mut usize) -> *mut u8 {
    let text = |p: *const u8, n: usize| if n == 0 { Some("") } else { std::str::from_utf8(std::slice::from_raw_parts(p, n)).ok() };
    let out = match (text(path, path_len), text(body, body_len)) {
        (Some(p), Some(b)) => catch_unwind(AssertUnwindSafe(|| wfsim_webapi::route_text(p, b)))
            .unwrap_or_else(|_| wfsim_webapi::err_json("the native engine panicked").to_string()),
        _ => wfsim_webapi::err_json("path or body is not UTF-8").to_string(),
    };
    let bytes = out.into_bytes().into_boxed_slice();
    *len = bytes.len();
    Box::into_raw(bytes).cast::<u8>()
}

/// Give back an answer of [`wfsim_api`].
///
/// # Safety
/// `ptr` and `len` are exactly what one call of [`wfsim_api`] returned, and
/// neither is used again.
#[no_mangle]
pub unsafe extern "C" fn wfsim_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(path: &str, body: &str) -> String {
        let mut len = 0;
        unsafe {
            let p = wfsim_api(path.as_ptr(), path.len(), body.as_ptr(), body.len(), &mut len);
            let s = std::str::from_utf8(std::slice::from_raw_parts(p, len)).unwrap().to_string();
            wfsim_free(p, len);
            s
        }
    }

    #[test]
    fn the_door_answers_what_route_text_answers() {
        for (path, body) in [("/api/meta", ""), ("/api/targets", "{}"), ("/api/nope", "{}"), ("/api/panel", "not json")] {
            assert_eq!(call(path, body), wfsim_webapi::route_text(path, body), "{path}");
        }
    }
}
