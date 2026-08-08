//! Scheduling behaviour of the admission queue.
//!
//! These drive [`AdmissionQueue`] directly rather than through a
//! `ProcessManager`, so every rule is exercised without spawning a process. The
//! queue's decisions are a pure function of its state plus the clock, which is
//! what makes that possible — and what makes `tokio::time::pause()` enough to
//! test the two fairness bounds that are otherwise minutes long.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::Instant;

use gglib_core::domain::{CacheRamHealth, SecondarySlotDecision};

use super::*;

// ── fixtures ──────────────────────────────────────────────────────────────

fn queue() -> Arc<AdmissionQueue> {
    Arc::new(AdmissionQueue::new())
}

fn resident(model_id: u32, name: &str) -> Resident {
    Resident {
        model_id,
        model_name: name.to_string(),
        context_size: 4096,
        port: 8000 + u16::try_from(model_id).unwrap_or(0),
        model_path: PathBuf::from("/models/x.gguf"),
        slot_restore_supported: true,
        cache_ram_health: CacheRamHealth::LlamaDefault,
        narration: None,
        inflight: 0,
        resident_since: Instant::now(),
        weights_bytes: 512 * 1024 * 1024,
    }
}

/// The common case for these tests: nothing may co-reside, so every model
/// change is a swap and the fairness rules are what decide it.
const NEVER_FITS: SecondarySlotDecision = SecondarySlotDecision::RefuseTooLarge {
    footprint_bytes: 9 * 1024 * 1024 * 1024,
    ceiling_bytes: 2 * 1024 * 1024 * 1024,
};

/// The co-residency case: anything fits, so the second slot is always offered.
const ALWAYS_FITS: SecondarySlotDecision = SecondarySlotDecision::Grant {
    footprint_bytes: 256 * 1024 * 1024,
    headroom_bytes: 8 * 1024 * 1024 * 1024,
};

/// Enqueue and immediately poll, the shape every requester's first iteration
/// takes. Returns the ticket so the caller can keep polling it.
fn request(q: &Arc<AdmissionQueue>, model: &str) -> (Ticket, AdmissionDecision) {
    let ticket = q.enqueue(model);
    let decision = q.poll(&ticket, NEVER_FITS);
    (ticket, decision)
}

/// Drive a model all the way to resident in `slot`, returning its lease.
fn make_resident(
    q: &Arc<AdmissionQueue>,
    model: &str,
    model_id: u32,
) -> gglib_core::ports::AdmissionLease {
    let (ticket, decision) = request(q, model);
    let AdmissionDecision::Launch { slot, .. } = decision else {
        panic!("expected a launch for {model}, got {decision:?}");
    };
    drop(ticket);
    q.install(slot, resident(model_id, model))
}

// ── the basics ────────────────────────────────────────────────────────────

/// A cold start has an empty primary slot, so the first request launches into
/// it without any fairness question arising.
#[tokio::test]
async fn the_first_request_launches_into_the_primary_slot() {
    let q = queue();
    let (_ticket, decision) = request(&q, "qwen-coder");

    assert_eq!(
        decision,
        AdmissionDecision::Launch {
            slot: PRIMARY_SLOT,
            evict: None
        }
    );
}

/// The steady state: once a model is resident and its slot is free, requests
/// are served without ever consulting the scheduler.
#[tokio::test]
async fn a_resident_model_serves_immediately() {
    let q = queue();
    // The launching request finishes, leaving the slot free for the next one.
    drop(make_resident(&q, "qwen-coder", 1));

    let (_ticket, decision) = request(&q, "qwen-coder");
    assert_eq!(decision, AdmissionDecision::Serve { slot: PRIMARY_SLOT });
}

/// A resident model with no rival waiting is limited only by its own capacity,
/// never by the fairness rules — those exist to arbitrate between models, and
/// there is only one here.
#[tokio::test]
async fn a_resident_model_is_not_throttled_without_a_rival() {
    tokio::time::pause();
    let q = queue();
    drop(make_resident(&q, "qwen-coder", 1));

    // Well past every fairness bound, with nothing to be fair to.
    tokio::time::advance(DRAIN_QUANTUM * 4).await;

    for _ in 0..5 {
        let (ticket, decision) = request(&q, "qwen-coder");
        let AdmissionDecision::Serve { slot } = decision else {
            panic!("a lone resident model must keep serving, got {decision:?}");
        };
        drop((ticket, q.claim(slot)));
    }
}

