//! `bytes.*` and `buffer.*` — immutable byte strings and mutable byte buffers.
//!
//! Ported from exosphere `core/base/stdlib/src/{bytes,buffer}.rs`. nanovm
//! had two arena objects, `Bytes` (immutable) and `Buffer` (mutable, shared
//! by reference). CVM1's `Value::Bytes` is a plain `Vec<u8>` (value
//! semantics), so a buffer is a shared `Value::Vector` of byte-valued ints:
//! `buffer.write` mutates it in place and every holder sees the change,
//! exactly as a nanovm `Buffer` reference did. `buffer.freeze` turns one into
//! `Value::Bytes`. Every `bytes.*` reader accepts either form.

use super::{get_int, get_str};
use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};

pub(super) fn register(caps: &mut HostCaps) {
    caps.register(Box::new(BytesLenCap));
    caps.register(Box::new(BytesSliceCap));
    caps.register(Box::new(BytesFromStringCap));
    caps.register(Box::new(BytesToStringCap));
    caps.register(Box::new(BufferAllocCap));
    caps.register(Box::new(BufferWriteCap));
    caps.register(Box::new(BufferReadCap));
    caps.register(Box::new(BufferFreezeCap));
}

/// Copy the bytes out of a `Value::Bytes` or a buffer.
pub(super) fn byte_view(cap: &str, v: &Value) -> Result<Vec<u8>, String> {
    match v {
        Value::Bytes(b) => Ok(b.clone()),
        Value::Vector(cells) => cells
            .borrow()
            .iter()
            .map(|cell| buffer_cell(cap, cell))
            .collect(),
        other => Err(format!(
            "{cap}: expected bytes or buffer, got {}",
            other.type_name()
        )),
    }
}

fn buffer_cell(cap: &str, cell: &Value) -> Result<u8, String> {
    match cell {
        Value::Int(i) => {
            u8::try_from(*i).map_err(|_| format!("{cap}: buffer cell {i} is not a byte (0..=255)"))
        }
        other => Err(format!(
            "{cap}: buffer cell is {}, not a byte",
            other.type_name()
        )),
    }
}

/// Non-negative integer argument (offsets, lengths, sizes).
pub(super) fn get_usize(cap: &str, args: &[Value], idx: usize) -> Result<usize, String> {
    let n = get_int(args, idx)?;
    usize::try_from(n).map_err(|_| format!("{cap}: argument {idx} must be >= 0, got {n}"))
}

/// `offset..offset + len`, or an out-of-bounds error against `size`.
pub(super) fn span(
    cap: &str,
    offset: usize,
    len: usize,
    size: usize,
) -> Result<std::ops::Range<usize>, String> {
    match offset.checked_add(len) {
        Some(end) if end <= size => Ok(offset..end),
        _ => Err(format!(
            "{cap}: range {offset}..{offset}+{len} out of bounds for length {size}"
        )),
    }
}

std_cap!(BytesLenCap, "bytes.len", Some(1), |args: &[Value]| {
    let data = byte_view("bytes.len", &args[0])?;
    Ok(Some(Value::Int(data.len() as i64)))
});

std_cap!(BytesSliceCap, "bytes.slice", Some(3), |args: &[Value]| {
    let data = byte_view("bytes.slice", &args[0])?;
    let offset = get_usize("bytes.slice", args, 1)?;
    let len = get_usize("bytes.slice", args, 2)?;
    let range = span("bytes.slice", offset, len, data.len())?;
    Ok(Some(Value::Bytes(data[range].to_vec())))
});

std_cap!(
    BytesFromStringCap,
    "bytes.from_string",
    Some(1),
    |args: &[Value]| {
        let s = get_str(args, 0)?;
        Ok(Some(Value::Bytes(s.into_bytes())))
    }
);

std_cap!(
    BytesToStringCap,
    "bytes.to_string",
    Some(1),
    |args: &[Value]| {
        let data = byte_view("bytes.to_string", &args[0])?;
        Ok(Some(Value::Str(
            String::from_utf8_lossy(&data).into_owned(),
        )))
    }
);

std_cap!(
    BufferAllocCap,
    "buffer.alloc",
    Some(1),
    |args: &[Value]| {
        let size = get_usize("buffer.alloc", args, 0)?;
        Ok(Some(Value::new_vector(vec![Value::Int(0); size])))
    }
);

/// The shared cells of a buffer argument (a `Value::Vector`).
pub(super) fn buffer_cells(
    cap: &str,
    v: &Value,
) -> Result<std::rc::Rc<std::cell::RefCell<Vec<Value>>>, String> {
    match v {
        Value::Vector(cells) => Ok(cells.clone()),
        Value::Bytes(_) => Err(format!(
            "{cap}: bytes are immutable; write into a buffer from buffer.alloc"
        )),
        other => Err(format!("{cap}: expected buffer, got {}", other.type_name())),
    }
}

