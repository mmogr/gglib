//! The JSON-RPC shapes, and a request's wait for its own reply: over an
//! in-process pipe standing in for the server, so the server's every line
//! and its silence are the test's to choose. What a server that exits
//! printed is read from a real child process, a shell script.

use std::time::Instant;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, duplex};

use super::*;

/// A client whose server is the far end of the returned pipe, waiting
/// `reply_timeout` for each reply.
fn client_and_server(reply_timeout: Duration) -> (McpClient, BufReader<DuplexStream>) {
    client_and_server_over(reply_timeout, 1 << 16)
}

/// As [`client_and_server`], over a pipe that holds `buffer` bytes each way
/// before a write waits for the other end to read.
fn client_and_server_over(
    reply_timeout: Duration,
    buffer: usize,
) -> (McpClient, BufReader<DuplexStream>) {
    let (near, far) = duplex(buffer);
    let (reader, writer) = tokio::io::split(near);
    let client = McpClient {
        process: None,
        pipes: Some(Mutex::new(Pipes {
            writer: Box::new(writer),
            reader: BufReader::new(Box::new(reader)),
        })),
        stderr: None,
        reply_timeout,
        request_id: AtomicU64::new(1),
        server_info: None,
        capabilities: None,
        protocol_version: None,
    };
    (client, BufReader::new(far))
}

/// The id of the next request the server reads.
async fn read_request(server: &mut BufReader<DuplexStream>) -> u64 {
    let mut line = String::new();
    server.read_line(&mut line).await.unwrap();
    let request: Value = serde_json::from_str(&line).unwrap();
    request["id"].as_u64().unwrap()
}

/// A `tools/call` reply to `id` whose one text item is `text`.
fn reply(id: u64, text: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": { "content": [{ "type": "text", "text": text }] },
    })
    .to_string()
        + "\n"
}

fn call_args() -> HashMap<String, Value> {
    HashMap::new()
}

/// The reply is the one with the request's id: startup noise, a dozen
/// notifications, a request from the server under the same id and a late
/// reply to another id all come first and are passed over.
#[tokio::test]
async fn the_reply_is_the_one_with_the_requests_id() {
    let (client, mut server) = client_and_server(Duration::from_secs(5));
    let serve = tokio::spawn(async move {
        let id = read_request(&mut server).await;
        let mut lines = String::from("npx: installing...\n\n");
        for n in 0..12 {
            lines += &json!({
                "jsonrpc": "2.0",
                "method": "notifications/progress",
                "params": { "progress": n, "total": 12 },
            })
            .to_string();
            lines += "\n";
        }
        // A request of the server's own, whose id can be the same number.
        lines += &json!({ "jsonrpc": "2.0", "id": id, "method": "roots/list" }).to_string();
        lines += "\n";
        lines += &reply(id + 100, "a reply to another request");
        lines += &reply(id, "the reply");
        server.get_mut().write_all(lines.as_bytes()).await.unwrap();
        server
    });

    let got = client.call_tool("draw", call_args()).await.unwrap();

    assert_eq!(
        got.data,
        Some(json!([{ "type": "text", "text": "the reply" }]))
    );
    drop(serve.await.unwrap());
}

/// A server that never answers is given up on at the reply timeout, even
/// while the read is waiting.
#[tokio::test]
async fn a_server_that_never_answers_times_out() {
    let (client, mut server) = client_and_server(Duration::from_millis(200));
    let serve = tokio::spawn(async move {
        read_request(&mut server).await;
        server
    });

    let started = Instant::now();
    let got = client.call_tool("draw", call_args()).await;

    assert!(matches!(got, Err(McpClientError::Timeout)), "{got:?}");
    assert!(started.elapsed() < Duration::from_secs(5));
    // Held open until now, so the silence was the server's, not an EOF.
    drop(serve.await.unwrap());
}

/// A server that closes its stdout before replying is an error that says so.
#[tokio::test]
async fn a_server_that_closes_its_output_is_a_closed_connection() {
    let (client, mut server) = client_and_server(Duration::from_secs(5));
    tokio::spawn(async move {
        read_request(&mut server).await;
        drop(server);
    });

    let got = client.call_tool("draw", call_args()).await.unwrap_err();

    assert!(
        got.to_string().ends_with("Server closed connection"),
        "{got}"
    );
}

/// A client connected to `sh -c script`, waiting `reply_timeout` for each
/// reply.
#[cfg(unix)]
async fn connect_to_script(
    script: &str,
    reply_timeout: Duration,
) -> (McpClient, Result<InitializeResult, McpClientError>) {
    let mut client = McpClient::new();
    client.reply_timeout = reply_timeout;
    let args = ["-c".to_string(), script.to_string()];
    let connected = client
        .connect_stdio("/bin/sh", &args, None, None, &[])
        .await;
    (client, connected)
}

/// A server that exits before it replies is an error that ends with what it
/// printed on stderr: the reason it gives for not starting.
#[cfg(unix)]
#[tokio::test]
async fn a_server_that_exits_says_what_it_printed() {
    let script = "echo 'Error accessing directory /path/to/allowed:' >&2; \
                  echo 'ENOENT: no such file or directory' >&2; exit 1";
    let (_client, connected) = connect_to_script(script, Duration::from_secs(5)).await;

    let got = connected.unwrap_err().to_string();
    assert!(
        got.ends_with(
            "Server closed connection - stderr: Error accessing directory /path/to/allowed: \
             | ENOENT: no such file or directory"
        ),
        "{got}"
    );
}

/// A server that exits having printed nothing is a closed connection and
/// nothing more.
#[cfg(unix)]
#[tokio::test]
async fn a_server_that_exits_silently_is_a_closed_connection() {
    let (_client, connected) = connect_to_script("exit 1", Duration::from_secs(5)).await;

    let got = connected.unwrap_err().to_string();
    assert!(got.ends_with("Server closed connection"), "{got}");
}

