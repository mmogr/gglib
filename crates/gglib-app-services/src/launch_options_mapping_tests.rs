//! A start request becomes launch options in one place, and these tests hold
//! the two callers to it: a pinned launch and a bare start.

use gglib_runtime::build_server_config;

use super::launch_options_tests::model;
use super::*;

/// `base` once for each of `states`, with `set` applied.
fn with_each<T: Clone>(
    base: &[StartServerRequest],
    states: &[T],
    set: impl Fn(&mut StartServerRequest, T),
) -> Vec<StartServerRequest> {
    let one = |request: &StartServerRequest, state: &T| {
        let mut request = request.clone();
        set(&mut request, state.clone());
        request
    };
    base.iter()
        .flat_map(|request| states.iter().map(move |state| (request, state)))
        .map(|(request, state)| one(request, state))
        .collect()
}

/// Every request of the table: each optional field set and unset, in every
/// combination, with MTP's draft count in its three states (unsaid, off, a
/// count) and jinja in its three.
fn requests() -> Vec<StartServerRequest> {
    let sampling = InferenceConfig {
        temperature: Some(0.4),
        ..Default::default()
    };
    let all = vec![StartServerRequest::default()];
    let all = with_each(&all, &[None, Some(8192)], |r, v| r.context_length = v);
    let all = with_each(&all, &[None, Some(9345)], |r, v| r.port = v);
    let all = with_each(&all, &[None, Some(true), Some(false)], |r, v| r.jinja = v);
    let all = with_each(&all, &[None, Some("none".to_owned())], |r, v| {
        r.reasoning_format = v;
    });
    let all = with_each(&all, &[None, Some(0), Some(4)], |r, v| {
        r.mtp_draft_n_max = v;
    });
    let all = with_each(&all, &[None, Some(0.6)], |r, v| r.mtp_draft_p_min = v);
    let all = with_each(&all, &[None, Some(sampling)], |r, v| {
        r.inference_params = v;
    });
    with_each(&all, &[false, true], |r, v| r.mlock = v)
}

/// A model with the `mtp` tag or without it, and with a context of its own or
/// without one.
fn a_model(mtp_tag: bool, own_context: Option<usize>) -> Model {
    Model {
        tags: if mtp_tag {
            vec!["mtp".to_owned()]
        } else {
            vec![]
        },
        server_defaults: own_context.map(|context_length| gglib_core::domain::ServerConfig {
            context_length: Some(context_length),
        }),
        ..model()
    }
}

/// The MTP draft count `model` launches with under `options`: the options
/// laid over a template that says nothing, as admission lays a pin's and a
/// start's, then built with the model's own tags, as the spawn builds them.
fn launched_draft_count(model: &Model, options: &ServerConfigOptions) -> Option<u32> {
    build_server_config(
        model.id,
        model.name.clone(),
        model.file_path.clone(),
        0,
        &model.tags,
        ServerConfigOptions::default().overlay(options),
    )
    .spec_draft_n_max
}

