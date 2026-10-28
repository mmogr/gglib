//! Which stream each line of `settings reveal` reaches, and the keys clap
//! takes for it.

use clap::Parser;
use gglib_core::secret::SettingSecret;

use super::write_value;
use crate::commands::Commands;
use crate::config_commands::{ConfigCommand, SettingsCommand};

const KEY: &str = "device-key-5f3a9c";

/// What `write_value` wrote to stdout and to stderr, and whether it failed.
fn run(secret: SettingSecret, value: Option<&str>) -> (String, String, anyhow::Result<()>) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let result = write_value(secret, value, &mut out, &mut err);
    (
        String::from_utf8(out).expect("stdout is UTF-8"),
        String::from_utf8(err).expect("stderr is UTF-8"),
        result,
    )
}

#[test]
fn the_value_is_alone_on_stdout_and_stderr_warns_without_it() {
    for secret in SettingSecret::ALL {
        let (out, err, result) = run(secret, Some(KEY));

        result.expect("a stored secret is revealed");
        assert_eq!(out, format!("{KEY}\n"), "{secret:?}");
        assert!(
            err.contains("secret"),
            "{secret:?}: stderr must warn: {err}"
        );
        assert!(
            !err.contains(KEY),
            "{secret:?}: the value must not reach stderr: {err}"
        );
    }
}

#[test]
fn with_nothing_stored_nothing_is_printed_and_the_refusal_names_what_stores_one() {
    let (out, err, result) = run(SettingSecret::RemotePairingKey, None);
    let refusal = result.expect_err("nothing stored is a failure").to_string();
    assert_eq!((out.as_str(), err.as_str()), ("", ""));
    assert!(
        refusal.contains("`gglib remote join <ticket>-<code>`"),
        "{refusal}"
    );
    assert!(
        refusal.contains("`gglib config settings reveal proxy-api-key`"),
        "the refusal names the proxy's own key: {refusal}"
    );

    let (out, err, result) = run(SettingSecret::ProxyApiKey, None);
    let refusal = result.expect_err("nothing stored is a failure").to_string();
    assert_eq!((out.as_str(), err.as_str()), ("", ""));
    assert!(refusal.contains("--proxy-api-key"), "{refusal}");
}

/// The secret `argv` asks `settings reveal` for, or clap's refusal.
fn parsed(argv: &[&str]) -> Result<SettingSecret, clap::Error> {
    let cli = crate::Cli::try_parse_from(argv)?;
    match cli.command {
        Some(Commands::Config {
            command:
                ConfigCommand::Settings {
                    command: SettingsCommand::Reveal { key },
                },
        }) => Ok(key.0),
        _ => panic!("{argv:?} is not `config settings reveal`"),
    }
}

/// Every hint that sends a person to a secret prints `reveal_command`, so
/// each must be a command this parser takes, for the secret it names.
#[test]
fn each_reveal_command_parses_to_its_secret() {
    for secret in SettingSecret::ALL {
        let mut argv = vec!["gglib"];
        argv.extend(secret.reveal_command().split(' ').skip(1));

        assert_eq!(parsed(&argv).expect("parses"), secret);
    }
}

#[test]
fn an_unknown_key_is_refused_by_clap_with_the_keys_it_takes() {
    let refusal = parsed(&["gglib", "config", "settings", "reveal", "hf-token"])
        .expect_err("not a secret settings keep");

    assert_eq!(refusal.kind(), clap::error::ErrorKind::InvalidValue);
    let text = refusal.to_string();
    for secret in SettingSecret::ALL {
        assert!(text.contains(secret.name()), "{text}");
    }
}

/// `argv`'s long help, its whitespace folded so a line clap wrapped reads
/// whole.
fn long_help(argv: &[&str]) -> String {
    let Err(shown) = crate::Cli::try_parse_from(argv) else {
        panic!("{argv:?} should exit with the help");
    };
    assert_eq!(shown.kind(), clap::error::ErrorKind::DisplayHelp);
    shown
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Help texts name the reveal in prose, where `reveal_command` cannot reach,
/// so each is held to its spelling here.
#[test]
fn the_help_that_names_the_reveal_spells_it_as_reveal_command_does() {
    let reveal = SettingSecret::ProxyApiKey.reveal_command();
    for argv in [
        ["gglib", "daemon", "run", "--help"].as_slice(),
        ["gglib", "config", "settings", "reveal", "--help"].as_slice(),
        ["gglib", "proxy", "--help"].as_slice(),
        ["gglib", "serve", "--help"].as_slice(),
    ] {
        let help = long_help(argv);
        assert!(help.contains(reveal), "{argv:?}: {help}");
    }
}
