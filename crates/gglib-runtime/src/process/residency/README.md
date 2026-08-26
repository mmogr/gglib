# Residency

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-residency-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-residency-complexity.json)

<!-- module-docs:start -->

Turning admission decisions into running llama-server processes.

[`ResidentSet`] is the acting half of admission control. The
[`admission`](crate::process::admission) queue decides *what should happen* —
serve from a resident model, launch into a slot, wait, give up — and this
module makes it so: resolving the model, stopping what is being displaced,
spawning llama-server, waiting for health, and recording the result back into
the queue.

# The shape of one admission

Everything that depends only on the model, not on the schedule, is resolved
once before the request ever joins the queue:

| Step | Why up front |
|---|---|
| Pin check | A foreign model is refused without queueing behind, or displacing, the pinned one |
| Catalog lookup | An unknown model 404s immediately rather than after a swap |
| Context resolution | The resident-match test needs the context this request would launch with |
| Footprint estimate | The second-slot decision needs it, and it cannot change while queued |

What remains in the loop is purely scheduling. That split is what keeps the
launch sequence a straight line rather than a state machine.

# The launch options template

[`ResidentSet`] carries a standing [`ServerConfigOptions`] rather than a
hand-picked list of cache fields. Every launch resolves to:

```text
template  ⊕  per-call overrides  ⊕  this request's context chain
```

where `⊕` is [`ServerConfigOptions::overlay`]. A flag added to
`ServerConfigOptions` reaches llama-server through this path with no change
here at all.

# Two residents, three budgets

A co-loaded secondary must not be sized as though it had the machine to itself.
[`vram`] nets the primary's weights and KV out of the host-RAM figure the
secondary's `--cache-ram` is computed against, so two residents cannot each
claim the same memory.

It owns two device-memory questions besides. *May* a secondary load at all is
answered against a **live** free-VRAM reading, because that decision is made
once and acted on immediately. *How large a context* the primary is fitted to
is answered against **total capacity less a fixed reservation** for the second
slot — deliberately not a live reading and deliberately not the current
resident set, because the fitted context becomes part of a resident's identity
and a budget that moves evicts and relaunches the model it just sized.

The asymmetry is the point: one question tolerates a figure that changes,
the other cannot.

# What this module is not responsible for

It does not schedule. It never decides whose turn it is, when a swap is fair,
or whether a request has waited too long — every one of those questions belongs
to the queue, and this module only asks and obeys.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`launch.rs`](launch.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-residency-launch-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-residency-launch-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-residency-launch-coverage.json) |
| [`residency_tests.rs`](residency_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-residency-residency_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-residency-residency_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-residency-residency_tests-coverage.json) |
| [`spawned_child.rs`](spawned_child.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-residency-spawned_child-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-residency-spawned_child-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-residency-spawned_child-coverage.json) |
| [`vram.rs`](vram.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-residency-vram-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-residency-vram-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-residency-vram-coverage.json) |
<!-- module-table:end -->

</details>
