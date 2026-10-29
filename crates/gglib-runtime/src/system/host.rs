//! This machine's name and free disk, read with sysinfo.
//!
//! The proxy has no host probe, so it builds for targets sysinfo does not;
//! [`host_facts`] hands it these two readers. The machine's name is also what
//! a pairing tells the other machine this one is called.

use std::path::Path;
use std::sync::Arc;

use gglib_proxy::HostFacts;
use sysinfo::Disks;

/// This machine's name, the first label of its host name.
///
/// `None` when [`gglib_core::domain::machine_name`] does not keep it. What
/// `/v1/models` publishes as `machine_name`, and what a pairing tells the
/// machine it pairs with this one is called.
#[must_use]
pub fn this_machine_name() -> Option<String> {
    sysinfo::System::host_name()
        .as_deref()
        .and_then(gglib_core::domain::machine_name)
}

/// Free bytes on the filesystem holding `path`: the mounted disk whose mount
/// point is the longest prefix of `path`, the most specific mount covering it.
///
/// `None` when no mounted disk covers `path`, which is always the case for a
/// relative one.
#[must_use]
pub fn free_disk_bytes(path: &Path) -> Option<u64> {
    Disks::new_with_refreshed_list()
        .list()
        .iter()
        .filter(|disk| path.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
        .map(sysinfo::Disk::available_space)
}

/// What a proxy on this machine is told about it: both readers above.
#[must_use]
pub fn host_facts() -> HostFacts {
    HostFacts {
        machine_name: Some(Arc::new(this_machine_name)),
        free_disk: Some(Arc::new(free_disk_bytes)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_path_is_on_no_mounted_disk() {
        assert_eq!(free_disk_bytes(Path::new("slots")), None);
    }
}
