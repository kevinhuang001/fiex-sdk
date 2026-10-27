//! System-keychain service. Missing credentials are `Ok(None)`; denied access
//! and an unavailable keychain are distinct typed errors. No secret is logged.
use crate::{context::ContextInner, RpcError};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
const PREFIX: &str = "fiex-json:v1:";
#[derive(Debug)]
pub enum SecretError {
    Denied(String),
    Unavailable(String),
    InvalidData(String),
    Host(RpcError),
}
impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Denied(message) => write!(f, "keychain access denied: {message}"),
            Self::Unavailable(message) => write!(f, "keychain unavailable: {message}"),
            Self::InvalidData(message) => write!(f, "invalid secret data: {message}"),
            Self::Host(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for SecretError {}
impl From<RpcError> for SecretError {
    fn from(error: RpcError) -> Self {
        Self::Host(error)
    }
}
#[derive(Clone)]
pub struct Secrets {
    context: ContextInner,
    scope: String,
}
impl Secrets {
    pub(crate) fn new(context: ContextInner) -> Self {
        Self {
            context,
            scope: "global".into(),
        }
    }
    pub fn scoped(&self, scope: &str) -> Self {
        Self {
            context: self.context.clone(),
            scope: scope.into(),
        }
    }
    fn response(value: Value) -> Result<Option<String>, SecretError> {
        let message = value["message"]
            .as_str()
            .unwrap_or("credential store rejected the operation")
            .to_string();
        match value["status"].as_str() {
            Some("present") => value["value"]
                .as_str()
                .map(|value| Some(value.to_string()))
                .ok_or_else(|| SecretError::InvalidData("missing secret value".into())),
            Some("missing" | "ok") => Ok(None),
            Some("denied") => Err(SecretError::Denied(message)),
            Some("unavailable") => Err(SecretError::Unavailable(message)),
            _ => Err(SecretError::InvalidData("invalid keychain response".into())),
        }
    }
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, SecretError> {
        let Some(raw) = Self::response(
            self.context
                .call("host.secret.get", json!({"scope":self.scope,"key":key}))?,
        )?
        else {
            return Ok(None);
        };
        let value = if let Some(json) = raw.strip_prefix(PREFIX) {
            serde_json::from_str(json).map_err(|_error| {
                SecretError::InvalidData("secret does not match the requested type".into())
            })?
        } else {
            Value::String(raw)
        };
        serde_json::from_value(value).map(Some).map_err(|_error| {
            SecretError::InvalidData("secret does not match the requested type".into())
        })
    }
    pub fn set<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> Result<(), SecretError> {
        let value = serde_json::to_value(value).map_err(|_error| {
            SecretError::InvalidData("secret does not match the requested type".into())
        })?;
        let raw = match value.as_str() {
            Some(text) if !text.starts_with(PREFIX) => text.to_string(),
            _ => format!("{PREFIX}{value}"),
        };
        Self::response(self.context.call(
            "host.secret.set",
            json!({"scope":self.scope,"key":key,"value":raw}),
        )?)?;
        Ok(())
    }
    pub fn delete(&self, key: &str) -> Result<(), SecretError> {
        Self::response(
            self.context
                .call("host.secret.delete", json!({"scope":self.scope,"key":key}))?,
        )?;
        Ok(())
    }
}
