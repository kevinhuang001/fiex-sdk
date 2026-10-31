//! One persistent, typed provider-instance registry for Core and the SDK.
//!
//! Descriptor registration is runtime-independent: native handlers and IPC
//! proxies differ only in the concrete instance implementing the feature.
//! Permissions and dynamic operation scope are rechecked by Core at dispatch.
use crate::{ProviderDescriptor, ProviderKind};
use std::{collections::BTreeMap, sync::Arc};

struct Registration<T: ?Sized> {
    descriptor: ProviderDescriptor,
    implementation: Arc<T>,
}

pub struct ProviderInstances<T: ?Sized> {
    kind: ProviderKind,
    entries: BTreeMap<String, Registration<T>>,
}

impl<T: ?Sized + Send + Sync> ProviderInstances<T> {
    pub fn new(kind: ProviderKind) -> Self {
        Self {
            kind,
            entries: BTreeMap::new(),
        }
    }

    pub fn kind(&self) -> ProviderKind {
        self.kind
    }

    pub fn register(
        &mut self,
        descriptor: ProviderDescriptor,
        implementation: Arc<T>,
    ) -> Result<(), String> {
        descriptor.validate()?;
        if descriptor.kind != self.kind {
            return Err("provider registered under the wrong kind".into());
        }
        match self.entries.entry(descriptor.id.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(Registration {
                    descriptor,
                    implementation,
                });
                Ok(())
            }
            std::collections::btree_map::Entry::Occupied(_) => {
                Err(format!("duplicate provider ID: {}", descriptor.id))
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<&T> {
        self.entries
            .get(id)
            .map(|value| value.implementation.as_ref())
    }

    /// Return an owned reference without keeping registry locks alive across
    /// a slow operation or IPC call. The caller rechecks permissions on use.
    pub fn get_arc(&self, id: &str) -> Option<Arc<T>> {
        self.entries
            .get(id)
            .map(|value| Arc::clone(&value.implementation))
    }

    pub fn descriptor(&self, id: &str) -> Option<&ProviderDescriptor> {
        self.entries.get(id).map(|value| &value.descriptor)
    }

    pub fn instances(&self) -> impl Iterator<Item = (&str, &T)> {
        self.entries
            .iter()
            .map(|(id, value)| (id.as_str(), value.implementation.as_ref()))
    }

    pub fn descriptors(&self) -> impl Iterator<Item = &ProviderDescriptor> {
        self.entries.values().map(|value| &value.descriptor)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn only(&self) -> Option<&T> {
        if self.entries.len() == 1 {
            self.entries.values().next().map(|value| value.implementation.as_ref())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProviderMatchRule;

    trait Handler: Send + Sync {
        fn id(&self) -> u32;
    }
    struct Native(u32);
    impl Handler for Native {
        fn id(&self) -> u32 {
            self.0
        }
    }

    fn descriptor(id: &str, kind: ProviderKind) -> ProviderDescriptor {
        ProviderDescriptor {
            id: id.into(),
            kind,
            priority: 0,
            r#match: ProviderMatchRule::default(),
        }
    }

    #[test]
    fn shared_registry_rejects_invalid_kind_and_duplicates_without_replacement() {
        let mut registry: ProviderInstances<dyn Handler> =
            ProviderInstances::new(ProviderKind::Info);
        assert!(registry.register(
            descriptor("wrong", ProviderKind::Preview), Arc::new(Native(1))
        ).is_err());
        assert!(registry.is_empty());
        registry
            .register(descriptor("native", ProviderKind::Info), Arc::new(Native(7)))
            .unwrap();
        assert!(registry.register(
            descriptor("native", ProviderKind::Info), Arc::new(Native(9))
        ).is_err());
        assert!(registry.register(
            descriptor("invalid/id", ProviderKind::Info), Arc::new(Native(9))
        ).is_err());
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get("native").unwrap().id(), 7);
        assert_eq!(registry.only().unwrap().id(), 7);
        assert_eq!(registry.descriptor("native").unwrap().kind, ProviderKind::Info);
        registry
            .register(descriptor("ipc", ProviderKind::Info), Arc::new(Native(8)))
            .unwrap();
        assert!(registry.only().is_none());
        assert_eq!(registry.instances().count(), 2);
    }
}
