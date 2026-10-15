use crush_ffi::{CrushPlugin, CrushPluginExport, FfiString, FfiType, FfiValue, FfiValueData};
use std::{cell::RefCell, ffi::CString};

thread_local! {
    // The host copies the result immediately after the callback returns. Keep
    // one result buffer per thread and replace it on the next call.
    static RETURN_BUFFER: RefCell<CString> = RefCell::new(CString::default());
}

extern "C" fn greet_impl(
    args: *const FfiValue,
    arg_count: usize,
    out_result: *mut FfiValue,
) -> bool {
    let name = if arg_count > 0 {
        let first_arg = unsafe { &*args };
        if let FfiType::String = first_arg.tag {
            let str_val = unsafe { first_arg.data.string };
            str_val.as_str().unwrap_or("World")
        } else {
            "World"
        }
    } else {
        "World"
    };

    let msg = format!("Hello, {} from Rust Extension!", name);
    RETURN_BUFFER
        .try_with(|buffer| {
            let mut buffer = buffer.borrow_mut();
            *buffer = CString::new(msg).expect("greeting has no NUL bytes");
            let string = FfiString {
                ptr: buffer.as_ptr(),
                len: buffer.as_bytes().len(),
            };
            unsafe {
                *out_result = FfiValue {
                    tag: FfiType::String,
                    data: FfiValueData { string },
                };
            }
        })
        .is_ok()
}

extern "C" fn version_impl(
    _args: *const FfiValue,
    arg_count: usize,
    out_result: *mut FfiValue,
) -> bool {
    if arg_count != 0 {
        return false;
    }
    unsafe {
        *out_result = FfiValue::from_int(500);
    }
    true
}

static EXPORTS: &[CrushPluginExport] = &[
    CrushPluginExport {
        name: c"rust.greet".as_ptr(),
        func: greet_impl,
    },
    CrushPluginExport {
        name: c"rust.version".as_ptr(),
        func: version_impl,
    },
];

static PLUGIN: CrushPlugin = CrushPlugin {
    plugin_name: c"example_plugin".as_ptr(),
    exports: EXPORTS.as_ptr(),
    export_count: 2,
};

#[unsafe(no_mangle)]
pub extern "C" fn crush_plugin_init() -> *const CrushPlugin {
    &PLUGIN as *const CrushPlugin
}
