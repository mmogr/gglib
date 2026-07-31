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
let policy = RetryPolicy::default();
match retry::decide(&policy, attempt, retry_after, elapsed, rng.random()) {
    RetryDecision::Retry { after } => tokio::time::sleep(after).await,
    RetryDecision::GiveUp(reason) => return Err(anyhow!("{}", reason.as_str())),
}
```

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`policy.rs`](policy.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-policy-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-policy-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-policy-coverage.json) |
| [`policy_tests.rs`](policy_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-policy_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-policy_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-retry-policy_tests-coverage.json) |
<!-- module-table:end -->

</details>
