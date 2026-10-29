//! The budget at a few readings, and no budget from a failed one.

use super::*;

const GB: u64 = 1_000_000_000;

fn reading(physical_bytes: u64, available_bytes: u64) -> DeviceReading {
    DeviceReading {
        physical_bytes,
        available_bytes,
        is_simulator: false,
    }
}

/// With plenty available, the share of physical memory is the budget.
#[test]
fn the_share_of_physical_memory_caps_the_budget() {
    assert_eq!(budget_from(&reading(8 * GB, 6 * GB)), Some(4_800_000_000));
    assert_eq!(budget_from(&reading(12 * GB, 9 * GB)), Some(7_200_000_000));
}

/// With less available than the share, what is available is the budget.
#[test]
fn what_is_available_caps_the_budget() {
    assert_eq!(budget_from(&reading(8 * GB, 3 * GB)), Some(3 * GB));
    assert_eq!(budget_from(&reading(6 * GB, 2 * GB)), Some(2 * GB));
}

/// Where the two meet, either is the budget.
#[test]
fn a_share_equal_to_what_is_available_is_the_budget() {
    assert_eq!(budget_from(&reading(10 * GB, 6 * GB)), Some(6 * GB));
}

/// A zero is a reading that failed, and gives no budget.
#[test]
fn a_zero_reading_gives_no_budget() {
    assert_eq!(budget_from(&reading(0, 3 * GB)), None);
    assert_eq!(budget_from(&reading(8 * GB, 0)), None);
    assert_eq!(budget_from(&reading(0, 0)), None);
}

/// A simulator's reading is made into a budget by the same rule.
#[test]
fn a_simulator_reading_follows_the_same_rule() {
    let simulated = DeviceReading {
        is_simulator: true,
        ..reading(8 * GB, 3 * GB)
    };
    assert_eq!(
        budget_from(&simulated),
        budget_from(&reading(8 * GB, 3 * GB))
    );
}
