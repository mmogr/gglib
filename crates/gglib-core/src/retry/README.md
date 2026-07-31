# Retry

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-complexity.json)

<!-- module-docs:start -->

Shared backoff policy for retryable upstream failures.

[`decide`] is a pure function: the caller owns the clock (passing `elapsed`)
and the randomness (passing `jitter_unit`), so the policy has no dependency on
a timer, an RNG, or any I/O. Execution — sleeping, re-issuing the request —
belongs to the adapter layers that call it.

Two consumers share this one policy so that client and server agree on backoff
shape by construction rather than by convention:

- the in-process LLM completion adapter, retrying a `503` from the proxy, and
- the proxy itself, waiting out model-startup contention before it ever emits
  a `503` to an external OpenAI-compatible client.

# Delay derivation

Without a server hint the delay is **full jitter** — `random(0, min(cap,
base·2ⁿ))`. The failure mode being defended against is several clients
colliding on a single model's startup; full jitter is the variant that
decorrelates them most aggressively, where fixed backoff would have every
waiter wake together and collide again.

With a `Retry-After` the server's value becomes a **floor** rather than being
replaced by jitter — retrying earlier than the server asked only burns an
attempt against a resource known to be unready. Jitter of up to
`initial_backoff` is added on top so clients handed an identical `Retry-After`
still spread out, and the floor is clamped to `max_backoff` so a buggy upstream
cannot park a request indefinitely.

```ignore
let policy = RetryPolicy::from_env();
match retry::decide(&policy, attempt, retry_after, elapsed, retry::jitter_unit()) {
    RetryDecision::Retry { after } => tokio::time::sleep(after).await,
    RetryDecision::GiveUp(reason) => return Err(anyhow!("{}", reason.as_str())),
}
```

# Configuration

`RetryPolicy::default()` is the tuned budget — 4 attempts inside 60 s.
`RetryPolicy::from_env()` layers the `GGLIB_*` escape hatches over it, resolved
once per process:

| Variable | Overrides |
|---|---|
| `GGLIB_LLM_RETRY_MAX_ATTEMPTS` | attempts, including the first |
| `GGLIB_LLM_RETRY_DEADLINE_SECS` | wall-clock ceiling on the whole sequence |

An unset or unparseable value leaves that field alone, so a typo degrades to
standard behaviour rather than disabling retry. Shortening the deadline pulls
`max_backoff` down with it, because otherwise the very first backoff would
overrun the budget and silently turn retrying off for someone who thought they
were only tightening it.

`RetryPolicy::disabled()` is the one-attempt policy `gglib chat --no-retry`
resolves to — asked for by name rather than by knowing that `max_attempts: 1`
means "off".

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`env.rs`](env.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-env-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-env-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-env-coverage.json) |
| [`env_tests.rs`](env_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-env_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-env_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-env_tests-coverage.json) |
| [`jitter.rs`](jitter.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-jitter-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-jitter-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-jitter-coverage.json) |
| [`policy.rs`](policy.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-policy-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-policy-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-policy-coverage.json) |
| [`policy_tests.rs`](policy_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-policy_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-policy_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-policy_tests-coverage.json) |
<!-- module-table:end -->

</details>