std_cap!(
    BufferWriteCap,
    "buffer.write",
    Some(3),
    |args: &[Value]| {
        let cells = buffer_cells("buffer.write", &args[0])?;
        let offset = get_usize("buffer.write", args, 1)?;
        // Copy the source first: writing a buffer into itself must not alias.
        let data = byte_view("buffer.write", &args[2])?;
        let mut cells = cells.borrow_mut();
        let range = span("buffer.write", offset, data.len(), cells.len())?;
        for (cell, byte) in cells[range].iter_mut().zip(&data) {
            *cell = Value::Int(i64::from(*byte));
        }
        Ok(Some(Value::Int(data.len() as i64)))
    }
);

std_cap!(BufferReadCap, "buffer.read", Some(3), |args: &[Value]| {
    let cells = buffer_cells("buffer.read", &args[0])?;
    let data = byte_view("buffer.read", &Value::Vector(cells))?;
    let offset = get_usize("buffer.read", args, 1)?;
    let len = get_usize("buffer.read", args, 2)?;
    let range = span("buffer.read", offset, len, data.len())?;
    Ok(Some(Value::Bytes(data[range].to_vec())))
});

std_cap!(
    BufferFreezeCap,
    "buffer.freeze",
    Some(1),
    |args: &[Value]| {
        let cells = buffer_cells("buffer.freeze", &args[0])?;
        let data = byte_view("buffer.freeze", &Value::Vector(cells.clone()))?;
        // nanovm moved the contents out (`mem::take`): the buffer is left empty.
        cells.borrow_mut().clear();
        Ok(Some(Value::Bytes(data)))
    }
);

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: Vec<Value>) -> Result<Option<Value>, String> {
        let mut caps = HostCaps::new();
        register(&mut caps);
        caps.get(name).expect(name).call(args)
    }

    fn bytes(b: &[u8]) -> Value {
        Value::Bytes(b.to_vec())
    }

    #[test]
    fn bytes_round_trip_through_strings() {
        let b = call("bytes.from_string", vec![Value::Str("héllo".into())])
            .unwrap()
            .unwrap();
        assert_eq!(b, bytes("héllo".as_bytes()));
        assert_eq!(call("bytes.len", vec![b.clone()]), Ok(Some(Value::Int(6))));
        assert_eq!(
            call("bytes.to_string", vec![b]),
            Ok(Some(Value::Str("héllo".into())))
        );
    }

    #[test]
    fn bytes_slice_checks_bounds() {
        let b = bytes(b"abcdef");
        assert_eq!(
            call("bytes.slice", vec![b.clone(), Value::Int(1), Value::Int(3)]),
            Ok(Some(bytes(b"bcd")))
        );
        assert!(
            call("bytes.slice", vec![b.clone(), Value::Int(4), Value::Int(3)])
                .unwrap_err()
                .contains("out of bounds")
        );
        assert!(
            call("bytes.slice", vec![b, Value::Int(-1), Value::Int(1)])
                .unwrap_err()
                .contains(">= 0")
        );
    }

    #[test]
    fn buffer_write_mutates_in_place_and_freeze_empties_it() {
        let buf = call("buffer.alloc", vec![Value::Int(4)]).unwrap().unwrap();
        let alias = buf.clone();
        assert_eq!(
            call(
                "buffer.write",
                vec![buf.clone(), Value::Int(1), bytes(b"hi")]
            ),
            Ok(Some(Value::Int(2)))
        );
        // the write is visible through every reference, like a nanovm Buffer
        assert_eq!(
            call("bytes.len", vec![alias.clone()]),
            Ok(Some(Value::Int(4)))
        );
        assert_eq!(
            call(
                "buffer.read",
                vec![alias.clone(), Value::Int(0), Value::Int(4)]
            ),
            Ok(Some(bytes(b"\0hi\0")))
        );
        assert_eq!(call("buffer.freeze", vec![buf]), Ok(Some(bytes(b"\0hi\0"))));
        assert_eq!(call("bytes.len", vec![alias]), Ok(Some(Value::Int(0))));
    }

    #[test]
    fn buffer_write_rejects_overflow_and_immutable_targets() {
        let buf = call("buffer.alloc", vec![Value::Int(2)]).unwrap().unwrap();
        assert!(
            call("buffer.write", vec![buf, Value::Int(1), bytes(b"xy")])
                .unwrap_err()
                .contains("out of bounds")
        );
        assert!(
            call(
                "buffer.write",
                vec![bytes(b"ab"), Value::Int(0), bytes(b"x")]
            )
            .unwrap_err()
            .contains("immutable")
        );
    }

    #[test]
    fn buffer_can_be_written_into_itself() {
        let buf = call("buffer.alloc", vec![Value::Int(4)]).unwrap().unwrap();
        call(
            "buffer.write",
            vec![buf.clone(), Value::Int(0), bytes(b"ab")],
        )
        .unwrap();
        call(
            "buffer.write",
            vec![buf.clone(), Value::Int(2), buf.clone()],
        )
        .unwrap_err(); // 4 bytes don't fit at offset 2
        call(
            "buffer.write",
            vec![buf.clone(), Value::Int(0), buf.clone()],
        )
        .unwrap();
        assert_eq!(
            call("buffer.read", vec![buf, Value::Int(0), Value::Int(2)]),
            Ok(Some(bytes(b"ab")))
        );
    }
}
