//! Typed Rust Debug kernel shared verbatim with Rust AOT. Std only: no VM,
//! registry, serde, or backend types. Descriptors are bounded JSON type trees.
use std::fmt::{self, Write};

pub const MAX_DEPTH: usize = 16;
pub const MAX_ITEMS: usize = 1024;
pub const MAX_OUTPUT: usize = 1024 * 1024;
pub const MAX_DESCRIPTOR: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    Unit,
    Bool,
    Int { bits: u32, signed: bool },
    F64,
    Str,
    Char,
    Seq(Box<Type>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Descriptor,
    Mismatch,
    Depth,
    Items,
    Output,
    Cycle,
    Borrow,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self {
            Self::Descriptor => "invalid or unsupported descriptor",
            Self::Mismatch => "value does not match its Rust type descriptor",
            Self::Depth => "depth quota exceeded",
            Self::Items => "item quota exceeded",
            Self::Output => "output byte quota exceeded",
            Self::Cycle => "cyclic sequence is unsupported",
            Self::Borrow => "sequence is already mutably borrowed",
        };
        write!(f, "rust.debug: {reason} (CRUSH-xps-9)")
    }
}
impl std::error::Error for Error {}

/// Runtime adapters borrow text and sequence contents; they never recursively
/// clone a graph before quotas and cycle checks have run.
pub enum Atom<'a> {
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(&'a str),
    Seq(usize),
    Other,
}
pub trait View: Sized {
    fn atom(&self) -> Atom<'_>;
    fn with_items<R>(&self, visit: impl FnOnce(&[Self]) -> Result<R, Error>) -> Result<R, Error>;
}

/// Only `kind` and `item` are accepted, in either order. No duplicate/unknown
/// keys, escaped names, trailing data or unbounded serde recursion.
pub fn descriptor(input: &str) -> Result<Type, Error> {
    struct Parser<'a> {
        bytes: &'a [u8],
        pos: usize,
    }
    impl Parser<'_> {
        fn ws(&mut self) {
            while self
                .bytes
                .get(self.pos)
                .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
            {
                self.pos += 1;
            }
        }
        fn eat(&mut self, c: u8) -> bool {
            self.ws();
            if self.bytes.get(self.pos) == Some(&c) {
                self.pos += 1;
                true
            } else {
                false
            }
        }
        fn name(&mut self) -> Result<&str, Error> {
            if !self.eat(b'"') {
                return Err(Error::Descriptor);
            }
            let start = self.pos;
            while self
                .bytes
                .get(self.pos)
                .is_some_and(u8::is_ascii_alphanumeric)
            {
                self.pos += 1;
            }
            let end = self.pos;
            if start == end || self.bytes.get(self.pos) != Some(&b'"') {
                return Err(Error::Descriptor);
            }
            self.pos += 1;
            std::str::from_utf8(&self.bytes[start..end]).map_err(|_| Error::Descriptor)
        }
        fn ty(&mut self, depth: usize) -> Result<Type, Error> {
            if depth > MAX_DEPTH {
                return Err(Error::Depth);
            }
            if !self.eat(b'{') {
                return Err(Error::Descriptor);
            }
            let mut kind = None;
            let mut item = None;
            loop {
                let key = self.name()?.to_string();
                if !self.eat(b':') {
                    return Err(Error::Descriptor);
                }
                match key.as_str() {
                    "kind" if kind.is_none() => kind = Some(self.name()?.to_string()),
                    "item" if item.is_none() => item = Some(self.ty(depth + 1)?),
                    _ => return Err(Error::Descriptor),
                }
                if self.eat(b'}') {
                    break;
                }
                if !self.eat(b',') {
                    return Err(Error::Descriptor);
                }
            }
            let kind = kind.ok_or(Error::Descriptor)?;
            if kind == "seq" {
                return Ok(Type::Seq(Box::new(item.ok_or(Error::Descriptor)?)));
            }
            if item.is_some() {
                return Err(Error::Descriptor);
            }
            Ok(match kind.as_str() {
                "unit" => Type::Unit,
                "bool" => Type::Bool,
                "f64" => Type::F64,
                "str" => Type::Str,
                "char" => Type::Char,
                "usize" => Type::Int {
                    bits: 64,
                    signed: false,
                },
                "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" => Type::Int {
                    bits: kind[1..].parse().map_err(|_| Error::Descriptor)?,
                    signed: kind.starts_with('i'),
                },
                _ => return Err(Error::Descriptor),
            })
        }
    }
    if input.len() > MAX_DESCRIPTOR {
        return Err(Error::Descriptor);
    }
    let mut p = Parser {
        bytes: input.as_bytes(),
        pos: 0,
    };
    let ty = p.ty(0)?;
    p.ws();
    if p.pos != p.bytes.len() {
        return Err(Error::Descriptor);
    }
    Ok(ty)
}

struct Output(String);
impl Write for Output {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if self
            .0
            .len()
            .checked_add(text.len())
            .is_none_or(|n| n > MAX_OUTPUT)
        {
            return Err(fmt::Error);
        }
        self.0.push_str(text);
        Ok(())
    }
}

pub fn format<V: View>(value: &V, ty: &Type) -> Result<String, Error> {
    fn render<V: View>(
        value: &V,
        ty: &Type,
        depth: usize,
        nodes: &mut usize,
        path: &mut Vec<usize>,
        out: &mut Output,
    ) -> Result<(), Error> {
        if depth > MAX_DEPTH {
            return Err(Error::Depth);
        }
        *nodes += 1;
        if *nodes > MAX_ITEMS {
            return Err(Error::Items);
        }
        match (ty, value.atom()) {
            (Type::Unit, Atom::Unit) => out.write_str("()"),
            (Type::Bool, Atom::Bool(v)) => write!(out, "{v:?}"),
            (Type::F64, Atom::Float(v)) => write!(out, "{v:?}"),
            (Type::Str, Atom::Text(v)) => write!(out, "{v:?}"),
            (Type::Char, Atom::Text(v)) => {
                let mut chars = v.chars();
                let c = chars.next().ok_or(Error::Mismatch)?;
                if chars.next().is_some() {
                    return Err(Error::Mismatch);
                }
                write!(out, "{c:?}")
            }
            (Type::Int { bits, signed }, Atom::Int(v)) => {
                if !matches!(bits, 8 | 16 | 32 | 64) {
                    return Err(Error::Descriptor);
                }
                let n = v as i128;
                let (lo, hi) = if *signed {
                    (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
                } else {
                    (0, (1i128 << bits) - 1)
                };
                if !(lo..=hi).contains(&n) {
                    return Err(Error::Mismatch);
                }
                write!(out, "{v:?}")
            }
            (Type::Seq(item), Atom::Seq(id)) => {
                if path.contains(&id) {
                    return Err(Error::Cycle);
                }
                path.push(id);
                out.write_char('[').map_err(|_| Error::Output)?;
                value.with_items(|items| {
                    if items.len() > MAX_ITEMS - *nodes {
                        return Err(Error::Items);
                    }
                    for (n, v) in items.iter().enumerate() {
                        if n > 0 {
                            out.write_str(", ").map_err(|_| Error::Output)?;
                        }
                        render(v, item, depth + 1, nodes, path, out)?;
                    }
                    Ok(())
                })?;
                path.pop();
                out.write_char(']')
            }
            _ => return Err(Error::Mismatch),
        }
        .map_err(|_| Error::Output)
    }
    let mut out = Output(String::new());
    render(value, ty, 0, &mut 0, &mut Vec::new(), &mut out)?;
    Ok(out.0)
}
