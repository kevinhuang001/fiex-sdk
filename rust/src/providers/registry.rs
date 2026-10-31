//! Typed per-feature provider instances, independent of JSON-RPC.
use crate::RpcError;
use serde_json::Value;

pub(super) struct ProviderSet<T: ?Sized> {
    entries: Vec<(String, Box<T>)>,
}
impl<T: ?Sized> Default for ProviderSet<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}
impl<T: ?Sized> ProviderSet<T> {
    pub fn register(&mut self, id: String, instance: Box<T>) -> Result<(), String> {
        if id.is_empty()
            || id.len() > 128
            || !id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | ':'))
        {
            return Err("invalid provider ID".into());
        }
        if self.entries.iter().any(|(known, _)| known == &id) {
            return Err(format!("duplicate provider ID: {id}"));
        }
        self.entries.push((id, instance));
        Ok(())
    }

    pub fn instances(&self) -> impl Iterator<Item = (&str, &T)> {
        self.entries
            .iter()
            .map(|(id, provider)| (id.as_str(), provider.as_ref()))
    }

    pub fn select(&self, params: &Value) -> Result<Option<&T>, RpcError> {
        let selected = params.get("providerId");
        if let Some(value) = selected {
            let id = value
                .as_str()
                .ok_or_else(|| RpcError::invalid_params("providerId must be a string"))?;
            return self
                .entries
                .iter()
                .find(|(name, _)| name == id)
                .map(|(_, instance)| Some(instance.as_ref()))
                .ok_or_else(|| RpcError::invalid_params("unknown providerId"));
        }
        match self.entries.as_slice() {
            [] => Ok(None),
            [(_, only)] => Ok(Some(only.as_ref())),
            _ => Err(RpcError::invalid_params(
                "providerId required for multiple registered providers",
            )),
        }
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
