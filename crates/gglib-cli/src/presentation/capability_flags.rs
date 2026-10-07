//! The capability flags, by the names the CLI takes and prints them under.

use gglib_app_services::types::SetCapabilitiesRequest;
use gglib_core::ModelCapabilities;

/// One capability flag: its name on the command line and in every listing,
/// its bit, and the field of an override request that sets or clears it.
pub(crate) struct CapabilityFlag {
    pub(crate) name: &'static str,
    pub(crate) bit: ModelCapabilities,
    pub(crate) field: fn(&mut SetCapabilitiesRequest) -> &mut Option<bool>,
}

/// Every capability flag, in the order it is listed.
///
/// The one table of their names. `gglib model capabilities` takes `--set` and
/// `--unset` values from it, and that command and `gglib model inspect` print
/// from it, so neither can name a flag the other does not.
pub(crate) const CAPABILITY_FLAGS: [CapabilityFlag; 4] = [
    CapabilityFlag {
        name: "supports-system-role",
        bit: ModelCapabilities::SUPPORTS_SYSTEM_ROLE,
        field: |request| &mut request.supports_system_role,
    },
    CapabilityFlag {
        name: "requires-strict-turns",
        bit: ModelCapabilities::REQUIRES_STRICT_TURNS,
        field: |request| &mut request.requires_strict_turns,
    },
    CapabilityFlag {
        name: "supports-tool-calls",
        bit: ModelCapabilities::SUPPORTS_TOOL_CALLS,
        field: |request| &mut request.supports_tool_calls,
    },
    CapabilityFlag {
        name: "supports-reasoning",
        bit: ModelCapabilities::SUPPORTS_REASONING,
        field: |request| &mut request.supports_reasoning,
    },
];

/// The flag names as the values `--set` and `--unset` take, so `--help` lists
/// them and any other name is refused before the command runs.
pub(crate) fn capability_names() -> clap::builder::PossibleValuesParser {
    CAPABILITY_FLAGS.map(|flag| flag.name).into()
}

/// One line per flag, saying `yes` where `caps` has it and `no` where not.
pub(crate) fn capability_lines(caps: ModelCapabilities) -> Vec<String> {
    let line = |flag: &CapabilityFlag| {
        let set = if caps.contains(flag.bit) { "yes" } else { "no" };
        format!("  {:<21} : {set}", flag.name)
    };
    CAPABILITY_FLAGS.iter().map(line).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The names and the layout both commands print: a flag that is set says
    /// `yes` on its own line and no other, and one that is not says `no`.
    #[test]
    fn each_flag_is_listed_under_its_name_as_yes_or_no() {
        let unset = [
            "  supports-system-role  : no",
            "  requires-strict-turns : no",
            "  supports-tool-calls   : no",
            "  supports-reasoning    : no",
        ];
        assert_eq!(capability_lines(ModelCapabilities::empty()), unset);

        let named = [
            ModelCapabilities::SUPPORTS_SYSTEM_ROLE,
            ModelCapabilities::REQUIRES_STRICT_TURNS,
            ModelCapabilities::SUPPORTS_TOOL_CALLS,
            ModelCapabilities::SUPPORTS_REASONING,
        ];
        for (row, bit) in named.into_iter().enumerate() {
            let mut expected = unset.map(str::to_owned);
            expected[row] = expected[row].replace(": no", ": yes");
            assert_eq!(capability_lines(bit), expected);
        }
    }

    /// The table covers every flag there is, each row's field is the one its
    /// bit is stored under, and no two rows share a name.
    #[test]
    fn the_table_names_every_flag_once_beside_its_own_field() {
        let listed = CAPABILITY_FLAGS
            .iter()
            .fold(ModelCapabilities::empty(), |all, flag| all | flag.bit);
        assert_eq!(listed, ModelCapabilities::all());

        for flag in &CAPABILITY_FLAGS {
            let mut request = SetCapabilitiesRequest::default();
            *(flag.field)(&mut request) = Some(true);
            let json = serde_json::to_value(&request).unwrap();
            let set: Vec<&String> = (json.as_object().unwrap().iter())
                .filter_map(|(field, value)| value.as_bool().map(|_| field))
                .collect();
            // `supports-system-role` is the field `supportsSystemRole`.
            let field = set[0].to_lowercase();
            assert_eq!(set.len(), 1, "{}", flag.name);
            assert_eq!(field, flag.name.replace('-', ""), "{}", flag.name);
            assert_eq!(flag.bit.bits().count_ones(), 1, "{}", flag.name);
        }
    }
}
