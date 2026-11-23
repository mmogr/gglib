#![doc = include_str!("README.md")]
mod gate;
mod wait;

use std::fmt;
use std::time::Duration;

pub use gate::HubGate;
pub use wait::{LimitNotice, OnLimit, clock, wait_out, waiting_sentence, whole_seconds};

/// The longest reset a limit is taken to name. The Hub's windows are five
/// minutes long: a longer reset is a misread header, and nothing waits on
/// one unattended.
pub const MAX_RESET: Duration = Duration::from_mins(15);

/// The shortest time an allowance with none left is closed, so an answer
/// that says its window resets now is not asked again at once, and again.
pub const MIN_CLOSED: Duration = Duration::from_secs(1);

/// How long a refusal that names no time is waited out: a fifth of the
/// Hub's five-minute window.
pub const HEADERLESS_WAIT: Duration = Duration::from_mins(1);

/// How many refusals in a row a caller takes before it gives up: a reset
/// the Hub keeps moving is not one that is coming.
pub const LIMITED_IN_A_ROW: u32 = 5;

/// One of the Hub's allowances. Each counts its own requests in its own
/// five-minute window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HubBucket {
    /// The API: searches, listings and model info.
    Api,
    /// Reads of a file at its `resolve` address.
    Resolvers,
    /// The website's pages.
    Pages,
}

impl HubBucket {
    /// Every allowance, in the order [`Self::index`] numbers them.
    const ALL: [Self; 3] = [Self::Api, Self::Resolvers, Self::Pages];

    /// The allowance a `RateLimit` header names `name`, or `None` for a
    /// name gglib does not know. A header with such a name is read as no
    /// header, so a refusal under it falls back to the address asked.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|bucket| bucket.name() == name)
    }

    /// The allowance a request for `path` counts against: an address with a
    /// `resolve` segment reads a file, one under `/api/` asks the API, and
    /// any other is a page.
    #[must_use]
    pub fn of_path(path: &str) -> Self {
        let mut segments = path.split('/').filter(|segment| !segment.is_empty());
        if segments.clone().any(|segment| segment == "resolve") {
            Self::Resolvers
        } else if segments.next() == Some("api") {
            Self::Api
        } else {
            Self::Pages
        }
    }

    /// The allowance's name as the Hub writes it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Resolvers => "resolvers",
            Self::Pages => "pages",
        }
    }

    /// The allowance's place in [`Self::ALL`].
    const fn index(self) -> usize {
        match self {
            Self::Api => 0,
            Self::Resolvers => 1,
            Self::Pages => 2,
        }
    }
}

impl fmt::Display for HubBucket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A limit as one answer of the Hub's stated it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HubLimit {
    /// The allowance the request counted against.
    pub bucket: HubBucket,
    /// Requests left in the allowance's window.
    pub remaining: u64,
    /// The time until the window resets, from when the answer was read.
    pub resets_in: Duration,
}

/// The limit a `RateLimit` header's text states, as the Hub writes it:
/// `"api";r=485;t=142`, the allowance's name quoted, then its requests left
/// (`r`) and the seconds until its window resets (`t`).
///
/// Spaces around each part are allowed, the parameters may come in any
/// order, and a parameter other than `r` and `t` is passed over. Anything
/// else, an allowance gglib does not know included, is `None`.
#[must_use]
pub fn parse(header_text: &str) -> Option<HubLimit> {
    let mut items = header_text.split(';').map(str::trim);
    let name = items.next()?.strip_prefix('"')?.strip_suffix('"')?;
    let bucket = HubBucket::named(name)?;
    let (mut remaining, mut resets) = (None, None);
    for item in items {
        let Some((key, value)) = item.split_once('=') else {
            continue;
        };
        let number = || value.trim().parse::<u64>().ok();
        match key.trim() {
            "r" => remaining = Some(number()?),
            "t" => resets = Some(number()?),
            _ => {}
        }
    }
    Some(HubLimit {
        bucket,
        remaining: remaining?,
        resets_in: Duration::from_secs(resets?),
    })
}

/// The limit a refusal (429) states: its `RateLimit` header's when that
/// parses, else the `Retry-After` seconds (or [`HEADERLESS_WAIT`] without
/// them) on the allowance a request for `path` counts against.
///
/// A refusal leaves none, so its `remaining` is 0 whatever the header says.
#[must_use]
pub fn refusal(header: Option<&str>, retry_after: Option<&str>, path: &str) -> HubLimit {
    let stated = header.and_then(parse).unwrap_or_else(|| HubLimit {
        bucket: HubBucket::of_path(path),
        remaining: 0,
        resets_in: retry_after
            .and_then(|seconds| seconds.trim().parse::<u64>().ok())
            .map_or(HEADERLESS_WAIT, Duration::from_secs),
    });
    HubLimit {
        remaining: 0,
        ..stated
    }
}

#[cfg(test)]
#[path = "parse_tests.rs"]
mod parse_tests;

#[cfg(test)]
#[path = "gate_tests.rs"]
mod gate_tests;

#[cfg(test)]
#[path = "wait_tests.rs"]
mod wait_tests;