/// Each field of a request reaches its option as it was sent, a pin's options
/// and a bare start's are the same options, and no option a request cannot
/// name is set beside them.
///
/// The destructuring is exhaustive on purpose: an option added to
/// `ServerConfigOptions` fails to compile here until it is given a row.
#[test]
fn a_request_reaches_its_options_as_sent_on_a_pin_and_on_a_bare_start() {
    let settings = Settings {
        default_context_size: Some(16_384),
        ..Default::default()
    };
    let models = [
        a_model(false, None),
        a_model(true, None),
        a_model(false, Some(32_768)),
        a_model(true, Some(32_768)),
    ];
    let requests = requests();
    assert_eq!(requests.len(), 576, "the table lost a field's states");

    for (model, request) in models
        .iter()
        .flat_map(|m| requests.iter().map(move |r| (m, r)))
    {
        let pin = plan_pinned_launch(model, &settings, request, ProxyGlobals::default());
        let (_, bare) = plan_bare_launch(model, &settings, request);
        assert_eq!(
            serde_json::to_value(&bare.options).expect("options serialise"),
            serde_json::to_value(&pin.pinned.launch_overrides).expect("options serialise"),
            "a bare start and a pin disagree on {request:?}"
        );

        let ServerConfigOptions {
            context_size,
            model_server_ctx,
            global_default_ctx,
            fitted_ctx,
            port,
            jinja,
            reasoning_format,
            mtp_draft_n_max,
            mtp_draft_p_min,
            slot_save_path,
            cache_ram_mb,
            cache_reuse,
            cache_type_k,
            cache_type_v,
            inference_params,
            mlock,
        } = pin.pinned.launch_overrides.clone();

        assert_eq!(context_size, request.context_length, "{request:?}");
        assert_eq!(port, request.port, "{request:?}");
        assert_eq!(jinja, request.jinja, "{request:?}");
        assert_eq!(reasoning_format, request.reasoning_format, "{request:?}");
        assert_eq!(mtp_draft_n_max, request.mtp_draft_n_max, "{request:?}");
        assert_eq!(mtp_draft_p_min, request.mtp_draft_p_min, "{request:?}");
        assert_eq!(mlock, request.mlock.then_some(true), "{request:?}");
        assert_eq!(inference_params.as_ref(), Some(&pin.inference));

        let own_context = model
            .server_defaults
            .as_ref()
            .and_then(|s| s.context_length);
        assert_eq!(model_server_ctx, own_context);
        assert_eq!(global_default_ctx, settings.default_context_size);
        assert_eq!(fitted_ctx, None);
        assert_eq!(slot_save_path, None);
        assert_eq!((cache_ram_mb, cache_reuse), (None, None));
        assert_eq!((cache_type_k, cache_type_v), (None, None));

        // What the spawn makes of those two MTP values and the model's tags is
        // what the plan tells the banner: the request where it speaks, the
        // tag only where it does not.
        assert_eq!(
            resolve_mtp_args(mtp_draft_n_max, mtp_draft_p_min, &model.tags),
            pin.mtp,
            "{request:?}"
        );
    }
}

/// An explicit "off" outranks the model's `mtp` tag: neither a pin nor a bare
/// start launches with a draft count, and the plan tells the banner so. The
/// same model asked nothing launches with the tag's two.
#[test]
fn an_explicit_mtp_off_launches_a_tagged_model_without_mtp_on_both_paths() {
    let model = a_model(true, None);
    // The draft count of a pin's launch, then of a bare start's, and whether
    // the plan tells the banner MTP is on.
    let launches = |request: &StartServerRequest| {
        let settings = Settings::default();
        let pin = plan_pinned_launch(&model, &settings, request, ProxyGlobals::default());
        let (_, bare) = plan_bare_launch(&model, &settings, request);
        let pinned = launched_draft_count(&model, &pin.pinned.launch_overrides);
        let bare = launched_draft_count(&model, &bare.options);
        ([pinned, bare], pin.mtp.enabled)
    };

    let off = StartServerRequest {
        mtp_draft_n_max: Some(0),
        ..Default::default()
    };
    assert_eq!(launches(&off), ([None, None], false));
    assert_eq!(
        launches(&StartServerRequest::default()),
        ([Some(2), Some(2)], true)
    );
}

/// A bare start hands admission the context somebody chose: the request's,
/// then the model's own, then the stored default, and none at all when nobody
/// chose one, so the fit decides. A pin's proxy falls back to the same answer.
#[test]
fn a_bare_start_falls_back_to_the_context_somebody_chose() {
    for (requested, own, stored, want) in [
        (Some(4096), Some(32_768), Some(16_384), Some(4096)),
        (None, Some(32_768), Some(16_384), Some(32_768)),
        (None, None, Some(16_384), Some(16_384)),
        (None, None, None, None),
    ] {
        let model = a_model(false, own);
        let settings = Settings {
            default_context_size: stored,
            ..Default::default()
        };
        let request = StartServerRequest {
            context_length: requested,
            ..Default::default()
        };

        let (default_ctx, _) = plan_bare_launch(&model, &settings, &request);
        let pin = plan_pinned_launch(&model, &settings, &request, ProxyGlobals::default());

        assert_eq!(default_ctx, want);
        assert_eq!(pin.unified.to_proxy_config().default_context, want);
    }
}
