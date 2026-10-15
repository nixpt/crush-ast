use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A deliberately small proof domain, not Rust's type system.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) enum Kind {
    Copy,
    Str,
    String,
    Vec(Box<Kind>),
    Array(Box<Kind>),
    View(Box<Kind>),
    Iter(Box<Kind>),
    StrView,
    Ref(Box<Kind>),
    Range,
    Callable(Vec<Kind>, Box<Kind>),
    Unknown,
}

impl Kind {
    pub(super) fn flat(&self) -> bool {
        matches!(self, Self::Copy | Self::Str | Self::String)
    }
    pub(super) fn transfer_needs_proof(&self) -> bool {
        !matches!(self, Self::Copy | Self::Str | Self::Callable(..))
    }
    pub(super) fn sequence(&self) -> Option<&Kind> {
        match self {
            Self::Vec(k) | Self::Array(k) | Self::View(k) => Some(k),
            _ => None,
        }
    }
}

pub(super) fn decode(value: Option<&Value>) -> Kind {
    value
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or(Kind::Unknown)
}

pub(crate) fn hint(pat: &syn::Pat) -> Value {
    let kind = match pat {
        syn::Pat::Type(p) => of_type(&p.ty),
        _ => Kind::Unknown,
    };
    serde_json::to_value(kind).unwrap()
}

pub(crate) fn signature(sig: &syn::Signature) -> (Value, Value) {
    let args: Vec<_> = sig
        .inputs
        .iter()
        .map(|p| match p {
            syn::FnArg::Typed(p) => of_type(&p.ty),
            _ => Kind::Unknown,
        })
        .collect();
    (
        serde_json::to_value(args).unwrap(),
        serde_json::to_value(result(&sig.output)).unwrap(),
    )
}

fn result(ret: &syn::ReturnType) -> Kind {
    match ret {
        syn::ReturnType::Default => Kind::Copy,
        syn::ReturnType::Type(_, t) => of_type(t),
    }
}

fn of_type(t: &syn::Type) -> Kind {
    match t {
        syn::Type::Path(p) => {
            let Some(segment) = p.path.segments.last() else {
                return Kind::Unknown;
            };
            match segment.ident.to_string().as_str() {
                "str" => Kind::Str,
                "String" => Kind::String,
                "Vec" => {
                    let item = match &segment.arguments {
                        syn::PathArguments::AngleBracketed(a) => a
                            .args
                            .iter()
                            .find_map(|arg| match arg {
                                syn::GenericArgument::Type(t) => Some(of_type(t)),
                                _ => None,
                            })
                            .unwrap_or(Kind::Unknown),
                        _ => Kind::Unknown,
                    };
                    Kind::Vec(Box::new(item))
                }
                "bool" | "char" | "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8"
                | "u16" | "u32" | "u64" | "u128" | "usize" | "f32" | "f64" => Kind::Copy,
                _ => Kind::Unknown,
            }
        }
        syn::Type::Reference(r) => {
            let k = of_type(&r.elem);
            if k == Kind::Str && r.mutability.is_none() {
                Kind::Str
            } else {
                Kind::Ref(Box::new(k))
            }
        }
        syn::Type::Array(a) => Kind::Array(Box::new(of_type(&a.elem))),
        syn::Type::Tuple(t) if t.elems.is_empty() => Kind::Copy,
        syn::Type::Paren(p) => of_type(&p.elem),
        syn::Type::BareFn(f) => Kind::Callable(
            f.inputs.iter().map(|a| of_type(&a.ty)).collect(),
            Box::new(result(&f.output)),
        ),
        syn::Type::ImplTrait(t) => t
            .bounds
            .iter()
            .find_map(|b| {
                let syn::TypeParamBound::Trait(b) = b else {
                    return None;
                };
                let s = b.path.segments.last()?;
                let syn::PathArguments::Parenthesized(args) = &s.arguments else {
                    return None;
                };
                Some(Kind::Callable(
                    args.inputs.iter().map(of_type).collect(),
                    Box::new(result(&args.output)),
                ))
            })
            .unwrap_or(Kind::Unknown),
        _ => Kind::Unknown,
    }
}
