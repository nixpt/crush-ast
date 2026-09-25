//! `binary.*` — fixed-width integer codecs over bytes / buffers.
//!
//! Ported from exosphere `core/base/stdlib/src/binary.rs` + nanovm's
//! `codec/binary.rs`. Readers accept bytes or a buffer; writers need a buffer
//! (as in nanovm, where `write_*` only accepted `Object::Buffer`).
//!
//! Naming: nanovm registered these as `binary.ReadU16Le` etc. (its macro ran
//! `stringify!` on the struct name — a bug, every other stdlib cap is
//! snake_case). They are `binary.read_u16_le` etc. here.
//!
//! Values: `u64` reads are reinterpreted as `i64` (CVM1 has no unsigned
//! int), and `write_u64_*` accepts any int, bit-for-bit. `write_u16_*` /
//! `write_u32_*` reject values outside the type's range instead of nanovm's
//! silent `as` truncation.

use super::bytes::{buffer_cells, byte_view, get_usize, span};
use super::get_int;
use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};

pub(super) fn register(caps: &mut HostCaps) {
    caps.register(Box::new(ReadU16LeCap));
    caps.register(Box::new(ReadU32LeCap));
    caps.register(Box::new(ReadU64LeCap));
    caps.register(Box::new(ReadU16BeCap));
    caps.register(Box::new(ReadU32BeCap));
    caps.register(Box::new(ReadU64BeCap));
    caps.register(Box::new(WriteU16LeCap));
    caps.register(Box::new(WriteU32LeCap));
    caps.register(Box::new(WriteU64LeCap));
    caps.register(Box::new(WriteU16BeCap));
    caps.register(Box::new(WriteU32BeCap));
    caps.register(Box::new(WriteU64BeCap));
}

macro_rules! binary_read {
    ($cap:ident, $name:expr, $ty:ty, $from:ident) => {
        std_cap!($cap, $name, Some(2), |args: &[Value]| {
            const WIDTH: usize = std::mem::size_of::<$ty>();
            let data = byte_view($name, &args[0])?;
            let offset = get_usize($name, args, 1)?;
            let range = span($name, offset, WIDTH, data.len())?;
            let mut raw = [0u8; WIDTH];
            raw.copy_from_slice(&data[range]);
            Ok(Some(Value::Int(<$ty>::$from(raw) as i64)))
        });
    };
}

macro_rules! binary_write {
    ($cap:ident, $name:expr, $ty:ty, $to:ident, $narrow:expr) => {
        std_cap!($cap, $name, Some(3), |args: &[Value]| {
            const WIDTH: usize = std::mem::size_of::<$ty>();
            let cells = buffer_cells($name, &args[0])?;
            let offset = get_usize($name, args, 1)?;
            let value = get_int(args, 2)?;
            let narrowed: $ty = $narrow($name, value)?;
            let raw = narrowed.$to();
            let mut cells = cells.borrow_mut();
            let range = span($name, offset, WIDTH, cells.len())?;
            for (cell, byte) in cells[range].iter_mut().zip(raw) {
                *cell = Value::Int(i64::from(byte));
            }
            Ok(Some(Value::Null))
        });
    };
}

/// Narrow `value` to `T`'s width, rejecting values that do not fit.
fn narrow<T: TryFrom<i64>>(cap: &str, value: i64) -> Result<T, String> {
    T::try_from(value).map_err(|_| {
        format!(
            "{cap}: value {value} does not fit in {} bits",
            std::mem::size_of::<T>() * 8
        )
    })
}

/// `u64` takes the i64's bit pattern (CVM1 has no unsigned ints).
fn bit_cast_u64(_cap: &str, value: i64) -> Result<u64, String> {
    Ok(value as u64)
}

