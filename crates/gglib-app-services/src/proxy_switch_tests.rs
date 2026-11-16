//! Tests for how a start that finds the endpoint on is compared with it, and
//! the words each difference is said in.

use super::*;

fn config(port: u16) -> ProxyConfig {
    ProxyConfig {
        host: "127.0.0.1".into(),
        port,
        ..ProxyConfig::default()
    }
}

fn pin(id: i64, name: &str) -> PinnedSpec {
    PinnedSpec {
        id,
        name: name.to_owned(),
        ..PinnedSpec::default()
    }
}

fn shape(config: &ProxyConfig, pinned: Option<&PinnedSpec>) -> ProxyShape {
    ProxyShape::of(config, pinned, config.port, config.api_key.as_deref())
}

/// The same start again changes nothing, and asking for any port is the
/// port that runs.
#[test]
fn the_same_endpoint_asked_again_is_no_change() {
    let running = shape(&config(8080), None);
    assert_eq!(change(&running, &shape(&config(8080), None)), None);
    assert_eq!(
        change(&running, &ProxyShape::of(&config(0), None, 0, None)),
        None
    );

    let qwen = pin(3, "Qwen3-4B");
    let pinned = shape(&config(8080), Some(&qwen));
    assert_eq!(change(&pinned, &shape(&config(8080), Some(&qwen))), None);
}

/// Each way the pin moves is said with the models it names.
#[test]
fn a_pin_that_moves_is_said_with_its_models() {
    let qwen = pin(3, "Qwen3-4B");
    let gemma = pin(5, "gemma-3-4b");
    let every = shape(&config(8080), None);
    let only_qwen = shape(&config(8080), Some(&qwen));

    assert_eq!(
        change(&every, &only_qwen),
        Some(EndpointChange {
            now: "on at port 8080, serving every model".into(),
            action: "serve only Qwen3-4B".into(),
            moves: false,
        })
    );
    assert_eq!(
        change(&only_qwen, &every),
        Some(EndpointChange {
            now: "on at port 8080, serving only Qwen3-4B".into(),
            action: "serve every model again".into(),
            moves: false,
        })
    );
    assert_eq!(
        change(&only_qwen, &shape(&config(8080), Some(&gemma)))
            .unwrap()
            .action,
        "serve only gemma-3-4b instead of Qwen3-4B"
    );
}

/// A different port and different sampling are both named; the key is
/// named only as different.
#[test]
fn flags_that_differ_are_each_named() {
    let running = shape(&config(8080), None);
    let asked = ProxyConfig {
        inference_override: Some(InferenceConfig {
            temperature: Some(0.5),
            ..InferenceConfig::default()
        }),
        api_key: Some("s3cret-key".into()),
        ..config(9929)
    };
    let said = change(&running, &shape(&asked, None)).expect("a change");
    assert_eq!(
        said.action,
        "restart it with port 9929 (now 8080), different sampling and a different API key"
    );
    assert!(!said.action.contains("s3cret"), "{said:?}");
}

/// The same model asked for with other launch options, a `--jinja` or a
/// context of its own, is not the endpoint as it runs: its options are
/// fixed for the run.
#[test]
fn the_same_model_with_other_launch_options_is_a_change() {
    let qwen = pin(3, "Qwen3-4B");
    let mut jinja = pin(3, "Qwen3-4B");
    jinja.launch_overrides.jinja = Some(true);
    let running = shape(&config(8080), Some(&qwen));

    assert_eq!(
        change(&running, &shape(&config(8080), Some(&jinja)))
            .unwrap()
            .action,
        "serve only Qwen3-4B with other launch options"
    );
}

/// A pin and a flag together are one sentence.
#[test]
fn a_pin_with_a_flag_is_one_sentence() {
    let qwen = pin(3, "Qwen3-4B");
    let asked = ProxyConfig {
        host: "0.0.0.0".into(),
        ..config(8080)
    };
    assert_eq!(
        change(&shape(&config(8080), None), &shape(&asked, Some(&qwen)))
            .unwrap()
            .action,
        "serve only Qwen3-4B, with address 0.0.0.0 (now 127.0.0.1)"
    );
}

/// Every part of a start the endpoint keeps for its run is compared, each way
/// round: a start that differs in one of them alone is a change.
#[test]
fn each_part_of_the_endpoint_is_compared_both_ways() {
    let base = config(8080);
    let cached = ProxyConfig {
        cache_enabled: true,
        slot_dir: Some("/slots/a".into()),
        disk_budget: DiskBudget::Auto,
        ..config(8080)
    };
    let pairs: Vec<(&str, ProxyConfig, ProxyConfig)> = vec![
        (
            "default context",
            base.clone(),
            ProxyConfig {
                default_context: Some(16_384),
                ..config(8080)
            },
        ),
        (
            "prompt cache on disk",
            base.clone(),
            ProxyConfig {
                cache_enabled: true,
                ..config(8080)
            },
        ),
        (
            "a different cache folder",
            cached.clone(),
            ProxyConfig {
                slot_dir: Some("/slots/b".into()),
                ..cached.clone()
            },
        ),
        (
            "a different cache budget",
            cached.clone(),
            ProxyConfig {
                disk_budget: DiskBudget::ExplicitBytes(4 << 30),
                ..cached
            },
        ),
        (
            "default profile",
            base.clone(),
            ProxyConfig {
                default_profile: Some("coding".into()),
                ..config(8080)
            },
        ),
        (
            "allowed hosts",
            base,
            ProxyConfig {
                allowed_hosts: vec!["desk.local".into()],
                ..config(8080)
            },
        ),
    ];
    for (named, one, other) in pairs {
        for (running, asked) in [(&one, &other), (&other, &one)] {
            let said = change(&shape(running, None), &shape(asked, None));
            assert!(
                said.as_ref()
                    .is_some_and(|c| c.action.contains(named.split(' ').next_back().unwrap())),
                "{named}: {said:?}"
            );
        }
    }
}

/// Allowed hosts are a set: the same hosts in another order, or one named
/// twice, are the endpoint as it is. A key asked for that is the one the
/// endpoint demands is the endpoint as it is too.
#[test]
fn hosts_compare_as_a_set_and_keys_as_demanded() {
    let one = ProxyConfig {
        allowed_hosts: vec!["a.local".into(), "b.local".into()],
        ..config(8080)
    };
    let other = ProxyConfig {
        allowed_hosts: vec!["b.local".into(), "a.local".into(), "b.local".into()],
        ..config(8080)
    };
    assert_eq!(change(&shape(&one, None), &shape(&other, None)), None);

    let running = ProxyShape::of(&config(8080), None, 8080, Some("k3y"));
    let asked = ProxyShape::of(&config(8080), None, 8080, Some("k3y"));
    assert_eq!(change(&running, &asked), None);
}

/// A switch to another port or address moves the endpoint: clients
/// connected now have to be pointed at it. One that pins a model does not.
#[test]
fn a_new_port_or_address_moves_the_endpoint() {
    let running = shape(&config(8080), None);
    assert!(change(&running, &shape(&config(9000), None)).unwrap().moves);
    let lan = ProxyConfig {
        host: "0.0.0.0".into(),
        ..config(8080)
    };
    assert!(change(&running, &shape(&lan, None)).unwrap().moves);
    assert!(
        !change(&running, &shape(&config(8080), Some(&pin(3, "Qwen3-4B"))))
            .unwrap()
            .moves
    );
}
