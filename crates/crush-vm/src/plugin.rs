use crate::host::{HostCap, HostCapSpec, HostCaps};
use crate::vm::Value;
use crush_ffi::{CrushPlugin, CrushPluginFunc, FfiString, FfiType, FfiValue, FfiValueData};
use libloading::{Library, Symbol};
use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::{CStr, CString};

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
        let mut owned_strings = Vec::new();
        let ffi_args: Vec<FfiValue> = args
            .into_iter()
            .map(|value| value_to_ffi(value, &mut owned_strings))
            .collect::<Result<_, _>>()?;
        let mut out_result = FfiValue::default();
        let success = (self.func)(ffi_args.as_ptr(), ffi_args.len(), &mut out_result);

        let result = ffi_to_value(out_result)?;
        if success {
            Ok(Some(result))
        } else {
            match result {
                Value::Str(message) => Err(message),
                _ => Err("FFI function returned false without a string error".to_string()),
            }
        }
    }
}

fn value_to_ffi(val: Value, owned_strings: &mut Vec<CString>) -> Result<FfiValue, String> {
    Ok(match val {
        Value::Null => FfiValue {
            tag: FfiType::Null,
            data: FfiValueData { integer: 0 },
        },
        Value::Bool(b) => FfiValue {
            tag: FfiType::Bool,
            data: FfiValueData { boolean: b },
        },
        Value::Int(i) => FfiValue {
            tag: FfiType::Int,
            data: FfiValueData { integer: i },
        },
        Value::Float(f) => FfiValue {
            tag: FfiType::Float,
            data: FfiValueData { float: f },
        },
        Value::Str(s) => {
            let cstr =
                CString::new(s).map_err(|_| "FFI string arguments cannot contain NUL bytes")?;
            let string = FfiString {
                ptr: cstr.as_ptr(),
                len: cstr.as_bytes().len(),
            };
            owned_strings.push(cstr);
            FfiValue {
                tag: FfiType::String,
                data: FfiValueData { string },
            }
        }
        other => {
            return Err(format!(
                "FFI MVP accepts null, bool, int, float, and string arguments; got {}",
                other.type_name()
            ))
        }
    })
}

fn ffi_to_value(ffi: FfiValue) -> Result<Value, String> {
    Ok(match ffi.tag {
        FfiType::Null => Value::Null,
        FfiType::Bool => Value::Bool(unsafe { ffi.data.boolean }),
        FfiType::Int => Value::Int(unsafe { ffi.data.integer }),
        FfiType::Float => Value::Float(unsafe { ffi.data.float }),
        FfiType::String | FfiType::Error => {
            let ffi_str = unsafe { ffi.data.string };
            if ffi_str.ptr.is_null() {
                Value::Str("".to_string())
            } else {
                let slice =
                    unsafe { std::slice::from_raw_parts(ffi_str.ptr as *const u8, ffi_str.len) };
                Value::Str(String::from_utf8_lossy(slice).into_owned())
            }
        }
        FfiType::Array => {
            return Err("FFI array results are not supported by the scalar MVP".to_string())
        }
        FfiType::Object => {
            return Err("FFI object results are not supported by the scalar MVP".to_string())
        }
    })
}

/// A loaded plugin that must stay alive for the FFI functions to remain valid.
pub struct LoadedPlugin {
    _library: Library,
}

/// Global cache of loaded libraries for the FFI gateway capability.
static LOADED_LIBS: std::sync::OnceLock<
    std::sync::Mutex<HashMap<String, std::sync::Arc<Library>>>,
> = std::sync::OnceLock::new();

