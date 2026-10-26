//! The three answers a device's server choice gives: none by default, every
//! id for all, and its own ids alone otherwise.

use super::*;

#[test]
fn the_default_admits_no_server() {
    let servers = DeviceServers::default();
    assert_eq!(servers, DeviceServers::none());
    assert!(servers.is_none());
    assert!(!servers.admits(1));
}

#[test]
fn all_admits_every_id() {
    let servers = DeviceServers::All;
    assert!(!servers.is_none());
    assert!(servers.admits(1) && servers.admits(i64::MAX));
}

#[test]
fn only_admits_its_own_ids() {
    let servers = DeviceServers::Only([3, 7].into());
    assert!(!servers.is_none());
    assert!(servers.admits(3) && servers.admits(7));
    assert!(!servers.admits(4));
}
