//! The names, the blank rule and the wire shape of a secret reference.

use super::*;

#[test]
fn a_blank_or_absent_value_holds_nothing_and_any_other_is_held_as_it_is() {
    for blank in [None, Some(""), Some("   "), Some("\t\n")] {
        assert_eq!(held(blank), None, "{blank:?}");
    }
    assert_eq!(held(Some(" key ")), Some(" key "));
}

/// The command is built from the name, so the two cannot disagree, and it
/// names the key `settings show` prints.
#[test]
fn each_reveal_command_ends_with_the_name_settings_show_prints() {
    assert_eq!(SettingSecret::ProxyApiKey.name(), "proxy-api-key");
    assert_eq!(
        SettingSecret::RemotePairingKey.name(),
        "remote-pairing.api-key"
    );
    for secret in SettingSecret::ALL {
        assert_eq!(
            secret.reveal_command(),
            format!("gglib config settings reveal {}", secret.name())
        );
    }
}

/// The body `POST /api/secrets/reveal` takes, both ways.
#[test]
fn a_reference_round_trips_through_its_wire_shape() {
    let cases = [
        (
            SecretRef::Setting {
                secret: SettingSecret::ProxyApiKey,
            },
            serde_json::json!({ "kind": "setting", "secret": "proxy_api_key" }),
        ),
        (
            SecretRef::Setting {
                secret: SettingSecret::RemotePairingKey,
            },
            serde_json::json!({ "kind": "setting", "secret": "remote_pairing_key" }),
        ),
        (
            SecretRef::McpEnv {
                server_id: 7,
                key: "TOKEN".to_owned(),
            },
            serde_json::json!({ "kind": "mcp_env", "server_id": 7, "key": "TOKEN" }),
        ),
    ];
    for (reference, wire) in cases {
        assert_eq!(serde_json::to_value(&reference).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<SecretRef>(wire).unwrap(),
            reference
        );
    }
}