/// The excess above what llama-server can start waits *here*, where the
/// dashboard can see it. Before the cap it was forwarded anyway and queued
/// inside llama-server instead, so a real backlog reported a depth of zero.
#[tokio::test]
async fn the_backlog_is_visible_while_it_waits() {
    let q = queue();
    let _serving = make_resident(&q, "qwen-coder", 1);
    let (_t1, first) = request(&q, "qwen-coder");
    let (_t2, second) = request(&q, "qwen-coder");

    assert_eq!(first, AdmissionDecision::Wait, "the slot is already busy");
    assert_eq!(second, AdmissionDecision::Wait);
    assert_eq!(
        q.snapshot().waiting(),
        2,
        "a backlog the scheduler cannot see is one it cannot drain"
    );
}

/// Two requesters must not both be told to launch the same model — that would
/// be two llama-servers fighting for one port and one card.
#[tokio::test]
async fn only_one_requester_drives_a_given_launch() {
    let q = queue();
    let (_first, first_decision) = request(&q, "qwen-coder");
    let (_second, second_decision) = request(&q, "qwen-coder");

    assert!(matches!(first_decision, AdmissionDecision::Launch { .. }));
    assert_eq!(second_decision, AdmissionDecision::Wait);
}

/// The waiter behind a launch is served by it, rather than launching again —
/// but it takes its turn rather than piling in alongside the request that drove
/// the launch, since the fresh instance can only serve one of them at a time.
#[tokio::test]
async fn a_waiter_is_served_by_the_launch_it_waited_for() {
    let q = queue();
    let (first, _) = request(&q, "qwen-coder");
    let (second, _) = request(&q, "qwen-coder");
    drop(first);

    let launcher = q.install(PRIMARY_SLOT, resident(1, "qwen-coder"));

    assert_eq!(
        q.poll(&second, NEVER_FITS),
        AdmissionDecision::Wait,
        "the launching request still holds the only slot"
    );

    drop(launcher);
    assert_eq!(
        q.poll(&second, NEVER_FITS),
        AdmissionDecision::Serve { slot: PRIMARY_SLOT },
        "and is served by that launch rather than driving a second one"
    );
    assert_eq!(q.snapshot().total_swaps, 0, "a cold start is not a swap");
}

// ── the invariant: no preemption ──────────────────────────────────────────

/// The guarantee the whole lease mechanism exists for. A model with a request
/// in flight is not evictable at any price — a swap that cut off a live
/// generation would be worse than any wait.
#[tokio::test]
async fn a_slot_with_a_request_in_flight_is_never_evicted() {
    let q = queue();
    let lease = make_resident(&q, "qwen-coder", 1);

    let (rival, decision) = request(&q, "nomic-embed");
    assert_eq!(
        decision,
        AdmissionDecision::Wait,
        "must not evict a busy slot"
    );

    // Even once every fairness bound has been blown through.
    tokio::time::pause();
    tokio::time::advance(DRAIN_QUANTUM * 4).await;
    assert_eq!(q.poll(&rival, NEVER_FITS), AdmissionDecision::Wait);

    // The instant the lease drops, the swap is legal.
    drop(lease);
    assert_eq!(
        q.poll(&rival, NEVER_FITS),
        AdmissionDecision::Launch {
            slot: PRIMARY_SLOT,
            evict: Some(1),
        }
    );
}

/// Leases are counted, not latched: two concurrent requests both have to
/// finish before the slot frees.
///
/// Takes the second lease through `lease()` rather than through admission,
/// which deliberately sidesteps the `SERVER_PARALLEL` cap — the release
/// accounting has to be right whatever the count reached, and the cap is not
/// what this test is about.
#[tokio::test]
async fn a_slot_frees_only_when_the_last_lease_drops() {
    let q = queue();
    let first = make_resident(&q, "qwen-coder", 1);
    let second = q.lease(PRIMARY_SLOT).expect("resident slot leases");

    let (rival, _) = request(&q, "nomic-embed");
    drop(first);
    assert_eq!(
        q.poll(&rival, NEVER_FITS),
        AdmissionDecision::Wait,
        "one lease still outstanding"
    );

    drop(second);
    assert!(matches!(
        q.poll(&rival, NEVER_FITS),
        AdmissionDecision::Launch { .. }
    ));
}

// ── capacity: what llama-server can actually start ────────────────────────

