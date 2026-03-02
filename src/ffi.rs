//! C-compatible FFI layer for use from JNI or other foreign callers.
//!
//! Safety contract:
//! - All pointer arguments must be valid (non-null) unless documented otherwise.
//! - Strings returned by `tentoku_*` functions must be freed with `tentoku_free_string`.
//! - Handles returned by `tentoku_open` must be freed with `tentoku_free`.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;

use crate::normalize::normalize_input;
use crate::sqlite_dict::SqliteDictionary;
use crate::tokenizer::tokenize;
use crate::word_search::word_search;

/// Opaque handle wrapping an open `SqliteDictionary`.
pub struct TentokuHandle {
    dict: SqliteDictionary,
}

/// Open a dictionary database at `db_path`.
///
/// Returns a non-null handle on success, or null on failure.
/// The caller must free the handle with [`tentoku_free`].
///
/// # Safety
/// `db_path` must be a valid, non-null, null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tentoku_open(db_path: *const c_char) -> *mut TentokuHandle {
    if db_path.is_null() {
        return std::ptr::null_mut();
    }
    let path_str = match CStr::from_ptr(db_path).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    match SqliteDictionary::open(Path::new(path_str)) {
        Ok(dict) => Box::into_raw(Box::new(TentokuHandle { dict })),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free a handle previously returned by [`tentoku_open`].
///
/// # Safety
/// `handle` must be a valid pointer previously returned by `tentoku_open`,
/// or null (no-op).
#[no_mangle]
pub unsafe extern "C" fn tentoku_free(handle: *mut TentokuHandle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// Tokenize `text` and return a JSON string.
///
/// Returns null on error. The caller must free the result with [`tentoku_free_string`].
///
/// # Safety
/// - `handle` must be a valid non-null pointer returned by `tentoku_open`.
/// - `text` must be a valid non-null, null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tentoku_tokenize_json(
    handle: *const TentokuHandle,
    text: *const c_char,
    max_results: u32,
) -> *mut c_char {
    if handle.is_null() || text.is_null() {
        return std::ptr::null_mut();
    }
    let text_str = match CStr::from_ptr(text).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let dict = &(*handle).dict;
    let tokens = tokenize(text_str, dict, max_results as usize);
    let json = match serde_json::to_string(&tokens) {
        Ok(j) => j,
        Err(_) => return std::ptr::null_mut(),
    };
    match CString::new(json) {
        Ok(cs) => cs.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Look up `word` and return matching entries as a JSON string.
///
/// Returns null if no match or on error. The caller must free the result with
/// [`tentoku_free_string`].
///
/// # Safety
/// - `handle` must be a valid non-null pointer returned by `tentoku_open`.
/// - `word` must be a valid non-null, null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tentoku_lookup_json(
    handle: *const TentokuHandle,
    word: *const c_char,
    max_results: u32,
) -> *mut c_char {
    if handle.is_null() || word.is_null() {
        return std::ptr::null_mut();
    }
    let word_str = match CStr::from_ptr(word).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let dict = &(*handle).dict;
    let (normalized, lengths) = normalize_input(word_str);
    let result = match word_search(&normalized, dict, max_results as usize, Some(&lengths)) {
        Some(r) => r,
        None => return std::ptr::null_mut(),
    };
    let json = match serde_json::to_string(&result.data) {
        Ok(j) => j,
        Err(_) => return std::ptr::null_mut(),
    };
    match CString::new(json) {
        Ok(cs) => cs.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free a string previously returned by any `tentoku_*_json` function.
///
/// # Safety
/// `s` must be a pointer returned by a `tentoku_*_json` function, or null (no-op).
#[no_mangle]
pub unsafe extern "C" fn tentoku_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}
