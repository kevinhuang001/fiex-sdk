//! Typed SDK provider instances use the same registry as native Core providers.
//! The SDK selects one already-registered instance; Core authorizes the call.
use crate::RpcError;
use fiex_provider_contracts::{
    ProviderDescriptor, ProviderInstances, ProviderKind, ProviderMatchRule,
};
use serde_json::Value;
use std::sync::Arc;

pub(super) struct ProviderSet<T: ?Sized + Send + Sync> {
    entries: ProviderInstances<T>,
}

impl<T: ?Sized + Send + Sync> ProviderSet<T> {
    pub fn new(kind: ProviderKind) -> Self {
        Self {
            entries: ProviderInstances::new(kind),
        }
    }

    pub fn register(&mut self, id: String, instance: Box<T>) -> Result<(), String> {
        self.entries.register(
            ProviderDescriptor {
                id,
                kind: self.entries.kind(),
                priority: 0,
                r#match: ProviderMatchRule::default(),
            },
            Arc::from(instance),
        )
    }

    pub fn instances(&self) -> impl Iterator<Item = (&str, &T)> {
        self.entries.instances()
    }

    pub fn select(&self, params: &Value) -> Result<Option<&T>, RpcError> {
        if let Some(value) = params.get("providerId") {
            let id = value
                .as_str()
                .ok_or_else(|| RpcError::invalid_params("providerId must be a string"))?;
            return self
                .entries
                .get(id)
                .map(Some)
                .ok_or_else(|| RpcError::invalid_params("unknown providerId"));
        }
        if self.entries.is_empty() {
            return Ok(None);
        }
        self.entries.only().map(Some).ok_or_else(|| {
            RpcError::invalid_params("providerId required for multiple registered providers")
        })
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
