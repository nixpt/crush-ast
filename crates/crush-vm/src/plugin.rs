use std::ffi::CStr;
use std::collections::HashMap;
use libloading::{Library, Symbol};
use crate::host::{HostCap, HostCapSpec, HostCaps};
use crate::vm::Value;
use crush_ffi::{CrushPlugin, CrushPluginFunc, FfiArray, FfiObject, FfiType, FfiValue, FfiValueData, FfiString};

/// Wraps a C-FFI function pointer as a Crush HostCap
struct FfiHostCap {
    name: String,
    func: CrushPluginFunc,
}

impl HostCap for FfiHostCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: self.name.clone(),
            argc: None, // We support variadic for FFI
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let ffi_args: Vec<FfiValue> = args.into_iter().map(value_to_ffi).collect();
        
        // We will pass strings directly, but since we own the strings we need to ensure 
        // they live long enough. The FfiValue has pointers to them. 
        // In a real implementation we must carefully manage CString lifetimes here.
        // For this minimal MVP, we leak the string or rely on the caller to drop it,
        // but `value_to_ffi` currently leaks the CString memory to keep it safe for FFI.
        
        let mut out_result = FfiValue::default();
        let success = (self.func)(ffi_args.as_ptr(), ffi_args.len(), &mut out_result);
        
        if success {
            Ok(Some(ffi_to_value(out_result)))
        } else {
            let err_msg = match out_result.tag {
                FfiType::String | FfiType::Error => ffi_to_value(out_result).to_string(),
                _ => "Unknown FFI error".to_string(),
            };
            Err(err_msg)
        }
    }
}

fn value_to_ffi(val: Value) -> FfiValue {
    match val {
        Value::Null => FfiValue { tag: FfiType::Null, data: FfiValueData { integer: 0 } },
        Value::Bool(b) => FfiValue { tag: FfiType::Bool, data: FfiValueData { boolean: b } },
        Value::Int(i) => FfiValue { tag: FfiType::Int, data: FfiValueData { integer: i } },
        Value::Float(f) => FfiValue { tag: FfiType::Float, data: FfiValueData { float: f } },
        Value::Str(s) => {
            // Leak the string so FFI can read it. A better design frees it after call.
            let cstr = std::ffi::CString::new(s).unwrap();
            let bytes = cstr.into_bytes_with_nul();
            let ptr = bytes.as_ptr() as *const std::ffi::c_char;
            let len = bytes.len() - 1;
            // Leak the vector so it doesn't get dropped
            std::mem::forget(bytes);
            
            FfiValue { tag: FfiType::String, data: FfiValueData { string: FfiString { ptr, len } } }
        },
        Value::Array(arr) => {
            let a = arr.borrow();
            let mut ffi_vals: Vec<FfiValue> = a.iter().map(|v| value_to_ffi(v.clone())).collect();
            let ptr = ffi_vals.as_ptr();
            let len = ffi_vals.len();
            std::mem::forget(ffi_vals); // leak for FFI
            FfiValue { tag: FfiType::Array, data: FfiValueData { array: FfiArray { ptr, len } } }
        }
        Value::Map(m) => {
            let map = m.borrow();
            let keys: Vec<*const std::ffi::c_char> = map.keys().map(|k| {
                let c = std::ffi::CString::new(k.as_str()).unwrap();
                let p = c.as_ptr();
                std::mem::forget(c);
                p
            }).collect();
            let vals: Vec<FfiValue> = map.values().map(|v| value_to_ffi(v.clone())).collect();
            let kptr = keys.as_ptr();
            let vptr = vals.as_ptr();
            let len = keys.len();
            std::mem::forget(keys);
            std::mem::forget(vals);
            FfiValue { tag: FfiType::Object, data: FfiValueData { object: FfiObject { keys: kptr, values: vptr, len } } }
        }
        _ => FfiValue { tag: FfiType::Null, data: FfiValueData { integer: 0 } },
    }
}

fn ffi_to_value(ffi: FfiValue) -> Value {
    match ffi.tag {
        FfiType::Null => Value::Null,
        FfiType::Bool => Value::Bool(unsafe { ffi.data.boolean }),
        FfiType::Int => Value::Int(unsafe { ffi.data.integer }),
        FfiType::Float => Value::Float(unsafe { ffi.data.float }),
        FfiType::String | FfiType::Error => {
            let ffi_str = unsafe { ffi.data.string };
            if ffi_str.ptr.is_null() {
                Value::Str("".to_string())
            } else {
                let slice = unsafe { std::slice::from_raw_parts(ffi_str.ptr as *const u8, ffi_str.len) };
                Value::Str(String::from_utf8_lossy(slice).into_owned())
            }
        },
        FfiType::Array => {
            let ffi_arr = unsafe { ffi.data.array };
            if ffi_arr.ptr.is_null() {
                Value::Array(std::rc::Rc::new(std::cell::RefCell::new(Vec::new())))
            } else {
                let mut vec = Vec::with_capacity(ffi_arr.len);
                for i in 0..ffi_arr.len {
                    vec.push(ffi_to_value(unsafe { std::ptr::read(ffi_arr.ptr.add(i)) }));
                }
                Value::Array(std::rc::Rc::new(std::cell::RefCell::new(vec)))
            }
        }
        FfiType::Object => {
            let ffi_obj = unsafe { ffi.data.object };
            if ffi_obj.keys.is_null() || ffi_obj.values.is_null() {
                Value::Map(std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::new())))
            } else {
                let mut map = std::collections::HashMap::new();
                for i in 0..ffi_obj.len {
                    let key_ptr = unsafe { std::ptr::read(ffi_obj.keys.add(i)) };
                    let val = unsafe { std::ptr::read(ffi_obj.values.add(i)) };
                    let key = if key_ptr.is_null() {
                        String::new()
                    } else {
                        unsafe { std::ffi::CStr::from_ptr(key_ptr) }.to_string_lossy().into_owned()
                    };
                    map.insert(key, ffi_to_value(val));
                }
                Value::Map(std::rc::Rc::new(std::cell::RefCell::new(map)))
            }
        }
    }
}

