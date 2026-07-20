use crate::journal::JournalStore;
use crate::registry::RegistryStore;

#[doc = "Durable stores jointly owned by the switch coordinator."]
#[derive(Debug, Clone)]
pub struct RuntimeStores {
    journal: JournalStore,
    registry: RegistryStore,
}

impl RuntimeStores {
    #[doc = "Groups the real filesystem Journal and Registry stores."]
    pub const fn new(journal: JournalStore, registry: RegistryStore) -> Self {
        Self { journal, registry }
    }

    #[doc = "Returns the durable transaction Journal."]
    pub const fn journal(&self) -> &JournalStore {
        &self.journal
    }

    #[doc = "Returns the append-only active-slot Registry."]
    pub const fn registry(&self) -> &RegistryStore {
        &self.registry
    }
}
