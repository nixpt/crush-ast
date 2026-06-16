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
            Type::Map(key, value) => write!(f, "map<{}, {}>", key, value),
            Type::Optional(inner) => write!(f, "optional<{}>", inner),
        }
    }
}

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
