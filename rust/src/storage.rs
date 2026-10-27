//! Plugin-private, typed persistent storage. CAS is serialized by Core, not by
//! a plugin-local lock, so independent tasks/processes cannot lose updates.
use crate::{context::ContextInner, RpcError};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

#[derive(Clone)]
pub struct Storage {
    context: ContextInner,
    scope: String,
}
impl Storage {
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
    fn raw(&self, key: &str) -> Result<Option<Value>, RpcError> {
        let result = self
            .context
            .call("host.storage.get", json!({"scope":self.scope,"key":key}))?;
        match result["found"].as_bool() {
            Some(true) => Ok(Some(result["value"].clone())),
            Some(false) => Ok(None),
            None => Err(RpcError::internal("invalid storage response")),
        }
    }
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, RpcError> {
        self.raw(key)?
            .map(|value| {
                serde_json::from_value(value).map_err(|error| RpcError::internal(error.to_string()))
            })
            .transpose()
    }
    pub fn set<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> Result<(), RpcError> {
        let value = serde_json::to_value(value)
            .map_err(|error| RpcError::invalid_params(error.to_string()))?;
        self.context.call(
            "host.storage.set",
            json!({"scope":self.scope,"key":key,"value":value}),
        )?;
        Ok(())
    }
    pub fn delete(&self, key: &str) -> Result<(), RpcError> {
        self.context
            .call("host.storage.delete", json!({"scope":self.scope,"key":key}))?;
        Ok(())
    }
    pub fn keys(&self) -> Result<Vec<String>, RpcError> {
        serde_json::from_value(
            self.context
                .call("host.storage.keys", json!({"scope":self.scope}))?,
        )
        .map_err(|error| RpcError::internal(error.to_string()))
    }
    pub fn compare_and_set<T: Serialize>(
        &self,
        key: &str,
        expected: Option<&T>,
        next: Option<&T>,
    ) -> Result<bool, RpcError> {
        self.cas(
            key,
            expected
                .map(serde_json::to_value)
                .transpose()
                .map_err(|error| RpcError::invalid_params(error.to_string()))?,
            next.map(serde_json::to_value)
                .transpose()
                .map_err(|error| RpcError::invalid_params(error.to_string()))?,
        )
    }
    fn cas(
        &self,
        key: &str,
        expected: Option<Value>,
        next: Option<Value>,
    ) -> Result<bool, RpcError> {
        self.context.call("host.storage.compareAndSet", json!({"scope":self.scope,"key":key,"expectedExists":expected.is_some(),"expected":expected,"delete":next.is_none(),"value":next}))?
            .as_bool().ok_or_else(|| RpcError::internal("invalid storage CAS response"))
    }
    /// The closure may run again after a conflict; keep it free of side effects.
    /// A bounded retry count prevents livelock under continuous contention.
    pub fn update<T: Serialize + DeserializeOwned>(
        &self,
        key: &str,
        mut update: impl FnMut(Option<T>) -> Option<T>,
    ) -> Result<Option<T>, RpcError> {
        for _ in 0..64 {
            let current = self.raw(key)?;
            let typed = current
                .clone()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|error| RpcError::internal(error.to_string()))?;
            let next = update(typed);
            let value = next
                .as_ref()
                .map(serde_json::to_value)
                .transpose()
                .map_err(|error| RpcError::invalid_params(error.to_string()))?;
            if self.cas(key, current, value)? {
                return Ok(next);
            }
        }
        Err(RpcError::internal(
            "storage update contention limit exceeded",
        ))
    }
}