/// A server that answers `initialize` and exits before reading the
/// `initialized` notification fails the start the same way: the write finds
/// the pipe broken, and the error ends with what the server printed.
#[cfg(unix)]
#[tokio::test]
async fn a_server_that_exits_after_its_reply_says_what_it_printed() {
    let reply = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "protocolVersion": "2024-11-05",
            "serverInfo": { "name": "leaving" },
            "capabilities": {},
        },
    });
    // stdin is closed before the reply is written, so the notification that
    // follows it has no reader whenever the client writes it.
    let script = format!(
        "read -r request; exec 0<&-; echo 'shutting down: no workspace' >&2; \
         echo '{reply}'; exit 1"
    );
    let (_client, connected) = connect_to_script(&script, Duration::from_secs(5)).await;

    let got = connected.unwrap_err().to_string();
    assert!(
        got.ends_with("Server closed connection - stderr: shutting down: no workspace"),
        "{got}"
    );
}

/// A server that prints far more on stderr than a pipe holds before it
/// replies still has its reply read: the print never blocks it.
#[cfg(unix)]
#[tokio::test]
async fn a_chatty_server_still_starts() {
    let reply = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "protocolVersion": "2024-11-05",
            "serverInfo": { "name": "chatty" },
            "capabilities": {},
        },
    });
    // 256 KiB on stderr, four times what a Linux pipe holds, then the reply
    // to `initialize`, then stdin read to its end.
    let script = format!(
        "i=0; while [ $i -lt 256 ]; do printf '%01024d' 0 >&2; i=$((i+1)); done; \
         read -r request; echo '{reply}'; exec cat >/dev/null"
    );
    let (_client, connected) = connect_to_script(&script, Duration::from_secs(10)).await;

    let initialized = connected.unwrap();
    assert_eq!(initialized.protocol_version, "2024-11-05");
}

/// The next request the server reads, whole.
async fn read_json(server: &mut BufReader<DuplexStream>) -> Value {
    let mut line = String::new();
    server.read_line(&mut line).await.unwrap();
    serde_json::from_str(&line).unwrap()
}

/// One request at a time per server: a second caller's line is not written
/// until the first has its reply, and each caller gets its own. The pipe
/// holds less than a request line, so the first caller's write waits for
/// the server to read and the second caller is already queued for the
/// pipes when that write ends.
#[tokio::test]
async fn one_request_is_in_flight_at_a_time() {
    let (client, mut server) = client_and_server_over(Duration::from_secs(2), 16);
    let serve = tokio::spawn(async move {
        for _ in 0..2 {
            let request = read_json(&mut server).await;
            let mut early = String::new();
            let second =
                tokio::time::timeout(Duration::from_millis(200), server.read_line(&mut early))
                    .await;
            assert!(second.is_err(), "a second request came first: {early}");
            let id = request["id"].as_u64().unwrap();
            let name = request["params"]["name"].as_str().unwrap();
            server
                .get_mut()
                .write_all(reply(id, name).as_bytes())
                .await
                .unwrap();
        }
        server
    });

    let (a, b) = tokio::join!(
        client.call_tool("a", call_args()),
        client.call_tool("b", call_args())
    );

    assert_eq!(
        a.unwrap().data,
        Some(json!([{ "type": "text", "text": "a" }]))
    );
    assert_eq!(
        b.unwrap().data,
        Some(json!([{ "type": "text", "text": "b" }]))
    );
    drop(serve.await.unwrap());
}

/// A notification is one line with its method and no id.
#[tokio::test]
async fn a_notification_is_one_line_with_no_id() {
    let (client, mut server) = client_and_server(Duration::from_secs(2));

    client
        .notify("notifications/initialized", None)
        .await
        .unwrap();
    drop(client);

    let sent = read_json(&mut server).await;
    assert_eq!(sent["method"], "notifications/initialized");
    assert!(sent.get("id").is_none(), "{sent}");
    let mut rest = String::new();
    assert_eq!(server.read_line(&mut rest).await.unwrap(), 0, "{rest}");
}

/// An error reply with a null id, the server saying it could not read a
/// line, is the answer, not something to wait past.
#[tokio::test]
async fn an_error_with_a_null_id_is_the_answer() {
    let (client, mut server) = client_and_server(Duration::from_secs(5));
    let serve = tokio::spawn(async move {
        read_request(&mut server).await;
        let line = json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": { "code": -32700, "message": "Parse error" },
        })
        .to_string()
            + "\n";
        server.get_mut().write_all(line.as_bytes()).await.unwrap();
        server
    });

    let started = Instant::now();
    let got = client.call_tool("draw", call_args()).await;

    assert!(
        matches!(&got, Err(McpClientError::ServerError { code: -32700, .. })),
        "{got:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(4));
    drop(serve.await.unwrap());
}

#[test]
fn test_json_rpc_request_serialization() {
    let request = JsonRpcRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "tools/list".to_string(),
        params: None,
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"jsonrpc\":\"2.0\""));
    assert!(json.contains("\"method\":\"tools/list\""));
    assert!(!json.contains("params")); // Should be omitted when None
}

#[test]
fn test_json_rpc_response_parsing() {
    let json = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
    let response: JsonRpcResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.id, Some(1));
    assert!(response.result.is_some());
    assert!(response.error.is_none());
}

#[test]
fn test_json_rpc_error_parsing() {
    let json = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32600,"message":"Invalid Request"}}"#;
    let response: JsonRpcResponse = serde_json::from_str(json).unwrap();
    assert!(response.error.is_some());
    assert_eq!(response.error.as_ref().unwrap().code, -32600);
}
