//! What `gglib daemon stop --remote` says about the machine it stops: its
//! name, as every other surface shows it, and never its fingerprint.

use gglib_core::domain::UNNAMED_PAIRED;

use super::{question, stopping_line};

#[test]
fn a_remote_stop_names_the_machine_it_stops() {
    assert_eq!(
        question("desk"),
        "  This stops the gglib daemon on desk: its proxy, its models, its downloads."
    );
    assert_eq!(
        stopping_line("desk"),
        "  \u{1f6d1} The gglib daemon on desk is stopping, and this side is disconnected."
    );
    assert!(
        stopping_line(UNNAMED_PAIRED).contains("on the paired machine is stopping"),
        "{}",
        stopping_line(UNNAMED_PAIRED)
    );
}
