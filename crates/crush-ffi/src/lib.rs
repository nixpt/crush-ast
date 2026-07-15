#![allow(improper_ctypes_definitions)]

use std::ffi::{c_char, CStr, CString};

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FfiType {
    Null = 0,
    Bool = 1,
    Int = 2,
    Float = 3,
    String = 4,
    Error = 5,
    Array = 6,
    Object = 7,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct FfiString {
    pub ptr: *const c_char,
    pub len: usize,
}

impl FfiString {
    pub fn as_str<'a>(self) -> Option<&'a str> {
        if self.ptr.is_null() || self.len == 0 {
            return Some("");
        }
        unsafe {
            let slice = std::slice::from_raw_parts(self.ptr as *const u8, self.len);
            std::str::from_utf8(slice).ok()
        }
    }

    /// Create an FfiString from a Rust &str (caller must keep source alive).
    pub fn from_str(s: &str) -> Self {
        FfiString {
            ptr: s.as_ptr() as *const c_char,
            len: s.len(),
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct FfiArray {
    pub ptr: *const FfiValue,
    pub len: usize,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct FfiObject {
    /// Opaque pointer to heap-allocated object data.
    pub ptr: *mut std::ffi::c_void,
    pub len: usize,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union FfiValueData {
    pub boolean: bool,
    pub integer: i64,
    pub float: f64,
    pub string: FfiString,
    pub array: FfiArray,
    pub object: FfiObject,
}

#[repr(C)]
pub struct FfiValue {
    pub tag: FfiType,
    pub data: FfiValueData,
}

impl Default for FfiValue {
    fn default() -> Self {
        FfiValue {
            tag: FfiType::Null,
            data: FfiValueData { integer: 0 },
        }
    }
}

impl FfiValue {
    pub fn null() -> Self { Self::default() }
    pub fn from_bool(b: bool) -> Self {
        FfiValue { tag: FfiType::Bool, data: FfiValueData { boolean: b } }
    }
    pub fn from_int(i: i64) -> Self {
        FfiValue { tag: FfiType::Int, data: FfiValueData { integer: i } }
    }
    pub fn from_float(f: f64) -> Self {
        FfiValue { tag: FfiType::Float, data: FfiValueData { float: f } }
    }
    pub fn from_string(s: &str) -> Self {
        FfiValue { tag: FfiType::String, data: FfiValueData { string: FfiString::from_str(s) } }
    }
    pub fn error(msg: &str) -> Self {
        FfiValue { tag: FfiType::Error, data: FfiValueData { string: FfiString::from_str(msg) } }
    }
}

/// A standard signature for an FFI exported function.
/// Returns true if successful, false if it threw an error (which is written to out_result).
pub type CrushPluginFunc = extern "C" fn(args: *const FfiValue, arg_count: usize, out_result: *mut FfiValue) -> bool;

#[repr(C)]
pub struct CrushPluginExport {
    pub name: *const c_char,
    pub func: CrushPluginFunc,
}

#[repr(C)]
pub struct CrushPlugin {
    pub plugin_name: *const c_char,
    pub exports: *const CrushPluginExport,
    pub export_count: usize,
}

unsafe impl Sync for CrushPluginExport {}
unsafe impl Sync for CrushPlugin {}