/// A loaded plugin that must stay alive for the FFI functions to remain valid.
pub struct LoadedPlugin {
    _library: Library,
}

/// Global cache of loaded libraries for the FFI gateway capability.
static LOADED_LIBS: std::sync::OnceLock<std::sync::Mutex<HashMap<String, std::sync::Arc<Library>>>> = std::sync::OnceLock::new();

fn get_or_load_library(path: &str) -> Result<std::sync::Arc<Library>, String> {
    let mutex = LOADED_LIBS.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let mut cache = mutex.lock().unwrap();
    if let Some(lib) = cache.get(path) {
        return Ok(lib.clone());
    }
    unsafe {
        let lib = Library::new(path).map_err(|e| format!("Failed to load FFI library '{}': {}", path, e))?;
        let arc_lib = std::sync::Arc::new(lib);
        cache.insert(path.to_string(), arc_lib.clone());
        Ok(arc_lib)
    }
}

/// Dynamic __crush_ffi__ gateway capability.
/// Signature: `__crush_ffi__(lib_path: String, cap_name: String, ...args)`
pub struct FfiGatewayCap;

impl HostCap for FfiGatewayCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "__crush_ffi__".to_string(),
            argc: None, // Variadic
            returns: true,
        }
    }

    fn call(&self, mut args: Vec<Value>) -> Result<Option<Value>, String> {
        if args.len() < 2 {
            return Err("__crush_ffi__ requires at least 2 arguments: lib_path and cap_name".to_string());
        }
        let cap_name = match args.remove(1) {
            Value::Str(s) => s,
            other => return Err(format!("__crush_ffi__ second argument must be cap_name (String), got {:?}", other)),
        };
        let lib_path = match args.remove(0) {
            Value::Str(s) => s,
            other => return Err(format!("__crush_ffi__ first argument must be lib_path (String), got {:?}", other)),
        };

        let lib = get_or_load_library(&lib_path)?;
        unsafe {
            let init_func: Symbol<unsafe extern "C" fn() -> *const CrushPlugin> = 
                lib.get(b"crush_plugin_init\0").map_err(|e| format!("Failed to find crush_plugin_init in '{}': {}", lib_path, e))?;
                
            let plugin_ptr = init_func();
            if plugin_ptr.is_null() {
                return Err("Plugin initialization returned null pointer".to_string());
            }
            
            let plugin = &*plugin_ptr;
            let exports_slice = std::slice::from_raw_parts(plugin.exports, plugin.export_count);
            
            let export = exports_slice.iter().find(|exp| {
                if exp.name.is_null() {
                    false
                } else {
                    let name = CStr::from_ptr(exp.name).to_string_lossy();
                    name == cap_name
                }
            }).ok_or_else(|| format!("Capability '{}' not found in plugin '{}'", cap_name, lib_path))?;

            // Prepare arguments
            let ffi_args: Vec<FfiValue> = args.into_iter().map(value_to_ffi).collect();
            let mut out_result = FfiValue::default();
            
            let success = (export.func)(ffi_args.as_ptr(), ffi_args.len(), &mut out_result);
            
            if success {
                Ok(Some(ffi_to_value(out_result)))
            } else {
                let err_msg = match out_result.tag {
                    FfiType::String | FfiType::Error => ffi_to_value(out_result).to_string(),
                    _ => "FFI call returned false without details".to_string(),
                };
                Err(err_msg)
            }
        }
    }
}

/// Load a Crush plugin from a shared library (.so / .dll / .dylib) and register its caps.
pub fn load_plugin(path: &str, host_caps: &mut HostCaps) -> Result<LoadedPlugin, String> {
    unsafe {
        let library = Library::new(path).map_err(|e| format!("Failed to load plugin: {}", e))?;
        
        let init_func: Symbol<unsafe extern "C" fn() -> *const CrushPlugin> = 
            library.get(b"crush_plugin_init\0").map_err(|e| format!("Failed to find crush_plugin_init: {}", e))?;
            
        let plugin_ptr = init_func();
        if plugin_ptr.is_null() {
            return Err("Plugin returned null".to_string());
        }
        
        let plugin = &*plugin_ptr;
        let exports_slice = std::slice::from_raw_parts(plugin.exports, plugin.export_count);
        
        for export in exports_slice {
            let name = CStr::from_ptr(export.name).to_string_lossy().into_owned();
            host_caps.register(Box::new(FfiHostCap {
                name,
                func: export.func,
            }));
        }
        
        Ok(LoadedPlugin { _library: library })
    }
}