/// llama-server is launched with `--parallel 1`, so admitting a second
/// concurrent request would not serve it any sooner — it would only move the
/// wait inside llama-server, out of the queue's sight.
#[tokio::test]
async fn a_resident_slot_admits_only_what_llama_server_can_serve() {
    let q = queue();
    let serving = make_resident(&q, "qwen-coder", 1);

    let (ticket, decision) = request(&q, "qwen-coder");
    assert_eq!(
        decision,
        AdmissionDecision::Wait,
        "the instance is already serving all it can"
    );

    drop(serving);
    assert_eq!(
        q.poll(&ticket, NEVER_FITS),
        AdmissionDecision::Serve { slot: PRIMARY_SLOT },
        "and is admitted the moment that finishes"
    );
}

/// The regression this whole exercise is about.
///
/// An agentic client issues its next request the instant the previous one
/// returns, so the model never goes idle for longer than it takes to hand over.
/// That used to pin the slot outright: every one of those requests was admitted
/// on arrival, `inflight` never reached zero, and the rival waited out
/// [`ADMISSION_DEADLINE`] for a 503 while the GPU sat there serving one request
/// at a time.
///
/// The incumbent must now stand aside once the rival is entitled to the slot,
/// rather than renewing its lease ahead of it.
#[tokio::test]
async fn a_pipelined_client_cannot_pin_a_model_forever() {
    tokio::time::pause();
    let q = queue();
    let in_flight = make_resident(&q, "qwen-coder", 1);

    // The rival arrives and waits out the incumbent's turn.
    let (rival, _) = request(&q, "nomic-embed");
    tokio::time::advance(DRAIN_QUANTUM + std::time::Duration::from_secs(1)).await;

    // The pipelined client has its next request queued before the current one
    // returns, which is exactly how it kept the slot pinned.
    let (pipelined, _) = request(&q, "qwen-coder");
    drop(in_flight);

    assert_eq!(
        q.poll(&pipelined, NEVER_FITS),
        AdmissionDecision::Wait,
        "the incumbent must not renew its lease over a rival's turn"
    );
    assert!(
        matches!(
            q.poll(&rival, NEVER_FITS),
            AdmissionDecision::Launch {
                slot: PRIMARY_SLOT,
                evict: Some(1)
            }
        ),
        "the rival must get the slot the quantum promised it"
    );
}

/// A request held back by the cap is waiting like any other, so the deadline
/// has to reach it. The capacity test sits above the deadline check in `poll`,
/// and an early return there would leave it waiting forever with nothing to
/// surface a 503.
#[tokio::test]
async fn a_capped_request_still_expires() {
    tokio::time::pause();
    let q = queue();
    let _never_released = make_resident(&q, "qwen-coder", 1);

    let (capped, decision) = request(&q, "qwen-coder");
    assert_eq!(decision, AdmissionDecision::Wait);

    tokio::time::advance(ADMISSION_DEADLINE).await;
    assert_eq!(q.poll(&capped, NEVER_FITS), AdmissionDecision::Expired);
}

// ── batching: the point of the exercise ───────────────────────────────────

/// The headline behaviour. Ten queued requests for a model that is not loaded
/// must cost exactly one swap between them, not one each.
#[tokio::test]
async fn a_burst_of_queued_requests_costs_a_single_swap() {
    let q = queue();
    let outgoing = make_resident(&q, "qwen-coder", 1);
    drop(outgoing); // idle, so the swap is permitted

    let tickets: Vec<Ticket> = (0..10).map(|_| q.enqueue("nomic-embed")).collect();

    // The first one through drives the launch; the rest wait behind it.
    let first = q.poll(&tickets[0], NEVER_FITS);
    let AdmissionDecision::Launch { slot, evict } = first else {
        panic!("expected a launch, got {first:?}");
    };
    assert_eq!(evict, Some(1));
    for ticket in &tickets[1..] {
        assert_eq!(q.poll(ticket, NEVER_FITS), AdmissionDecision::Wait);
    }

    // The launching request is served by its own launch, then finishes.
    drop(q.install(slot, resident(2, "nomic-embed")));

    // Every remaining request is served by that same launch, one at a time —
    // the instance serves one at a time, so they take turns rather than
    // overlapping, but not one of them pays for another swap.
    for ticket in &tickets[1..] {
        assert_eq!(
            q.poll(ticket, NEVER_FITS),
            AdmissionDecision::Serve { slot }
        );
        drop(q.claim(slot));
    }

    assert_eq!(
        q.snapshot().total_swaps,
        1,
        "ten requests must not cost ten swaps"
    );
}

