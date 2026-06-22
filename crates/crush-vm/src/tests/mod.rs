use crate::assembler::{assemble, disassemble};
use crate::vm::{Quotas, Value, run};

fn run_src(src: &str) -> crate::vm::VmResult {
    let prog = assemble(src, None, None).expect("assembly");
    run(&prog, &Quotas::default()).expect("vm run")
}

fn run_src_with_perms(src: &str, perms: &[&str]) -> crate::vm::VmResult {
    let prog = assemble(src, Some(perms), None).expect("assembly");
    run(&prog, &Quotas::default()).expect("vm run")
}


// ---- Sub-module declarations ----

#[cfg(test)]
mod arith;

#[cfg(test)]
mod control_flow;

#[cfg(test)]
mod data_types;

#[cfg(test)]
mod capabilities;

#[cfg(test)]
mod surfaces;

#[cfg(test)]
mod async_green;

#[cfg(test)]
mod matrix;
