//! Binary property lists ("bplist00"): the body format of AirPlay 2 RTSP.

mod read;
mod write;

pub use read::parse;
pub use write::serialize;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Real(f64),
    /// Seconds since 2001-01-01 UTC.
    Date(f64),
    String(String),
    Data(Vec<u8>),
    Array(Vec<Value>),
    /// Keys keep their order: some receivers compare captures byte for byte.
    Dict(Vec<(String, Value)>),
}

impl Value {
    pub fn dict(entries: Vec<(&str, Value)>) -> Value {
        Value::Dict(entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    pub fn string(text: &str) -> Value {
        Value::String(text.to_string())
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Dict(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }
}