/// Alternating traffic is the case that motivated M9. Two models, ten requests
/// each arriving interleaved, must not cost twenty swaps.
#[tokio::test]
async fn alternating_traffic_does_not_swap_per_request() {
    tokio::time::pause();
    let q = queue();
    let mut tickets = Vec::new();
    for _ in 0..10 {
        tickets.push(q.enqueue("qwen-coder"));
        tickets.push(q.enqueue("nomic-embed"));
    }

    // Drive the queue to completion, serving whatever it grants.
    let mut guard = 0;
    while !tickets.is_empty() && guard < 500 {
        guard += 1;
        let mut remaining = Vec::new();
        let mut progressed = false;
        for ticket in tickets {
            match q.poll(&ticket, NEVER_FITS) {
                // `Serve` has already counted this request against the slot, so
                // the lease must be claimed — exactly as a real requester does.
                // Leaving it unclaimed would pin the model resident forever.
                // Dropping it straight away stands in for the request finishing.
                AdmissionDecision::Serve { slot } => {
                    drop(q.claim(slot));
                    progressed = true;
                }
                AdmissionDecision::Launch { slot, .. } => {
                    drop(q.install(slot, resident(if slot == 0 { 1 } else { 2 }, &ticket.model)));
                    progressed = true;
                }
                AdmissionDecision::Wait => remaining.push(ticket),
                AdmissionDecision::Expired => panic!("nothing should expire here"),
            }
        }
        tickets = remaining;
        // Let the clock run only when a round achieved nothing. Advancing it
        // unconditionally would end the turn holder's quantum after every
        // single request, which is precisely the per-request swapping this
        // test exists to rule out.
        if !tickets.is_empty() && !progressed {
            tokio::time::advance(DRAIN_QUANTUM + std::time::Duration::from_secs(1)).await;
        }
    }

    assert!(tickets.is_empty(), "the queue must drain");
    let swaps = q.snapshot().total_swaps;
    assert!(
        swaps <= 2,
        "twenty alternating requests cost {swaps} swaps — each model should \
         drain its whole batch within one turn"
    );
}

// ── fairness ──────────────────────────────────────────────────────────────

/// A turn holder with a continuous stream of its own work still has to yield
/// once its quantum is up, or the rival never runs.
#[tokio::test]
async fn the_turn_holder_yields_once_its_quantum_elapses() {
    tokio::time::pause();
    let q = queue();
    let lease = make_resident(&q, "qwen-coder", 1);

    // The turn holder keeps work queued throughout, so "queue empty" never
    // fires and only the quantum can end the turn.
    let _busy = q.enqueue("qwen-coder");
    let (rival, _) = request(&q, "nomic-embed");
    drop(lease);

    assert_eq!(
        q.poll(&rival, NEVER_FITS),
        AdmissionDecision::Wait,
        "the turn is still young"
    );

    tokio::time::advance(DRAIN_QUANTUM + std::time::Duration::from_secs(1)).await;

    assert!(
        matches!(q.poll(&rival, NEVER_FITS), AdmissionDecision::Launch { .. }),
        "the quantum must end the turn"
    );
}

/// Arrival order, not queue depth, decides who is next — so a model with a
/// hundred requests behind it cannot bury a model with one older request. This
/// is what bounds the wait to roughly one quantum rather than to however long
/// the busier model stays busy.
#[tokio::test]
async fn a_deep_queue_does_not_outrank_a_single_older_request() {
    tokio::time::pause();
    let q = queue();
    let lease = make_resident(&q, "qwen-coder", 1);

    let (lonely, _) = request(&q, "nomic-embed");
    let _crowd: Vec<Ticket> = (0..50).map(|_| q.enqueue("qwen-coder")).collect();
    drop(lease);

    tokio::time::advance(DRAIN_QUANTUM + std::time::Duration::from_secs(1)).await;

    assert!(
        matches!(
            q.poll(&lonely, NEVER_FITS),
            AdmissionDecision::Launch { .. }
        ),
        "fifty newer requests must not bury one older one"
    );
}

