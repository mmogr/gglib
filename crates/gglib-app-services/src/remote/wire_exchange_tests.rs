//! Tests for the enable and join exchanges: what a body means when it is
//! empty, what an older client's body still means, and what the enable
//! answer carries. `wire_tests.rs` has the status.

use super::*;

#[test]
fn an_empty_body_is_the_safe_default() {
    let req = RemoteEnableBody::default().into_request();
    assert!(req.mcp_servers.is_empty(), "no MCP server is opened");
    assert!(req.relay.is_none());
    assert!(req.discovery, "discovery is on unless switched off");
}

/// Every body and answer of the two exchanges that says nothing still reads,
/// every field at its default.
#[test]
fn an_exchange_that_says_nothing_still_reads() {
    let enable_body: RemoteEnableBody = serde_json::from_str("{}").expect("an empty enable body");
    let join_body: RemoteJoinBody = serde_json::from_str("{}").expect("an empty join body");
    let enabled: RemoteEnableResponse =
        serde_json::from_str("{}").expect("an enable answer that says nothing");
    let joined: RemoteJoinResponse =
        serde_json::from_str("{}").expect("a join answer that says nothing");

    assert_eq!(enable_body, RemoteEnableBody::default());
    assert_eq!(join_body, RemoteJoinBody::default());
    assert_eq!(
        enabled,
        RemoteEnableResponse {
            ticket: String::new(),
            code: None,
            pairing: None,
            expires_in_s: None,
            device: None,
            mcp_servers: Vec::new(),
            already_up: false,
        }
    );
    assert_eq!(
        joined,
        RemoteJoinResponse {
            port: 0,
            base_url: String::new(),
            ticket_fingerprint: String::new(),
            name: None,
            paired: false,
            moved_from: None,
            replaced: None,
        }
    );
}

/// The daemon's join answer carries the name of the machine joined and of
/// the pairing the join replaced, under the names `gglib remote join` reads
/// them by to say so (#1042).
#[test]
fn the_join_answer_names_the_machine_joined_and_the_pairing_it_replaced() {
    let answer = RemoteJoinResponse::from(Joined {
        port: 8180,
        base_url: "http://127.0.0.1:8180/v1".to_owned(),
        ticket_fingerprint: "e5a1b0c2d3f4".to_owned(),
        name: Some("desk".to_owned()),
        paired: true,
        moved_from: None,
        replaced: Some("laptop".to_owned()),
    });

    let sent = serde_json::to_value(answer).expect("the answer serialises");
    assert_eq!(sent["name"], "desk");
    assert_eq!(sent["replaced"], "laptop");
}

/// The enable body does not send `keep_identity`, and a body from an older
/// client that does, `true` or `false`, reads as the same body without it.
#[test]
fn keep_identity_is_off_the_wire_and_an_older_body_that_sends_it_still_reads() {
    let sent = serde_json::to_value(RemoteEnableBody::default()).expect("serialise");
    assert!(
        sent.get("keep_identity").is_none(),
        "the enable body still sends keep_identity: {sent}"
    );

    for flag in ["true", "false"] {
        let body: RemoteEnableBody = serde_json::from_str(&format!(
            r#"{{"mcp_servers":["tavily"],"discovery":false,"keep_identity":{flag}}}"#
        ))
        .expect("a body from an older client still deserialises");
        assert_eq!(
            body,
            RemoteEnableBody {
                mcp_servers: vec!["tavily".to_owned()],
                discovery: Some(false),
                ..RemoteEnableBody::default()
            },
            "keep_identity={flag} changed what the body says"
        );
    }
}

/// A body from a client that still sends the old switch reads, and the
/// switch opens nothing: it was every server, and no list says which.
#[test]
fn an_older_clients_allow_mcp_is_ignored_and_opens_nothing() {
    for flag in ["true", "false"] {
        let body: RemoteEnableBody =
            serde_json::from_str(&format!(r#"{{"allow_mcp":{flag},"discovery":true}}"#))
                .expect("a body from an older client still deserialises");
        let req = body.into_request();
        assert!(req.discovery);
        assert!(
            req.mcp_servers.is_empty(),
            "allow_mcp={flag} opened {:?}",
            req.mcp_servers
        );
    }
}

/// Each name is trimmed and named once, in the order first named; one that
/// trims to nothing is kept, for the daemon to refuse rather than to drop.
#[test]
fn the_names_are_trimmed_and_deduplicated_in_order() {
    let body: RemoteEnableBody =
        serde_json::from_str(r#"{"mcp_servers":[" tavily ","fs","tavily","","  ","fs "]}"#)
            .unwrap();
    assert_eq!(body.into_request().mcp_servers, ["tavily", "fs", ""]);
}

#[test]
fn discovery_off_is_carried_through() {
    let body: RemoteEnableBody =
        serde_json::from_str(r#"{"mcp_servers":["tavily"],"discovery":false}"#).unwrap();
    let req = body.into_request();
    assert_eq!(req.mcp_servers, ["tavily"]);
    assert!(!req.discovery);
}

#[test]
fn an_empty_join_body_reuses_the_last_ticket_on_a_free_port() {
    let req = RemoteJoinBody::default().into_request();
    assert!(req.pairing.is_none());
    assert!(req.port.is_none());
    assert!(req.discovery);
}

/// An enable answer that offered a code, as the daemon's JSON has it, with
/// `device` set or left out.
fn enable_answer(device: Option<&str>) -> serde_json::Value {
    let mut answer = serde_json::json!({
        "ticket": "pipeadlvvgabqkyqvn6vjp7nhslea45a5yls6pnkmizfv4bbu2hxa5iruaaauhlp2na",
        "code": "483920",
        "pairing": "pipeadlvvgabqkyqvn6vjp7nhslea45a5yls6pnkmizfv4bbu2hxa5iruaaauhlp2na-483920",
        "expires_in_s": 120,
        "mcp_servers": [],
        "already_up": false,
    });
    if let Some(device) = device {
        answer["device"] = device.into();
    }
    answer
}

/// The answer that hands out a code names the device the code will issue a
/// key to, and the CLI reads it back: it is how the pairing screen names what
/// paired.
#[test]
fn the_enable_answer_carries_the_device_the_code_is_for() {
    let got: RemoteEnableResponse = serde_json::from_value(enable_answer(Some("dev-a1b2c3d4")))
        .expect("the answer the daemon sends");

    assert_eq!(got.device.as_deref(), Some("dev-a1b2c3d4"));
}

/// A daemon that predates per-device keys names no device, and its answer
/// still reads, code and all.
#[test]
fn an_enable_answer_without_a_device_still_reads() {
    let got: RemoteEnableResponse =
        serde_json::from_value(enable_answer(None)).expect("an answer with no device still reads");

    assert_eq!(got.device, None);
    assert_eq!(got.code.as_deref(), Some("483920"));
}
