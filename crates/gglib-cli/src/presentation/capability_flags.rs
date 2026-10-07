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
/// from it, so neither can name a flag the other does not. This module's
/// tests write it to `contracts/models/capability_flags.json`, which the
/// page's flags are held to.
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
    use std::path::PathBuf;

    use super::*;

    /// The key an override request carries `flag` under: the one field of the
    /// request that setting it sets, as the request serialises.
    fn request_key(flag: &CapabilityFlag) -> String {
        let mut request = SetCapabilitiesRequest::default();
        *(flag.field)(&mut request) = Some(true);
        let json = serde_json::to_value(&request).unwrap();
        let set: Vec<&String> = (json.as_object().unwrap().iter())
            .filter_map(|(field, value)| value.as_bool().map(|_| field))
            .collect();
        assert_eq!(set.len(), 1, "{}", flag.name);
        set[0].clone()
    }

    /// One row of `contracts/models/capability_flags.json`: a flag's name, its
    /// bit in a model's `capabilities` number, and its key in an override
    /// request.
    #[derive(serde::Serialize)]
    struct Recorded {
        name: &'static str,
        bit: u32,
        field: String,
    }

    /// The table as the file holds it, in the table's order.
    fn recorded() -> Vec<Recorded> {
        let row = |flag: &CapabilityFlag| Recorded {
            name: flag.name,
            bit: flag.bit.bits(),
            field: request_key(flag),
        };
        CAPABILITY_FLAGS.iter().map(row).collect()
    }

    fn recorded_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/models/capability_flags.json")
    }

    /// The checked-in file is exactly what the table gives. Run with
    /// `GGLIB_RECORD_CONTRACTS=1` to rewrite it after a deliberate change.
    #[test]
    fn the_checked_in_flags_are_the_tables() {
        let mut want = serde_json::to_string_pretty(&recorded()).expect("serialise");
        want.push('\n');
        let path = recorded_path();
        if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).expect("make contracts/models");
            std::fs::write(&path, &want).expect("write capability_flags.json");
        }
        let have =
            std::fs::read_to_string(&path).expect("read contracts/models/capability_flags.json");
        assert!(
            have == want,
            "contracts/models/capability_flags.json is stale; rerun with \
             GGLIB_RECORD_CONTRACTS=1\n{want}"
        );
    }

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

    /// The table covers every flag there is, each row's field and bit are
    /// the request's and the core's of the same name, and no two rows share
    /// a name.
    #[test]
    fn the_table_names_every_flag_once_beside_its_own_field() {
        let listed = CAPABILITY_FLAGS
            .iter()
            .fold(ModelCapabilities::empty(), |all, flag| all | flag.bit);
        assert_eq!(listed, ModelCapabilities::all());

        for flag in &CAPABILITY_FLAGS {
            // `supports-system-role` is the field `supportsSystemRole`.
            let field = request_key(flag).to_lowercase();
            assert_eq!(field, flag.name.replace('-', ""), "{}", flag.name);
            // And the one constant `SUPPORTS_SYSTEM_ROLE`.
            let constants: Vec<&str> = flag.bit.iter_names().map(|(name, _)| name).collect();
            let constant = flag.name.to_uppercase().replace('-', "_");
            assert_eq!(constants, [constant.as_str()], "{}", flag.name);
        }
    }
}