/// The hard cap, and the only bound that holds unconditionally. A model under
/// continuous overlapping load never reaches zero in-flight, so no swap is ever
/// legal and the quantum has nothing to end; the deadline is what stops the
/// rival waiting forever.
#[tokio::test]
async fn a_permanently_busy_slot_expires_the_rival_rather_than_stalling_it() {
    tokio::time::pause();
    let q = queue();
    let _never_released = make_resident(&q, "qwen-coder", 1);

    let (rival, _) = request(&q, "nomic-embed");
    tokio::time::advance(DRAIN_QUANTUM * 4).await;
    assert_eq!(
        q.poll(&rival, NEVER_FITS),
        AdmissionDecision::Wait,
        "still blocked, because preemption is never an option"
    );

    tokio::time::advance(ADMISSION_DEADLINE).await;
    assert_eq!(q.poll(&rival, NEVER_FITS), AdmissionDecision::Expired);
}

/// A turn holder that has run out of work does not get to keep the slot warm
/// against a waiting rival.
#[tokio::test]
async fn an_idle_turn_holder_yields_immediately() {
    let q = queue();
    let lease = make_resident(&q, "qwen-coder", 1);
    drop(lease);

    let (rival, decision) = request(&q, "nomic-embed");
    assert!(
        matches!(decision, AdmissionDecision::Launch { evict: Some(1), .. }),
        "got {decision:?}"
    );
    drop(rival);
}

/// Arrival order decides which model is up next, so a model with many requests
/// cannot repeatedly jump ahead of one with a single older request.
#[tokio::test]
async fn the_globally_oldest_waiter_decides_which_model_is_next() {
    let q = queue();
    let lease = make_resident(&q, "qwen-coder", 1);
    drop(lease);

    let older = q.enqueue("model-a");
    let newer = q.enqueue("model-b");

    assert_eq!(
        q.poll(&newer, NEVER_FITS),
        AdmissionDecision::Wait,
        "a later arrival must not overtake"
    );
    assert!(matches!(
        q.poll(&older, NEVER_FITS),
        AdmissionDecision::Launch { .. }
    ));
}

// ── the second slot ───────────────────────────────────────────────────────

/// The co-residency payoff: a model that fits takes the empty second slot
/// instead of evicting the primary.
#[tokio::test]
async fn a_model_that_fits_co_loads_rather_than_swapping() {
    let q = queue();
    let _lease = make_resident(&q, "qwen-coder", 1);

    let ticket = q.enqueue("nomic-embed");
    let decision = q.poll(&ticket, ALWAYS_FITS);

    assert_eq!(
        decision,
        AdmissionDecision::Launch {
            slot: PRIMARY_SLOT + 1,
            evict: None,
        }
    );
    assert_eq!(q.snapshot().total_swaps, 0, "a co-load is not a swap");
}

/// A co-resident model is admitted while the primary is mid-generation —
/// the case the whole second slot exists for.
#[tokio::test]
async fn co_resident_models_never_contend() {
    let q = queue();
    let chat = make_resident(&q, "qwen-coder", 1);

    let ticket = q.enqueue("nomic-embed");
    let AdmissionDecision::Launch { slot, .. } = q.poll(&ticket, ALWAYS_FITS) else {
        panic!("expected a co-load");
    };
    drop(ticket);
    let embed = q.install(slot, resident(2, "nomic-embed"));

    // The co-load happened while the primary was mid-generation, which is the
    // whole point of the second slot. Both launching requests now finish.
    drop((chat, embed));

    // Each slot has its own capacity, so both models serve at the same time —
    // neither one's cap has anything to say about the other's.
    let (chat_again, chat_decision) = request(&q, "qwen-coder");
    let (embed_again, embed_decision) = request(&q, "nomic-embed");
    assert_eq!(
        chat_decision,
        AdmissionDecision::Serve { slot: PRIMARY_SLOT }
    );
    assert_eq!(embed_decision, AdmissionDecision::Serve { slot });
    assert_eq!(q.snapshot().total_swaps, 0);

    drop((chat_again, embed_again));
}

/// VRAM overflow is not a failure — it falls back to the swap path, which is
/// exactly the pre-M9 behaviour and always correct.
#[tokio::test]
async fn a_model_that_does_not_fit_falls_back_to_swapping() {
    let q = queue();
    let lease = make_resident(&q, "qwen-coder", 1);
    drop(lease);

    let ticket = q.enqueue("llama-70b");
    let decision = q.poll(&ticket, NEVER_FITS);

    assert_eq!(
        decision,
        AdmissionDecision::Launch {
            slot: PRIMARY_SLOT,
            evict: Some(1),
        },
        "must swap rather than co-load"
    );
    assert_eq!(q.snapshot().total_swaps, 1);
}

