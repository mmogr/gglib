# Residency

<!-- module-docs:start -->

Turning admission decisions into running servers: llama-server for a model
that chats, stable-diffusion.cpp's `sd-server` for one that draws.

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
| Catalog lookup | An unknown model 404s immediately rather than after a swap |
| Pin check | A foreign model is refused without queueing behind, or displacing, the pinned one; it compares the resolved model's id, so the pin answers to its id and its name |
| Preflight | The weights and projector on disk; for an image model `sd-server` installed (`image_runtime_not_installed`), a file for every role its family needs (`image_model_incomplete`, naming them all), and each linked file on disk. Here because once `poll` grants a launch it has already forgotten the model being displaced |
| Context resolution | The resident-match test needs the context this request would launch with |
| Footprint estimate | The second-slot decision needs it, and it cannot change while queued |

An image model skips the context: it has none. It is placed by its files
and its family's compute margin (`vram::image_footprint`) under the queue's
image rules, and a held primary it cannot go beside refuses it at once as
`image_model_does_not_fit`.

What remains in the loop is purely scheduling. That split is what keeps the
launch sequence a straight line rather than a state machine.

An observed admission (`admit_observed`, which the image driver uses) is
told its place in line, 1 being next, each time the queue says wait and the
place has changed. The observer runs after the poll, outside the queue's
lock.

# One launch, either runtime

The launch stops the model it displaces **first**, before anything can
fail, then checks the files again (one removed while the request waited
fails here, the same for both runtimes), then branches only where the
runtimes differ: llama-server's KV types, prompt cache, disk slots and
command line, or `sd-server`'s recipe and its narration (runtime build,
family, each component, slot, memory). The spawn under the write lock, the
guard, the health race (`/health` or `/v1/models` by runtime) and the
install into the queue are shared. An `sd-server` resident is recycled when
a component is relinked, never for a context.

# The launch options template

[`ResidentSet`] carries a standing [`ServerConfigOptions`] rather than a
hand-picked list of cache fields. Every launch resolves to:

```text
template  ⊕  per-call overrides  ⊕  this request's context chain
```

where `⊕` is [`ServerConfigOptions::overlay`]. A flag added to
`ServerConfigOptions` reaches llama-server through this path with no change
here at all.

# A projector is part of a resident's identity

A model linked to a projector is launched with `--mmproj <projector>`. Like
the context size, the projector is fixed when llama-server starts, so a
resident launched with another projector than the request resolves to is
recycled, and the next pass launches the model with the one it is linked to
now: a changed link takes effect at the model's next admission (a proxy
request or a start), with no restart asked of anyone. A resident that a run
holds is kept, and that request refused. A run sent straight to a server
already started is not admitted here, and keeps the projector that server
was started with. A projector missing on disk fails the launch by name,
before the launch stops anything it displaces; a resident recycled for its
link was stopped before that, when the request found it.

# A Stop names a model, not a slot

`ResidentSet::stop_model` stops a model in whichever slot holds it, found and
emptied under one lock, then its process killed: a person's Stop on an image
model beside a chat model reaches the image model and leaves the chat model
running. A stop of the current model (`ResidentSet::stop_primary`: a
benchmark's, the proxy's cache clear and its restart of a dead server) stops
whatever the primary slot holds by that model's id, through the same path,
so neither empties a slot under a render.
A render's stop goes through `ProcessManager::retire_render` instead: a
render's lease released by slot after `stop_model` emptied that slot would
take a request from whatever model was launched there in between. So for an
image model a render holds, `stop_model` asks the render to stop
(`AdmissionQueue::ask_render_stop`) and waits, up to `RENDER_STOP_WAIT` (20
seconds), until no render holds it: a render waiting for its turn leaves the
line and drops its lease, after which the Stop empties the slot as usual,
and one that is drawing is retired by its driver within a second, which
empties the slot itself. A Stop that outwaits that answers an error and has
emptied nothing; the render stays asked.

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
