//! Admission: the context a model is loaded at, the models it refuses, and
//! the one model it holds.

use std::time::Duration;

use gglib_core::cache_config::{DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V};
use gglib_core::domain::ImageFamily;
use gglib_core::domain::phone_fit::{FitVerdict, fit_verdict};
use gglib_core::ports::{LaunchOverrides, ModelRuntimeError, ModelRuntimePort};
use gglib_core::services::total_model_bytes;

use super::test_support::{BUDGET, GIB, Shelf, hello, model, read, runtime, slow};
use crate::state::EngineState;

#[tokio::test]
async fn a_model_is_loaded_at_the_rung_its_budget_fits_and_served_by_the_shim() {
    let shelf = Shelf::new();
    let small = shelf.model(7, "small", GIB);
    let weights = small.file_path.display().to_string();
    let (runtime, record) = runtime(hello(), vec![small]).await;
    let admission = runtime
        .admit("small", None, None, LaunchOverrides::default())
        .await
        .expect("a model that fits is admitted");

    let target = &admission.target;
    assert_eq!(target.effective_ctx, 8192);
    assert_eq!(target.model_id, 7);
    assert_eq!(target.model_name, "small");
    assert!(target.just_started);
    let (path, params) = read(&record).loads.pop().expect("one load");
    assert_eq!(path, weights);
    assert_eq!(params.n_ctx, 8192);
    // The cache the fit sized is the cache that is made.
    assert_eq!(
        (params.cache_type_k, params.cache_type_v),
        (DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V)
    );
    assert_eq!(
        runtime.state(),
        EngineState::Ready {
            id: 7,
            model: "small".to_owned(),
            n_ctx: 8192
        }
    );

    // The target is the shim, and the shim serves the model admitted.
    let props: serde_json::Value = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("{}/props", target.base_url))
        .send()
        .await
        .expect("the shim answers")
        .json()
        .await
        .expect("JSON");
    assert_eq!(props["model_alias"], "small");
    assert_eq!(props["default_generation_settings"]["n_ctx"], 8192);
    let current = runtime.current_model().await.expect("a model is running");
    assert_eq!(current.port, target.port);
    assert!(!current.just_started);
}

#[tokio::test]
async fn a_model_too_big_for_the_budget_is_refused_before_anything_loads() {
    let shelf = Shelf::new();
    let (runtime, record) = runtime(hello(), vec![shelf.model(1, "big", 3 * GIB)]).await;
    let refused = runtime
        .admit("big", None, None, LaunchOverrides::default())
        .await;

    match refused {
        Err(ModelRuntimeError::ChatModelDoesNotFit {
            needed_bytes,
            budget_bytes,
        }) => {
            assert_eq!(budget_bytes, BUDGET);
            assert!(needed_bytes > BUDGET);
        }
        other => panic!("refused as too big, not {other:?}"),
    }
    assert!(read(&record).loads.is_empty());
    assert_eq!(runtime.state(), EngineState::Idle);
    assert!(runtime.current_model().await.is_none());
}

#[tokio::test]
async fn the_budget_and_the_model_held_are_the_same_across_admissions() {
    let shelf = Shelf::new();
    let (runtime, record) = runtime(hello(), vec![shelf.model(1, "small", GIB)]).await;
    let first = runtime
        .admit("small", None, None, LaunchOverrides::default())
        .await
        .unwrap();
    // By id this time, and with the proxy's default context, which the
    // runtime does not read.
    let second = runtime
        .admit("1", None, Some(4096), LaunchOverrides::default())
        .await
        .unwrap();

    assert_eq!(runtime.budget_bytes(), Some(BUDGET));
    assert_eq!(first.target.effective_ctx, 8192);
    assert_eq!(second.target.effective_ctx, 8192);
    assert!(
        !second.target.just_started,
        "the model held is served again"
    );
    assert_eq!(read(&record).loads.len(), 1);
}

#[tokio::test]
async fn a_second_model_frees_the_first_before_it_loads() {
    let shelf = Shelf::new();
    let models = vec![
        shelf.model(1, "first", GIB),
        shelf.model(2, "second", 3 * GIB / 2),
    ];
    let files: Vec<String> = models
        .iter()
        .map(|m| m.file_path.display().to_string())
        .collect();
    let (runtime, record) = runtime(hello(), models).await;
    runtime
        .admit("first", None, None, LaunchOverrides::default())
        .await
        .unwrap();
    let second = runtime
        .admit("second", None, None, LaunchOverrides::default())
        .await
        .unwrap();

    assert_eq!(second.target.effective_ctx, 4096);
    let record = read(&record);
    let paths: Vec<&String> = record.loads.iter().map(|(p, _)| p).collect();
    assert_eq!(paths, [&files[0], &files[1]]);
    assert_eq!(record.unloads, 1, "the first model was freed");
    assert_eq!(record.loads_over_a_model, 0, "never two models at once");
    let current = runtime.current_model().await.expect("a model is running");
    assert_eq!(current.model_id, 2);
    assert_eq!(
        runtime.state(),
        EngineState::Ready {
            id: 2,
            model: "second".to_owned(),
            n_ctx: 4096
        }
    );
}