/// With both slots full, the secondary is what gets sacrificed — losing the
/// model chat traffic follows would be the wrong trade.
#[tokio::test]
async fn a_third_model_evicts_the_secondary_not_the_primary() {
    let q = queue();
    let chat = make_resident(&q, "qwen-coder", 1);
    let embed_ticket = q.enqueue("nomic-embed");
    let AdmissionDecision::Launch { slot, .. } = q.poll(&embed_ticket, ALWAYS_FITS) else {
        panic!("expected a co-load");
    };
    drop(embed_ticket);
    let embed = q.install(slot, resident(2, "nomic-embed"));
    drop((chat, embed));

    let third = q.enqueue("reranker");
    assert_eq!(
        q.poll(&third, ALWAYS_FITS),
        AdmissionDecision::Launch {
            slot: PRIMARY_SLOT + 1,
            evict: Some(2),
        }
    );
}

// ── giving up ─────────────────────────────────────────────────────────────

/// A request that never reaches the front gives up with a deadline rather than
/// holding a connection open forever.
#[tokio::test]
async fn a_request_expires_once_its_deadline_passes() {
    tokio::time::pause();
    let q = queue();
    let _held = make_resident(&q, "qwen-coder", 1); // never released

    let (rival, _) = request(&q, "nomic-embed");
    tokio::time::advance(ADMISSION_DEADLINE + std::time::Duration::from_secs(1)).await;

    assert_eq!(q.poll(&rival, NEVER_FITS), AdmissionDecision::Expired);
}

/// An expired or abandoned request must stop holding the front of the queue,
/// or a departed client would go on forcing swaps nobody needs.
#[tokio::test]
async fn an_abandoned_request_stops_forcing_swaps() {
    let q = queue();
    let lease = make_resident(&q, "qwen-coder", 1);
    let (rival, _) = request(&q, "nomic-embed");

    assert_eq!(q.snapshot().waiting(), 1);
    q.abandon(&rival);
    assert_eq!(q.snapshot().waiting(), 0);

    drop(lease);
    // Nothing is waiting, so nothing displaces the resident model.
    assert_eq!(q.snapshot().slots.len(), 1);
    assert_eq!(q.snapshot().slots[0].model_name, "qwen-coder");
}

/// A failed launch must not leave the slot latched in `Loading` — the next
/// request has to be able to try again.
#[tokio::test]
async fn a_failed_launch_frees_the_slot_for_another_attempt() {
    let q = queue();
    let (first, decision) = request(&q, "qwen-coder");
    let AdmissionDecision::Launch { slot, .. } = decision else {
        panic!("expected a launch");
    };
    drop(first);

    q.launch_failed(slot);

    let (_retry, retry_decision) = request(&q, "qwen-coder");
    assert!(
        matches!(retry_decision, AdmissionDecision::Launch { .. }),
        "got {retry_decision:?}"
    );
}

// ── telemetry ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_snapshot_reports_residency_and_queue_depth() {
    let q = queue();
    let _lease = make_resident(&q, "qwen-coder", 1);
    let _a = q.enqueue("nomic-embed");
    let _b = q.enqueue("nomic-embed");

    let snapshot = q.snapshot();
    assert_eq!(snapshot.slots.len(), 1);
    assert_eq!(snapshot.slots[0].model_name, "qwen-coder");
    assert!(snapshot.slots[0].is_primary);
    assert_eq!(snapshot.slots[0].inflight, 1);
    assert_eq!(snapshot.waiting(), 2);
    assert_eq!(snapshot.queued[0].model_name, "nomic-embed");
    assert_eq!(snapshot.queued[0].waiting, 2);
}

/// A co-resident secondary explains itself by name, so the dashboard can say
/// which model is enjoying the slot.
#[tokio::test]
async fn the_snapshot_names_a_co_resident_secondary() {
    let q = queue();
    let _chat = make_resident(&q, "qwen-coder", 1);
    let ticket = q.enqueue("nomic-embed");
    let AdmissionDecision::Launch { slot, .. } = q.poll(&ticket, ALWAYS_FITS) else {
        panic!("expected a co-load");
    };
    drop(ticket);
    let _embed = q.install(slot, resident(2, "nomic-embed"));

    let snapshot = q.snapshot();
    assert_eq!(snapshot.secondary_slot.state, "resident");
    assert!(snapshot.secondary_slot.detail.contains("nomic-embed"));
    assert_eq!(snapshot.slots.len(), 2);
    assert!(!snapshot.slots[1].is_primary);
}