binary_read!(ReadU16LeCap, "binary.read_u16_le", u16, from_le_bytes);
binary_read!(ReadU32LeCap, "binary.read_u32_le", u32, from_le_bytes);
binary_read!(ReadU64LeCap, "binary.read_u64_le", u64, from_le_bytes);
binary_read!(ReadU16BeCap, "binary.read_u16_be", u16, from_be_bytes);
binary_read!(ReadU32BeCap, "binary.read_u32_be", u32, from_be_bytes);
binary_read!(ReadU64BeCap, "binary.read_u64_be", u64, from_be_bytes);
binary_write!(
    WriteU16LeCap,
    "binary.write_u16_le",
    u16,
    to_le_bytes,
    narrow
);
binary_write!(
    WriteU32LeCap,
    "binary.write_u32_le",
    u32,
    to_le_bytes,
    narrow
);
binary_write!(
    WriteU64LeCap,
    "binary.write_u64_le",
    u64,
    to_le_bytes,
    bit_cast_u64
);
binary_write!(
    WriteU16BeCap,
    "binary.write_u16_be",
    u16,
    to_be_bytes,
    narrow
);
binary_write!(
    WriteU32BeCap,
    "binary.write_u32_be",
    u32,
    to_be_bytes,
    narrow
);
binary_write!(
    WriteU64BeCap,
    "binary.write_u64_be",
    u64,
    to_be_bytes,
    bit_cast_u64
);

#[cfg(test)]
mod tests {
    use super::*;

    fn caps() -> HostCaps {
        let mut caps = HostCaps::new();
        register(&mut caps);
        super::super::bytes::register(&mut caps);
        caps
    }

    fn call(caps: &HostCaps, name: &str, args: Vec<Value>) -> Result<Option<Value>, String> {
        caps.get(name).expect(name).call(args)
    }

    #[test]
    fn reads_both_endiannesses() {
        let caps = caps();
        let b = Value::Bytes(vec![0x01, 0x02, 0x03, 0x04]);
        let read = |name, off| call(&caps, name, vec![b.clone(), Value::Int(off)]);
        assert_eq!(read("binary.read_u16_le", 0), Ok(Some(Value::Int(0x0201))));
        assert_eq!(read("binary.read_u16_be", 0), Ok(Some(Value::Int(0x0102))));
        assert_eq!(
            read("binary.read_u32_le", 0),
            Ok(Some(Value::Int(0x0403_0201)))
        );
        assert_eq!(
            read("binary.read_u32_be", 0),
            Ok(Some(Value::Int(0x0102_0304)))
        );
        assert!(
            read("binary.read_u16_be", 3)
                .unwrap_err()
                .contains("out of bounds")
        );
        assert!(
            read("binary.read_u64_le", 0)
                .unwrap_err()
                .contains("out of bounds")
        );
    }

    #[test]
    fn write_then_read_round_trips_through_a_buffer() {
        let caps = caps();
        let buf = call(&caps, "buffer.alloc", vec![Value::Int(8)])
            .unwrap()
            .unwrap();
        call(
            &caps,
            "binary.write_u32_be",
            vec![buf.clone(), Value::Int(2), Value::Int(0xDEAD_BEEF)],
        )
        .unwrap();
        assert_eq!(
            call(
                &caps,
                "binary.read_u32_be",
                vec![buf.clone(), Value::Int(2)]
            ),
            Ok(Some(Value::Int(0xDEAD_BEEF)))
        );
        call(
            &caps,
            "binary.write_u64_le",
            vec![buf.clone(), Value::Int(0), Value::Int(-1)],
        )
        .unwrap();
        assert_eq!(
            call(&caps, "binary.read_u64_le", vec![buf, Value::Int(0)]),
            Ok(Some(Value::Int(-1)))
        );
    }

    #[test]
    fn narrow_writes_reject_out_of_range_values() {
        let caps = caps();
        let buf = call(&caps, "buffer.alloc", vec![Value::Int(4)])
            .unwrap()
            .unwrap();
        let err = call(
            &caps,
            "binary.write_u16_le",
            vec![buf.clone(), Value::Int(0), Value::Int(70_000)],
        )
        .unwrap_err();
        assert!(err.contains("does not fit in 16 bits"), "{err}");
        assert!(
            call(
                &caps,
                "binary.write_u32_le",
                vec![buf, Value::Int(0), Value::Int(-1)]
            )
            .is_err()
        );
    }

    #[test]
    fn writes_need_a_buffer() {
        let caps = caps();
        let err = call(
            &caps,
            "binary.write_u16_le",
            vec![Value::Bytes(vec![0, 0]), Value::Int(0), Value::Int(1)],
        )
        .unwrap_err();
        assert!(err.contains("immutable"), "{err}");
    }
}
