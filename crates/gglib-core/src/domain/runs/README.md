# runs

<!-- module-docs:start -->

The wire shapes of a run: a reply the daemon owns from start to end, so it
survives the client that asked for it going away, and what its id may be.
`RunInfo::holds` is the one rule read off them: a run holds the conversation
it writes until it is reported ended. The daemon admits no second run to a
held conversation, and `gglib chat --continue` asks the daemon's listing once,
before it starts.

The TypeScript in `src/types/generated` is generated from these types, and
`contracts/runs/recorded.json` holds sample bodies both clients replay, and
the frames of a reply whose tool made an image (`tool_reply`).

<!-- module-docs:end -->
