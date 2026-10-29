//! What a proxy run is told about the machine it runs on.
//!
//! [`ServeConfig`](crate::ServeConfig) holds one of these. This crate reads no
//! host facts itself: it has no probe, so it builds for a target that offers
//! none, such as iOS. The caller hands in a reader for each fact its platform
//! can answer, and leaves out the rest; [`HostFacts::none`] answers neither.

use std::path::Path;
use std::sync::Arc;

/// Reads this machine's name. `None` when it has none to give.
pub type MachineNameReader = Arc<dyn Fn() -> Option<String> + Send + Sync>;

/// Reads the free bytes on the filesystem holding a path. `None` when it
/// cannot tell.
pub type FreeDiskReader = Arc<dyn Fn(&Path) -> Option<u64> + Send + Sync>;

/// The readers a proxy run asks about its host, each optional.
///
/// Readers rather than values: both facts are read when they are used, so a
/// rename shows on the next `/v1/models` and each eviction sweep sees the disk
/// as it is then.
#[derive(Clone)]
pub struct HostFacts {
    /// This machine's name, for `machine_name` on `/v1/models`, read on every
    /// listing and kept only as [`gglib_core::domain::machine_name`] keeps it.
    /// `None` leaves the field out of the response.
    pub machine_name: Option<MachineNameReader>,
    /// The free space under the slot directory, for the
    /// [`DiskBudget::Auto`](crate::slot_eviction::DiskBudget::Auto) budget.
    /// `None` leaves that budget unset, so a sweep evicts no slot file; a
    /// reader that cannot tell counts as no free space (see
    /// [`evict_over_budget`](crate::slot_eviction::evict_over_budget)).
    pub free_disk: Option<FreeDiskReader>,
}

impl HostFacts {
    /// A host that answers nothing: no machine name, and no free-space
    /// reading.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            machine_name: None,
            free_disk: None,
        }
    }

    /// This machine's name, as `/v1/models` publishes it: the reader's
    /// answer, kept only when [`gglib_core::domain::machine_name`] keeps it.
    pub(crate) fn read_machine_name(&self) -> Option<String> {
        let raw = self.machine_name.as_ref().and_then(|read| read())?;
        gglib_core::domain::machine_name(&raw)
    }
}
