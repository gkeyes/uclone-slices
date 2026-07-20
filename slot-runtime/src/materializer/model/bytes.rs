#[doc = "Observed regular-file bytes in the paired CE and DE base trees."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataBytes {
    ce: u64,
    de: u64,
}

impl DataBytes {
    #[doc = "Constructs exact CE and DE byte counts from bounded tree walks."]
    pub const fn new(ce: u64, de: u64) -> Self {
        Self { ce, de }
    }

    #[doc = "Returns the regular-file bytes in one encryption domain."]
    pub const fn domain(self, domain: super::DataDomain) -> u64 {
        match domain {
            super::DataDomain::Ce => self.ce,
            super::DataDomain::De => self.de,
        }
    }

    #[doc = "Returns the checked aggregate bytes for both domains."]
    pub const fn total(self) -> Option<u64> {
        self.ce.checked_add(self.de)
    }
}
