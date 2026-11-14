//! The Thinking rule over every combination of what a turn says and what
//! its chat remembers.

use super::Thinking::{self, Default as Unset, Off};
use super::{Remember, Settled, settle};

/// Each row is what a turn said, what its chat remembered, whether the run
/// is off (a budget of `0`, else none), and what is written.
#[test]
fn every_combination_of_said_and_remembered_settles_as_the_rule_says() {
    let table: [(Option<Thinking>, Option<Thinking>, bool, Remember); 9] = [
        (None, None, false, None),
        (None, Some(Off), true, None),
        (None, Some(Unset), false, None),
        (Some(Off), None, true, Some(Some(Off))),
        (Some(Off), Some(Off), true, None),
        (Some(Off), Some(Unset), true, Some(Some(Off))),
        (Some(Unset), None, false, None),
        (Some(Unset), Some(Off), false, Some(None)),
        (Some(Unset), Some(Unset), false, Some(None)),
    ];
    for (said, remembered, off, remember) in table {
        let budget = off.then_some(0);
        assert_eq!(
            settle(said, remembered),
            Settled { budget, remember },
            "said {said:?}, remembered {remembered:?}"
        );
    }
}
