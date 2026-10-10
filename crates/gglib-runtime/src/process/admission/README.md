# Admission

<!-- module-docs:start -->

Who gets the GPU next, and for how long.

Every request for a model passes through [`AdmissionQueue`] before it is
routed. The queue decides one of four things: serve it now from a resident
model, launch that model into a free or evictable slot, wait, or give up.

# Why a queue rather than a wait

The proxy used to absorb model-swap collisions by *waiting them out* — poll,
back off, poll again, surface a 503 after thirty seconds. That treats each
request as if it were alone. Under alternating traffic (a chat client and an
embeddings client sharing one endpoint) it is the worst possible strategy: N
requests produce N swaps, each one tearing down a llama-server, reloading
weights, and re-prefilling a prompt, while every request pays the full latency
of the swap it triggered.

A queue can see what the waiter could not: that five more requests for the same
model are already behind this one. Granting a model the GPU therefore grants it
a *turn*, and the turn drains every request queued for it before the next model
gets a look in. N alternating requests cost two swaps instead of N.

# Fairness

Three rules, and they compose to bound the wait:

| Rule | Effect |
|---|---|
| Global FIFO | The oldest waiting request decides which model is up next, so a model with a hundred requests behind it cannot bury one with a single older request |
| [`DRAIN_QUANTUM`] | A turn ends once it has run this long with a rival waiting — or immediately, if nothing is left queued for the turn holder |
| `SERVER_PARALLEL` | A resident model is admitted at most this many requests at once, matching what its llama-server can actually start |

The first two decide *whose* turn it is. The third is what makes a turn able to
end at all, and it is worth being exact about why.

**A swap never preempts a request in flight.** A slot with outstanding leases is
not evictable, full stop — no bound in this module can override that, because
the alternative is killing a live generation mid-stream. So a turn can only
change hands in a moment when the outgoing model has nothing in flight.

llama-server is launched with `--parallel 1`. If the queue admits four
concurrent requests for a resident model anyway, all four are forwarded, three
of them queue up *inside* llama-server, and the slot stays pinned for the whole
serialised run — invisibly, since those three are not in this queue and do not
appear in its depth. A client that keeps one request outstanding at all times
never lets the count reach zero, and the quantum expires against a slot that can
never actually be handed over. Capping admission at what the instance can start
is what keeps the backlog here, where it is visible and ordered, and where the
count between requests genuinely returns to zero.

The cap has to be exactly `SERVER_PARALLEL` rather than a little above it to
keep the pipeline fed: a slot becomes evictable only at *zero* in flight, so any
standing surplus would reintroduce the stall.

One more rule follows from the same reasoning: a resident model **stands aside**
once a rival is entitled to its slot under the two rules above, rather than
renewing its lease. Without that the cap would only create an instant at zero
in flight, and which of the woken requesters claims that instant is a race.

What none of this covers is a single generation that simply runs for a very long
time — it is indivisible, so the rival behind it waits. [`ADMISSION_DEADLINE`]
is the backstop there: the request gives up and gets a 503 with `Retry-After`,
and the caller controls its own backoff from there.

The deadline measures **stall, not age**. A waiter's clock is paused while any
launch is in flight (a load is bounded work, not a wedge — the first request on
a cold daemon used to 503 against its own model load here) and resets whenever
the queue provably moves: a lease released, a launch landing or failing, a slot
evicted. Only a queue that does *nothing* for the whole deadline expires a
waiter, which is exactly the hog-or-wedge case the backstop exists for.

This is why admission returns a lease rather than just a target — see
[`AdmissionLease`](gglib_core::ports::AdmissionLease).

A run that talks to llama-server's port directly (an agent run) takes a
*hold* instead (`hold.rs`): while held, the resident is neither swapped out
nor recycled, yet none of its `SERVER_PARALLEL` capacity is taken. An
explicit stop, or the proxy's restart of a dead server, still takes it. A rival
that only a held slot could take is passed over at the front of the line, so
it never blocks a request for another model that could go; it waits until its
own [`ADMISSION_DEADLINE`] and then gets the ordinary stall 503, which does not
mention the hold. An image model in that position is refused at once instead
([`Refusal::HeldSlot`]), naming the held model. A request for the
held model at another context gets a 503 at once; since VS Code's gateway
treats a 503 as final, waiting up to the deadline would serve it better.

# Two slots

[`SLOT_COUNT`] is 2. The second exists so a small auxiliary model — an embedder,
a title generator — can stay loaded instead of fighting the chat model for the
only slot. Whether a candidate may take it is decided by
[`decide_secondary_slot`](gglib_core::domain::decide_secondary_slot) against a
live free-VRAM reading; this module only asks.

An image model, served by `sd-server`, is placed differently
(`state_placement.rs`). It never takes an empty primary, where the next large
chat model would evict it. It takes the second slot when the memory check
grants it, or whenever the primary is empty, since then there is nothing to
share memory with; its verdict is judged by free memory alone, without the
ceiling that keeps large chat models in the swap path. Otherwise it swaps into
an evictable primary under the ordinary turn rules. The caller says which
program serves a request in its [`Candidate`].

# The generation gate

Residency says which models are loaded; the gate says which of them may
generate (`state_gate.rs` for the rules, `gate.rs` for the waiting). An image
render on `sd-server` needs the GPU to itself, LLM generations may share it,
and the queue hands out turns first come first served, in one order with the
tickets above. A render starts once no LLM turn is in flight and nothing
asked before it; LLM turns, and llama-server serves and launches, wait while a
render holds the GPU or waits ahead of them, so a stream of chats cannot
starve it.

Only requests on llama-server residents count as LLM turns. A render takes its
`sd-server` lease first and gives it to the turn, so its own lease never holds
it back, and requests for an `sd-server` resident skip the gate on the fast
path. Tickets waiting for a slot are not turns: an older chat waiting to evict
the slot a waiting render pins would otherwise wait on the render while the
render waited on it, the self-wait shape of
[#721](https://github.com/mmogr/gglib/issues/721) in another form.

A render step is progress, so nobody behind a long render reaches
[`ADMISSION_DEADLINE`] while it keeps stepping, and a gate waiter expires
under the same stall rule as a ticket. A render whose process had to be killed
is retired by `AdmissionQueue::retire_render`: the kill first, then one locked
release and eviction that touches the slot only while it still holds that
model, then the turn ends. A person's Stop on an image model a render holds
goes the same way: `ask_render_stop` notes it and empties nothing; a render
still waiting for its turn leaves the line with its lease, and the driver of
one that is drawing reads `render_stop_asked` at its next look at the job and
retires it. Requests through the proxy take leases and so count
already; the callers that take explicit turns arrive with image drawing.

The dashboard reads the gate as `AdmissionSnapshot::generation`: the render
holding it (its image model, step and total), the LLM turns in flight, and how
many callers wait for a turn, so a chat held behind a render is seen waiting
for its turn rather than for a slot.

# What this module is not responsible for

It does not launch, stop, or health-check anything, and it never touches a
process. It records what is resident and hands out decisions; the
[`residency`](crate::process::residency) module acts on them.

<!-- module-docs:end -->
