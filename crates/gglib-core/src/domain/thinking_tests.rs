//! The Thinking rule over every combination of what a turn says and what
//! its chat remembers, with and without a budget of the request's own.

use super::Thinking::{self, Default as Unset, Off};
use super::{Remember, Settled, settle};

/// The budgets a request may carry of its own: none, a ceiling, the launch
/// default, and the one that stops thinking for that request alone.
const OWN_BUDGETS: [Option<i32>; 4] = [None, Some(4096), Some(-1), Some(0)];

/// Each row is what a turn said, what its chat remembered, whether the run
/// is off (a budget of `0`, else the request's own), and what is written.
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
        for own in OWN_BUDGETS {
            let budget = if off { Some(0) } else { own };
            assert_eq!(
                settle(said, remembered, own),
                Settled { budget, remember },
                "said {said:?}, remembered {remembered:?}, own budget {own:?}"
            );
        }
    }
}

/// A chat switched off stays off on a turn whose request asks for a budget:
/// the page's device-wide budget rides on every message it sends.
#[test]
fn a_remembered_off_beats_a_requests_own_budget() {
    assert_eq!(settle(None, Some(Off), Some(4096)).budget, Some(0));
    assert_eq!(settle(None, Some(Off), Some(-1)).budget, Some(0));
}

/// Whatever budget a request carries, what is written is what it would be
/// with none: a request's own `0` stops that request's thinking and leaves
/// the chat remembering nothing.
#[test]
fn a_requests_own_budget_is_never_remembered() {
    for said in [None, Some(Off), Some(Unset)] {
        for remembered in [None, Some(Off), Some(Unset)] {
            let without = settle(said, remembered, None).remember;
            for own in OWN_BUDGETS {
                assert_eq!(settle(said, remembered, own).remember, without);
            }
        }
    }
    assert_eq!(
        settle(None, None, Some(0)),
        Settled {
            budget: Some(0),
            remember: None
        }
    );
}