/// The verdict handed to `poll` lands in the snapshot whenever a secondary
/// slot could have used it, so an idle second slot can explain itself on the
/// dashboard. (The verdict is a value computed by the caller before the queue
/// locks — the regression guarded here is #721, where a callback ran under the
/// lock and re-entered the queue.)
#[tokio::test]
async fn a_consulted_refusal_is_recorded_for_the_dashboard() {
    let q = queue();
    let _lease = make_resident(&q, "qwen-coder", 1);

    // Secondary slot is empty, so the refusal is consulted — and recorded.
    let (_rival, _) = request(&q, "llama-70b");

    assert_eq!(q.snapshot().secondary_slot.state, "too_large");
}

/// A grant that has not finished loading holds nothing, so it reports the slot
/// as available rather than resident.
#[tokio::test]
async fn a_consulted_grant_reports_the_slot_as_available() {
    let q = queue();
    let _lease = make_resident(&q, "qwen-coder", 1);

    let ticket = q.enqueue("nomic-embed");
    let AdmissionDecision::Launch { .. } = q.poll(&ticket, ALWAYS_FITS) else {
        panic!("expected a co-load");
    };

    assert_eq!(q.snapshot().secondary_slot.state, "available");
}

/// A verdict for a moment when no secondary slot was on offer never mattered,
/// so it must not overwrite the last one that did.
#[tokio::test]
async fn an_unconsulted_verdict_does_not_overwrite_the_recorded_one() {
    let q = queue();
    let _chat = make_resident(&q, "qwen-coder", 1);

    // A granted co-load leaves the secondary slot `Loading` — neither empty
    // nor evictable, so nothing consults a verdict while it stays that way.
    let embed = q.enqueue("nomic-embed");
    let AdmissionDecision::Launch { .. } = q.poll(&embed, ALWAYS_FITS) else {
        panic!("expected a co-load");
    };
    drop(embed);

    let (_rival, _) = request(&q, "llama-70b"); // NEVER_FITS, unconsulted

    assert_eq!(
        q.snapshot().secondary_slot.state,
        "available",
        "the refusal was never consulted and must not be recorded"
    );
}

/// Every request that passes through is counted, so the ratio against
/// `total_swaps` shows the batching actually working.
#[tokio::test]
async fn total_queued_counts_every_request_including_fast_path_ones() {
    let q = queue();
    let _lease = make_resident(&q, "qwen-coder", 1);
    for _ in 0..5 {
        let (_t, _) = request(&q, "qwen-coder");
    }

    assert_eq!(q.snapshot().total_queued, 6, "the launch plus five serves");
}

// ── waking ────────────────────────────────────────────────────────────────

/// A released lease is the event the scheduler has been waiting for, so it must
/// wake subscribers rather than leaving them to time out.
#[tokio::test]
async fn releasing_a_lease_wakes_subscribers() {
    let q = queue();
    let lease = make_resident(&q, "qwen-coder", 1);

    let waiter = Arc::clone(&q);
    let handle = tokio::spawn(async move {
        let changed = waiter.subscribe();
        tokio::pin!(changed);
        changed.as_mut().enable();
        changed.await;
    });

    // Give the subscriber a chance to register before the wakeup fires.
    tokio::task::yield_now().await;
    drop(lease);

    tokio::time::timeout(std::time::Duration::from_secs(5), handle)
        .await
        .expect("a released lease must wake subscribers")
        .expect("waiter task panicked");
}

/// The subscription is enabled before the state is read, so a change that lands
/// in between is still delivered. Without that ordering a waiter would sit out
/// its whole timeout on a wakeup it had already missed.
#[tokio::test]
async fn a_wakeup_between_subscribing_and_waiting_is_not_lost() {
    let q = queue();
    let lease = make_resident(&q, "qwen-coder", 1);

    let changed = q.subscribe();
    tokio::pin!(changed);
    changed.as_mut().enable();

    // Fires *after* the subscription but *before* anything awaits it.
    drop(lease);

    tokio::time::timeout(std::time::Duration::from_secs(5), changed)
        .await
        .expect("the wakeup must survive the gap between subscribing and awaiting");
}
