//! CRUSH-zorro-3: a crush program with a GPU kernel in it.
//!
//! - Always: the host part still compiles to CVM1 (the kernel is left out of it), and
//!   running without the `gpu` grant is refused by the preflight, naming `--gpu`.
//! - With `--features gpu` on a box with an NVIDIA GPU: `examples/gpu/vec_add.crush`
//!   runs its kernel on the device and the host-side check finds no mismatches.
//!   Skipped (not failed) when no CUDA device is available.

use crush_lang_sdk::compile::{compile_crush_source, compile_crush_to_casm};

const VEC_ADD: &str = include_str!("../../../examples/gpu/vec_add.crush");

#[test]
fn kernel_is_left_out_of_the_host_program() {
    let casm = compile_crush_to_casm(VEC_ADD).expect("compile to CASM");
    assert!(casm.functions.contains_key("vec_add"), "the kernel is in the CASM");
    // Before CRUSH-zorro-3 the `ptx_*` ops in the kernel made this bail with
    // "Unsupported CVM1 opcode".
    let program = compile_crush_source(VEC_ADD).expect("host program compiles to CVM1");
    let missing = crush_lang_sdk::effects::missing_grants(&program, None).unwrap();
    let names: Vec<_> = missing.iter().map(|m| m.capability.as_str()).collect();
    assert_eq!(names, ["gpu.launch"]);
}

#[cfg(feature = "gpu")]
#[test]
fn vec_add_runs_on_the_gpu() {
    use crush_lang_sdk::{HostCapsBuilder, Runtime};
    use std::sync::Arc;

    if cudarc::driver::CudaDevice::new(0).is_err() {
        eprintln!("[crush] SKIP vec_add_runs_on_the_gpu: no CUDA device");
        return;
    }
    let program = compile_crush_source(VEC_ADD).unwrap();
    let casm = Arc::new(compile_crush_to_casm(VEC_ADD).unwrap());
    let caps = HostCapsBuilder::new().stdlib(true).gpu(true).gpu_program(casm).build();
    let result = Runtime::new().with_host_caps(caps).run(&program).expect("run");
    // out[1] = 1 + 2, out[999] = 999 + 1998, and 0 mismatches over all 1000.
    let lines: Vec<&str> = result.output.lines().collect();
    assert_eq!(lines, ["3.0", "2997.0", "0"], "output was:\n{}", result.output);
}
