use crush_ffi::{CrushPlugin, CrushPluginExport, CrushPluginFunc, FfiType, FfiValue, FfiValueData, FfiString};
use std::ffi::CString;

extern "C" fn greet_impl(args: *const FfiValue, arg_count: usize, out_result: *mut FfiValue) -> bool {
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
    // Leak the CString so the VM can read it. In a real system, the VM would copy and we would free.
    let cstr = CString::new(msg).unwrap();
    let bytes = cstr.into_bytes_with_nul();
    let ptr = bytes.as_ptr() as *const std::ffi::c_char;
    let len = bytes.len() - 1;
    std::mem::forget(bytes);

    unsafe {
        *out_result = FfiValue {
            tag: FfiType::String,
            data: FfiValueData {
                string: FfiString { ptr, len }
            }
        };
    }
    true
}

static EXPORTS: &[CrushPluginExport] = &[
    CrushPluginExport {
        name: c"greet".as_ptr(),
        func: greet_impl,
    },
];

static PLUGIN: CrushPlugin = CrushPlugin {
    plugin_name: c"example_plugin".as_ptr(),
    exports: EXPORTS.as_ptr(),
    export_count: 1, // Array length
};

#[unsafe(no_mangle)]
pub extern "C" fn crush_plugin_init() -> *const CrushPlugin {
    &PLUGIN as *const CrushPlugin
}
