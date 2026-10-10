# runs

<!-- module-docs:start -->

`/v1/runs/*`: a paired device's door to the daemon's runs, replies the
daemon owns until they end. The same five calls, bodies and event framing as
the daemon's `/api/runs/*`, served through the `RunsPort` the proxy was
started with, and answered `503 runs_unavailable` without one.

`PUT /v1/runs/{id}?kind=agent` with `{conversation_id, content, images?,
thinking?, answer_saved?, draw?}` is a
paired device's turn on one of the hub's chats: handed to the daemon's
`AgentRunStarter`, which runs the reply as an agent run in the device's
scope and saves it to the chat. Only a named device may; anything else is
`403 device_not_named`. `thinking` is the chat's Thinking choice, `"off"` or
`"default"`, said on the turn that changes it; any other word, like any other
key, is `400 invalid_request`. `answer_saved: true`, with empty `content` and
no image, answers the question the chat already ends in, as a change at
`/v1/chats/{id}/changes` leaves it (ADR 0017); on a chat that ends in none it
is `409 nothing_to_answer`. `draw` is the device's Draw button, sent only
when pressed.

`PUT /v1/runs/{id}?kind=chat&tools=builtin` is a chat run for a chat the
device keeps itself, run through the agent loop with gglib's builtins: the
body is the device's unchanged `OpenAI` chat request, handed to the same
starter (`start_chat`), and `&draw=true` offers the image tool for that
message. Only a named device may. The run it answers says `frames: "agent"`:
its events are the agent loop's, not `OpenAI` chunks, and a reader picks its
decoder by that key. Without `tools` a chat run is what it always was, its
answer carries no `frames` key, and `draw` alone is `400 invalid_request`.

A request the tunnel edge marked is served in the scope of the device it
named, and sees that device's runs and every run on one of the hub's
chats, which belongs to the chat; any other request is this machine's. The edge reaches the proxy as any client does, so a client
that reaches the proxy directly can forge the markers and be taken for a
device:
this machine not reading a device's reply is a courtesy, not a boundary.

# Module Layout

```text
runs/
  mod.rs       — the module
  handlers.rs  — start, list, read, follow and cancel a run, with errors in
                 the proxy's shape and the registry's codes; nothing a client
                 sent is echoed
  scope.rs     — who is asking, from the `Tunnelled` marker
  turn.rs      — a device's turn on a hub chat, and its chat run with
                 builtins, to the daemon's starter
  sse.rs       — a run's events as server-sent events: `id: <seq>` and
                 `data: <frame>`, then one `event: run` with the run's final
                 state, ending early when the server stops; shared with the
                 daemon's `/api/runs`
```

<!-- module-docs:end -->
