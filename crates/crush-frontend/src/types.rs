use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Type {
    Int,
    Float,
    Bool,
    String,
    Null,
    Void,                           // For functions with no return
    Any,                            // Dynamic type
    Struct(String),                 // Reference to a named defined struct
    Function(Vec<Type>, Box<Type>), // Args -> Return
    Array(Box<Type>),
    Tuple(Vec<Type>),
    List(Box<Type>),
    Vector(Box<Type>),
    Set(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Optional(Box<Type>),
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::Bool => write!(f, "bool"),
            Type::String => write!(f, "string"),
            Type::Null => write!(f, "null"),
            Type::Void => write!(f, "void"),
            Type::Any => write!(f, "any"),
            Type::Struct(name) => write!(f, "struct {}", name),
            Type::Function(args, ret) => {
                write!(f, "fn(")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ") -> {}", ret)
            }
            Type::Array(inner) => write!(f, "array<{}>", inner),
            Type::Tuple(types) => {
                let types_str: Vec<String> = types.iter().map(|t| t.to_string()).collect();
                write!(f, "tuple<{}>", types_str.join(", "))
            }
            Type::List(inner) => write!(f, "list<{}>", inner),
            Type::Vector(inner) => write!(f, "vector<{}>", inner),
            Type::Set(inner) => write!(f, "set<{}>", inner),
            Type::Map(key, value) => write!(f, "map<{}, {}>", key, value),
            Type::Optional(inner) => write!(f, "optional<{}>", inner),
        }
    }
}

/// The bare container names a type annotation may use (#504): `Array`, `List`,
/// `Set` and `Map`, each with its lowercase twin (as every scalar name has),
/// meaning the container with an `Any` element type. The parser cannot tell a
/// declared struct of the same name apart, so it keeps the name as a
/// `CastType::TypeRef`; the semantic pass and the kind table call this only
/// when no declared kind has that name (a declared kind wins).
pub(crate) fn bare_container(name: &str) -> Option<crush_cast::CastType> {
    use crush_cast::CastType;
    let any = || Box::new(CastType::Any);
    let name = LOWERCASE_TWINS
        .iter()
        .find(|(twin, _)| *twin == name)
        .map_or(name, |(_, canonical)| *canonical);
    match name {
        "Array" => Some(CastType::Array(any())),
        "List" => Some(CastType::List(any())),
        "Set" => Some(CastType::Set(any())),
        "Map" => Some(CastType::Map(any())),
        _ => None,
    }
}

/// The lowercase twins of the container names (as `Int`/`int` for the scalars).
/// Set this to `&[]` to drop them: it is the only place they are spelled.
#[rustfmt::skip]
const LOWERCASE_TWINS: &[(&str, &str)] = &[("array", "Array"), ("list", "List"), ("set", "Set"), ("map", "Map")];

#[cfg(test)]
mod tests {
    use super::Type;

    #[test]
    fn display_array_map_optional_types() {
        assert_eq!(Type::Array(Box::new(Type::Int)).to_string(), "array<int>");
        assert_eq!(
            Type::Map(Box::new(Type::String), Box::new(Type::Float)).to_string(),
            "map<string, float>"
        );
        assert_eq!(
            Type::Optional(Box::new(Type::Bool)).to_string(),
            "optional<bool>"
        );
    }
}
