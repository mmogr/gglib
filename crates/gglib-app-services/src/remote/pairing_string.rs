//! The string a person pastes into `gglib remote connect`.
//!
//! Either a bare ticket — later sessions, with the key already stored — or
//! `<ticket>-<code>`, the one string `enable` shows. The split is on the last
//! `-`: a ticket's alphabet has no hyphen (a `pipe` prefix and base32), so
//! the only one that can appear is the separator `enable` put there.

use std::str::FromStr;

use modelpipe::Ticket;

/// A pairing string, taken apart.
#[derive(Debug)]
pub(super) struct Parsed {
    /// Who to dial.
    pub ticket: Ticket,
    /// The one-time code to redeem for the key, when this is a first pairing.
    pub code: Option<String>,
}

/// How many digits a pairing code has. The other half of
/// `gglib_core::access::generate_pairing_code`.
const CODE_LEN: usize = 6;

/// Parse `<ticket>` or `<ticket>-<code>`, case-insensitively for the ticket.
///
/// # Errors
///
/// A message addressed to the person who typed it: what was expected and,
/// where a ticket was found but the rest was not a code, that too.
pub(super) fn parse(input: &str) -> Result<Parsed, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err(
            "a pairing string is `<ticket>-<code>` as shown by `gglib remote enable`, \
                    or a bare ticket once this machine holds the key"
                .to_owned(),
        );
    }
    let (ticket_part, code) = match input.rsplit_once('-') {
        Some((ticket, code)) if is_code(code) => (ticket, Some(code.to_owned())),
        Some(_) => {
            return Err(format!(
                "the part after the last `-` should be the {CODE_LEN}-digit pairing code"
            ));
        }
        None => (input, None),
    };
    let ticket = Ticket::from_str(ticket_part).map_err(|e| format!("that is not a ticket: {e}"))?;
    Ok(Parsed { ticket, code })
}

fn is_code(s: &str) -> bool {
    s.len() == CODE_LEN && s.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ticket from modelpipe's own format vectors.
    const TICKET: &str = "pipeadlvvgabqkyqvn6vjp7nhslea45a5yls6pnkmizfv4bbu2hxa5iruaaauhlp2na";

    #[test]
    fn a_ticket_and_a_code_come_apart() {
        let parsed = parse(&format!("{TICKET}-483920")).expect("parses");
        assert_eq!(parsed.code.as_deref(), Some("483920"));
        assert_eq!(parsed.ticket.to_string(), TICKET);
    }

    #[test]
    fn a_bare_ticket_has_no_code() {
        let parsed = parse(TICKET).expect("parses");
        assert!(parsed.code.is_none());
    }

    #[test]
    fn the_ticket_half_is_case_insensitive_as_a_qr_makes_it() {
        let upper = format!("{}-483920", TICKET.to_uppercase());
        let parsed = parse(&upper).expect("parses");
        assert_eq!(
            parsed.ticket.to_string(),
            TICKET,
            "canonical form is lowercase"
        );
        assert_eq!(parsed.code.as_deref(), Some("483920"));
    }

    #[test]
    fn a_suffix_that_is_not_six_digits_is_named_as_the_problem() {
        let err = parse(&format!("{TICKET}-48392")).unwrap_err();
        assert!(err.contains("6-digit"), "{err}");
        let err = parse(&format!("{TICKET}-abcdef")).unwrap_err();
        assert!(err.contains("6-digit"), "{err}");
    }

    #[test]
    fn garbage_and_blank_are_refused_with_the_expected_shape() {
        assert!(parse("").unwrap_err().contains("<ticket>-<code>"));
        assert!(
            parse("not-a-ticket-123456")
                .unwrap_err()
                .contains("not a ticket")
        );
    }

    #[test]
    fn surrounding_whitespace_is_forgiven() {
        assert!(parse(&format!("  {TICKET}-483920\n")).is_ok());
    }
}