/// Two quantizations of one model share its name. Asked for by id, each is
/// the one loaded, and its states say which by that id.
#[tokio::test]
async fn a_model_asked_for_by_id_is_named_by_that_id_beside_a_namesake() {
    let shelf = Shelf::new();
    let q4 = shelf.model(4, "twin", GIB);
    let mut q8 = shelf.model(8, "twin", GIB);
    q8.file_path = shelf.file("twin-Q8_0.gguf", GIB);
    let (runtime, _) = runtime(hello(), vec![q4, q8]).await;

    for id in [8, 4] {
        let admitted = runtime
            .admit(&id.to_string(), None, None, LaunchOverrides::default())
            .await
            .expect("the model is admitted");
        assert_eq!(admitted.target.model_id, id);
        assert_eq!(
            runtime.state(),
            EngineState::Ready {
                id,
                model: "twin".to_owned(),
                n_ctx: 8192
            }
        );
    }
    let sent = runtime.inner.engine.states().sent();
    assert!(
        sent.contains(&EngineState::Unloading {
            id: 8,
            model: "twin".to_owned()
        }),
        "the namesake loaded first was freed by its id: {sent:?}"
    );
}

#[tokio::test]
async fn a_context_asked_for_reloads_the_model_held_at_that_context() {
    let shelf = Shelf::new();
    let (runtime, record) = runtime(hello(), vec![shelf.model(1, "small", GIB)]).await;
    runtime
        .admit("small", None, None, LaunchOverrides::default())
        .await
        .unwrap();
    let smaller = runtime
        .admit("small", Some(2048), None, LaunchOverrides::default())
        .await
        .unwrap();

    assert_eq!(smaller.target.effective_ctx, 2048);
    assert!(smaller.target.just_started);
    let loads: Vec<u32> = read(&record).loads.iter().map(|(_, p)| p.n_ctx).collect();
    assert_eq!(loads, [8192, 2048]);
}

#[tokio::test]
async fn a_model_the_catalog_does_not_hold_and_an_image_model_are_refused() {
    let mut drawer = model(2, "drawer", GIB);
    drawer.image_family = Some(ImageFamily::Sdxl);
    let (runtime, record) = runtime(hello(), vec![drawer]).await;

    let unknown = runtime
        .admit("nobody", None, None, LaunchOverrides::default())
        .await;
    assert!(
        matches!(&unknown, Err(ModelRuntimeError::ModelNotFound(name)) if name == "nobody"),
        "{unknown:?}"
    );
    let image = runtime
        .admit("drawer", None, None, LaunchOverrides::default())
        .await;
    assert!(
        matches!(image, Err(ModelRuntimeError::ImageModelCannotChat(_))),
        "{image:?}"
    );
    assert!(read(&record).loads.is_empty());
}

#[tokio::test]
async fn admissions_of_two_models_at_once_take_turns() {
    let shelf = Shelf::new();
    let models = vec![shelf.model(1, "first", GIB), shelf.model(2, "second", GIB)];
    let script = slow(Duration::ZERO, Duration::from_millis(100));
    let (runtime, record) = runtime(script, models).await;

    let (first, second) = tokio::join!(
        runtime.admit("first", None, None, LaunchOverrides::default()),
        runtime.admit("second", None, None, LaunchOverrides::default()),
    );

    assert!(first.is_ok() && second.is_ok(), "{first:?} {second:?}");
    let record = read(&record);
    assert_eq!(record.loads.len(), 2);
    assert_eq!(record.loads_over_a_model, 0, "never two models at once");
    assert_eq!(record.unloads, 1);
    let current = runtime.current_model().await.expect("a model is running");
    let held = match runtime.state() {
        EngineState::Ready { id, .. } => id,
        other => panic!("one model ready, not {other:?}"),
    };
    assert_eq!(current.model_id, held, "the runtime and the engine agree");
}

#[tokio::test]
async fn a_model_with_a_projector_is_fitted_on_its_weights_as_the_model_browser_fits_it() {
    let shelf = Shelf::new();
    let mut seer = shelf.model(3, "seer", GIB);
    // The catalog's figure counts the projector, which the engine never
    // loads: at 2 GiB it would not fit the budget at all.
    seer.projector = Some(shelf.file("seer-mmproj.gguf", GIB));
    seer.file_size_bytes = 2 * GIB;
    // The model browser's verdict for an installed model.
    let (browser, _) = fit_verdict(
        seer.context_length,
        Some(total_model_bytes(&seer.file_path)),
        seer.kv_elems_per_token,
        Some(BUDGET),
    );
    assert_eq!(browser, FitVerdict::Fits { ctx: 8192 });
    let (runtime, record) = runtime(hello(), vec![seer]).await;

    let admitted = runtime
        .admit("seer", None, None, LaunchOverrides::default())
        .await
        .expect("the weights fit");

    assert_eq!(admitted.target.effective_ctx, 8192);
    assert_eq!(read(&record).loads[0].1.n_ctx, 8192);
}

#[tokio::test]
async fn a_model_trained_under_the_lowest_rung_is_refused_naming_its_trained_context() {
    let shelf = Shelf::new();
    let mut short = shelf.model(1, "short", GIB);
    short.context_length = Some(2048);
    let mut script = hello();
    script.n_ctx_train = 2048;
    let (runtime, record) = runtime(script, vec![short]).await;

    let refused = runtime
        .admit("short", None, None, LaunchOverrides::default())
        .await;

    match refused {
        Err(ModelRuntimeError::SpawnFailed(ref why)) => {
            assert!(why.contains("'short' was trained for 2048 tokens"), "{why}");
            assert!(why.contains("under the 4096"), "{why}");
        }
        ref other => panic!("refused naming its trained context, not {other:?}"),
    }
    assert!(read(&record).loads.is_empty());
    assert_eq!(runtime.state(), EngineState::Idle);
}
