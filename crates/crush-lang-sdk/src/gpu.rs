//! `gpu.launch` — run a crush kernel on an NVIDIA GPU (CRUSH-zorro-3, feature `gpu`).
//!
//! ```crush
//! fn vec_add(a, b, out, n) {
//!     let i = gpu.global_id();
//!     if i < n { gpu.store_f32(out, i, gpu.load_f32(a, i) + gpu.load_f32(b, i)); }
//! }
//! gpu.launch("vec_add", n, a, b, out, n);
//! ```
//!
//! The kernel is a function of the same program that uses the device intrinsics
//! (`gpu.*`, lowered by the frontend to `ptx_*` CASM ops). The host VM never runs it;
//! this capability holds the program's CASM, compiles the named function to PTX with
//! `crush_ptx::compile_kernel` on first use, loads it through the CUDA driver and
//! launches it.
//!
//! `gpu.launch(kernel, threads, args...)`:
//! - `threads` — total threads, launched as `ceil(threads/256)` blocks of 256.
//! - each array argument (an array or vector of numbers) is uploaded as an f32
//!   device buffer and passed as its pointer; after the kernel finishes, every buffer
//!   is copied back into its array **in place** (as floats), so outputs are read from
//!   the arrays the program passed in.
//! - each integer argument is passed as a 64-bit scalar.
//!
//! Float scalars, and buffers of types other than f32, are not supported yet.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use cudarc::driver::{CudaDevice, CudaSlice, DevicePtr, LaunchAsync, LaunchConfig};
use crush_vm::host::{HostCap, HostCapSpec};
use crush_vm::vm::Value;

const BLOCK: u32 = 256;

pub struct GpuLaunchCap {
    program: Option<Arc<casm::Program>>,
    device: Mutex<Option<Arc<CudaDevice>>>,
    /// Kernels already loaded into the device (module name = kernel name).
    loaded: Mutex<HashSet<String>>,
}

impl GpuLaunchCap {
    pub fn new(program: Option<Arc<casm::Program>>) -> Self {
        GpuLaunchCap {
            program,
            device: Mutex::new(None),
            loaded: Mutex::new(HashSet::new()),
        }
    }

    /// The CUDA device, opened on first launch (so a program that never calls
    /// `gpu.launch` works on a box without a GPU).
    fn device(&self) -> Result<Arc<CudaDevice>, String> {
        let mut dev = self.device.lock().unwrap();
        if let Some(d) = dev.as_ref() {
            return Ok(d.clone());
        }
        let d = CudaDevice::new(0).map_err(|e| format!("no CUDA device: {e}"))?;
        *dev = Some(d.clone());
        Ok(d)
    }

    fn ensure_loaded(&self, dev: &Arc<CudaDevice>, kernel: &str) -> Result<(), String> {
        let mut loaded = self.loaded.lock().unwrap();
        if loaded.contains(kernel) {
            return Ok(());
        }
        let program = self
            .program
            .as_ref()
            .ok_or("gpu.launch: this host gave no program to take kernels from")?;
        let ptx = crush_ptx::compile_kernel(program, kernel)
            .map_err(|e| format!("compiling `{kernel}` to PTX: {e}"))?;
        // cudarc wants `&'static str` function names; one leak per distinct kernel.
        let name: &'static str = Box::leak(kernel.to_string().into_boxed_str());
        dev.load_ptx(cudarc::nvrtc::Ptx::from_src(ptx), name, &[name])
            .map_err(|e| format!("loading `{kernel}` PTX: {e}"))?;
        loaded.insert(kernel.to_string());
        Ok(())
    }
}

/// The elements of an array-like value, if it is one.
fn array_items(v: &Value) -> Option<Vec<Value>> {
    match v {
        Value::Array(a) | Value::Vector(a) => Some(a.borrow().clone()),
        _ => None,
    }
}

/// Overwrite an array-like value's elements in place.
fn write_back(v: &Value, data: &[f32]) {
    if let Value::Array(a) | Value::Vector(a) = v {
        let mut items = a.borrow_mut();
        items.clear();
        items.extend(data.iter().map(|&f| Value::Float(f as f64)));
    }
}

impl HostCap for GpuLaunchCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "gpu.launch".to_string(),
            argc: None,
            returns: false,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let [kernel, threads, kargs @ ..] = args.as_slice() else {
            return Err("usage: gpu.launch(kernel_name, threads, args...)".into());
        };
        let Value::Str(kernel) = kernel else {
            return Err("first argument must be the kernel's name (a string)".into());
        };
        let threads = match threads {
            Value::Int(n) if *n > 0 && *n <= u32::MAX as i64 => *n as u32,
            other => return Err(format!("thread count must be a positive int, got {other:?}")),
        };

        let dev = self.device()?;
        self.ensure_loaded(&dev, kernel)?;
        let func = dev
            .get_func(kernel, kernel)
            .ok_or_else(|| format!("kernel `{kernel}` missing after load"))?;

        // Upload buffers; every kernel parameter is one u64 (a device pointer or an
        // integer scalar), the ABI crush-ptx declares.
        let mut buffers: Vec<(usize, CudaSlice<f32>)> = Vec::new();
        let mut params: Vec<u64> = Vec::with_capacity(kargs.len());
        for (i, arg) in kargs.iter().enumerate() {
            if let Some(items) = array_items(arg) {
                let host: Vec<f32> = items
                    .iter()
                    .map(|v| match v {
                        Value::Float(f) => Ok(*f as f32),
                        Value::Int(n) => Ok(*n as f32),
                        other => Err(format!("argument {}: buffer elements must be numbers, got {other:?}", i + 3)),
                    })
                    .collect::<Result<_, _>>()?;
                // A zero-length device allocation is an error in CUDA; give an empty
                // array one (unused) element.
                let upload = if host.is_empty() { vec![0.0] } else { host };
                let buf = dev.htod_sync_copy(&upload).map_err(|e| format!("upload: {e}"))?;
                params.push(*buf.device_ptr());
                buffers.push((i, buf));
            } else {
                match arg {
                    Value::Int(n) => params.push(*n as u64),
                    other => {
                        return Err(format!(
                            "argument {}: expected an array (f32 buffer) or an int, got {other:?}",
                            i + 3
                        ));
                    }
                }
            }
        }

        let cfg = LaunchConfig {
            grid_dim: (threads.div_ceil(BLOCK), 1, 1),
            block_dim: (BLOCK, 1, 1),
            shared_mem_bytes: 0,
        };
        let mut ptrs: Vec<*mut std::ffi::c_void> =
            params.iter_mut().map(|p| p as *mut u64 as *mut std::ffi::c_void).collect();
        // SAFETY: `ptrs` points at one u64 per kernel parameter, matching the
        // `.param .u64` ABI crush-ptx emits; every pointer parameter is a live
        // allocation in `buffers` for the duration of the launch.
        unsafe { func.launch(cfg, &mut ptrs[..]) }.map_err(|e| format!("launch `{kernel}`: {e}"))?;
        dev.synchronize().map_err(|e| format!("`{kernel}` failed on the device: {e}"))?;

        for (i, buf) in &buffers {
            let len = array_items(&kargs[*i]).map_or(0, |v| v.len());
            let data = dev.dtoh_sync_copy(buf).map_err(|e| format!("download: {e}"))?;
            write_back(&kargs[*i], &data[..len]);
        }
        Ok(None)
    }
}

pub fn register(caps: &mut crush_vm::host::HostCaps, program: Option<Arc<casm::Program>>) {
    caps.register(Box::new(GpuLaunchCap::new(program)));
}
