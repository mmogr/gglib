//! What a session says about the sampling flags it was started with.

use super::*;

const NOTE: &str = "\"Let clients set sampling\" is on: gglib config set \
                    trust-client-sampling true";

fn trusting(trusted: Option<bool>) -> Settings {
    Settings {
        trust_client_sampling: trusted,
        ..Settings::default()
    }
}

/// While the endpoint does not trust a client's sampling, the session says
/// once that the chat keeps the flags it names and the endpoint uses its own,
/// naming the setting and the command that turns it on.
#[test]
fn an_untrusted_endpoint_is_named_once_with_the_setting_and_its_command() {
    let sampling = ChatSamplingArgs {
        temperature: Some(0.9),
        top_p: Some(0.5),
        top_k: Some(20),
        ..ChatSamplingArgs::default()
    };
    for trusted in [None, Some(false)] {
        let lines = notices(&sampling, Target::Local, &trusting(trusted));

        assert_eq!(
            lines,
            [format!(
                "  Note: this chat keeps --temperature, --top-p and --top-k, but the endpoint \
                 uses its own sampling until {NOTE}"
            )],
            "{trusted:?}"
        );
    }
}

/// A trusted endpoint, a session on the paired machine, and a session that
/// names no flag the endpoint gates, `--max-tokens` alone included, say
/// nothing.
#[test]
fn nothing_is_said_when_the_endpoint_takes_what_the_chat_keeps() {
    let named = ChatSamplingArgs {
        repeat_penalty: Some(1.1),
        ..ChatSamplingArgs::default()
    };
    let max_tokens = ChatSamplingArgs {
        max_tokens: Some(256),
        ..ChatSamplingArgs::default()
    };

    assert!(notices(&named, Target::Local, &trusting(Some(true))).is_empty());
    assert!(notices(&named, Target::Remote, &trusting(None)).is_empty());
    assert!(notices(&max_tokens, Target::Local, &trusting(None)).is_empty());
    assert!(notices(&ChatSamplingArgs::default(), Target::Local, &trusting(None)).is_empty());
    let one = notices(&named, Target::Local, &trusting(None));
    assert!(one[0].contains("keeps --repeat-penalty, but"), "{one:?}");
}

#[test]
fn flags_read_as_a_list() {
    assert_eq!(flags(&[]), "");
    assert_eq!(flags(&["--a"]), "--a");
    assert_eq!(flags(&["--a", "--b"]), "--a and --b");
    assert_eq!(flags(&["--a", "--b", "--c"]), "--a, --b and --c");
}
