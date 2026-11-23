# Hub Limit

<!-- module-docs:start -->

Hugging Face's rate limit, as one fact, one clock and one sentence.

The Hub answers every request with a `RateLimit` header, `"api";r=485;t=142`:
which allowance the request counted against ([`HubBucket`]), how many requests
are left in it, and the seconds until its window resets. [`parse`] reads it
into a [`HubLimit`]. A refusal (429) that carries no usable header is read by
[`refusal`] from its `Retry-After`, with the allowance taken from the address
asked.

[`HubGate`] keeps, per allowance, the moment it opens again: a refusal closes
it, and so does an answer that says none are left, since the next request
would be refused. One gate can be shared by several clients. A client asks
the gate before it sends, so a closed allowance is not asked again until its
reset. Time is a parameter of every call, so a test passes the moments it
means.

What a client does at a limit is its [`OnLimit`]: a daemon, and anything else
that answers requests, surfaces it at once, with the time it lifts; a
one-shot command waits it out ([`wait_out`]) and is told each second, through
its [`LimitNotice`], the sentence to show: [`waiting_sentence`], "waiting for
Hugging Face's rate limit, 2:14". That is the one wording every surface shows.

The bounds, so nothing waits for ever, are named constants: a reset is clamped
to [`MAX_RESET`], a refusal with no time waits [`HEADERLESS_WAIT`], and a
caller gives up after [`LIMITED_IN_A_ROW`] refusals in a row.

It is **not** responsible for sending anything: no HTTP client is here.
`gglib-hf` reads the headers off its responses and keeps the gate.

<!-- module-docs:end -->
