#[cfg(feature = "client-lite")]
pub(crate) use crate::json::{parse as read, to_string as write};

#[cfg(not(feature = "client-lite"))]
pub(crate) fn read(input: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(input).map_err(|error| error.to_string())
}

#[cfg(not(feature = "client-lite"))]
pub(crate) fn write(value: &serde_json::Value) -> String {
    value.to_string()
}

#[cfg(feature = "client-lite")]
pub fn json_error(input: &str) -> Option<String> {
    read(input).err()
}
