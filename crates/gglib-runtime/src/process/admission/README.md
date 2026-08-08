# Admission

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-admission-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-admission-complexity.json)

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

This is why admission returns a lease rather than just a target — see
[`AdmissionLease`](gglib_core::ports::AdmissionLease).

# Two slots

[`SLOT_COUNT`] is 2. The second exists so a small auxiliary model — an embedder,
a title generator — can stay loaded instead of fighting the chat model for the
only slot. Whether a candidate may take it is decided by
[`decide_secondary_slot`](gglib_core::domain::decide_secondary_slot) against a
live free-VRAM reading; this module only asks.

# What this module is not responsible for

It does not launch, stop, or health-check anything, and it never touches a
process. It records what is resident and hands out decisions; the
[`residency`](crate::process::residency) module acts on them.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`lease.rs`](lease.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-admission-lease-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-admission-lease-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-admission-lease-coverage.json) |
| [`queue_tests.rs`](queue_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-admission-queue_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-admission-queue_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-admission-queue_tests-coverage.json) |
| [`state.rs`](state.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-admission-state-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-admission-state-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-admission-state-coverage.json) |
<!-- module-table:end -->

</details>
