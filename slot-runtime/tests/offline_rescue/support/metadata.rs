use uclone_slot_runtime::domain::{BootId, CommitNonce};
use uclone_slot_runtime::rescue::{RescueError, RescueId, RescueMetadata, RescueMetadataSource};

#[derive(Debug, Default)]
pub(crate) struct FakeMetadata {
    calls: usize,
}

impl FakeMetadata {
    pub(crate) const fn calls(&self) -> usize {
        self.calls
    }
}

impl RescueMetadataSource for FakeMetadata {
    fn next_rescue(&mut self) -> Result<RescueMetadata, RescueError> {
        self.calls += 1;
        Ok(RescueMetadata::new(
            RescueId::parse("rescue-host-0001")?,
            CommitNonce::parse("rescue-nonce-0001")?,
            BootId::parse("boot-host-0001")?,
        ))
    }
}