fn get_or_load_library(path: &str) -> Result<std::sync::Arc<Library>, String> {
    let mutex = LOADED_LIBS.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let mut cache = mutex.lock().unwrap();
    if let Some(lib) = cache.get(path) {
        return Ok(lib.clone());
    }
    unsafe {
        let lib = Library::new(path)
            .map_err(|e| format!("Failed to load FFI library '{}': {}", path, e))?;
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
            return Err(
                "__crush_ffi__ requires at least 2 arguments: lib_path and cap_name".to_string(),
            );
        }
        let cap_name = match args.remove(1) {
            Value::Str(s) => s,
            other => {
                return Err(format!(
                    "__crush_ffi__ second argument must be cap_name (String), got {:?}",
                    other
                ))
            }
        };
        let lib_path = match args.remove(0) {
            Value::Str(s) => s,
            other => {
                return Err(format!(
                    "__crush_ffi__ first argument must be lib_path (String), got {:?}",
                    other
                ))
            }
        };

        let lib = get_or_load_library(&lib_path)?;
        unsafe {
            let init_func: Symbol<unsafe extern "C" fn() -> *const CrushPlugin> =
                lib.get(b"crush_plugin_init\0").map_err(|e| {
                    format!("Failed to find crush_plugin_init in '{}': {}", lib_path, e)
                })?;

            let plugin_ptr = init_func();
            if plugin_ptr.is_null() {
                return Err("Plugin initialization returned null pointer".to_string());
            }

            let plugin = &*plugin_ptr;
            if plugin.export_count > 0 && plugin.exports.is_null() {
                return Err("Plugin export table is null".to_string());
            }
            let exports_slice = if plugin.export_count == 0 {
                &[]
            } else {
                std::slice::from_raw_parts(plugin.exports, plugin.export_count)
            };

            let export = exports_slice
                .iter()
                .find(|exp| {
                    !exp.name.is_null()
                        && CStr::from_ptr(exp.name).to_string_lossy() == cap_name
                })
                .ok_or_else(|| {
                    format!(
                        "Capability '{}' not found in plugin '{}'",
                        cap_name, lib_path
                    )
                })?;

            // The frame owns converted strings through the synchronous call.
            let mut owned_strings = Vec::new();
            let ffi_args: Vec<FfiValue> = args
                .into_iter()
                .map(|value| value_to_ffi(value, &mut owned_strings))
                .collect::<Result<_, _>>()?;
            let mut out_result = FfiValue::default();

            let success = (export.func)(ffi_args.as_ptr(), ffi_args.len(), &mut out_result);
            let result = ffi_to_value(out_result)?;
            if success {
                Ok(Some(result))
            } else {
                match result {
                    Value::Str(message) => Err(message),
                    _ => Err("FFI function returned false without a string error".to_string()),
                }
            }
        }
    }
}

/// Load a Crush plugin from a shared library (.so / .dll / .dylib) and register its caps.
pub fn load_plugin(path: &str, host_caps: &mut HostCaps) -> Result<LoadedPlugin, String> {
    load_plugin_filtered(path, None, host_caps)
}

/// Load a host-selected plugin and grant only the named exports in `allowlist`.
pub fn load_plugin_exports(
    path: &str,
    allowlist: &[&str],
    host_caps: &mut HostCaps,
) -> Result<LoadedPlugin, String> {
    if allowlist.is_empty() {
        return Err("plugin export allowlist must not be empty".to_string());
    }
    load_plugin_filtered(path, Some(allowlist), host_caps)
}

fn load_plugin_filtered(
    path: &str,
    allowlist: Option<&[&str]>,
    host_caps: &mut HostCaps,
) -> Result<LoadedPlugin, String> {
    unsafe {
        let library = Library::new(path).map_err(|e| format!("Failed to load plugin: {}", e))?;

        let init_func: Symbol<unsafe extern "C" fn() -> *const CrushPlugin> = library
            .get(b"crush_plugin_init\0")
            .map_err(|e| format!("Failed to find crush_plugin_init: {}", e))?;

        let plugin_ptr = init_func();
        if plugin_ptr.is_null() {
            return Err("Plugin returned null".to_string());
        }

        let plugin = &*plugin_ptr;
        if plugin.export_count > 0 && plugin.exports.is_null() {
            return Err("Plugin export table is null".to_string());
        }
        let exports_slice = if plugin.export_count == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(plugin.exports, plugin.export_count)
        };

        let mut found = HashSet::new();
        let mut selected = Vec::new();
        for export in exports_slice {
            if export.name.is_null() {
                return Err("Plugin export name is null".to_string());
            }
            let name = CStr::from_ptr(export.name).to_string_lossy().into_owned();
            if !found.insert(name.clone()) {
                return Err(format!("Plugin has duplicate export name `{name}`"));
            }
            if allowlist
                .map(|allowed| allowed.contains(&name.as_str()))
                .unwrap_or(true)
            {
                selected.push((name, export.func));
            }
        }
        if let Some(allowed) = allowlist {
            let missing: Vec<_> = allowed
                .iter()
                .filter(|name| !found.contains(**name))
                .copied()
                .collect();
            if !missing.is_empty() {
                return Err(format!(
                    "Plugin is missing allowed export(s): {}",
                    missing.join(", ")
                ));
            }
        }
        for (name, func) in selected {
            host_caps.register(Box::new(FfiHostCap { name, func }));
        }

        Ok(LoadedPlugin { _library: library })
    }
}
