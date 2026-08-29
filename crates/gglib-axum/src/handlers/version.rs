//! Build-identity endpoint.

use axum::Json;

use crate::dto::version::VersionDto;

/// `GET /api/version` — which build of gglib is answering.
///
/// `/health` already carries the version and fingerprint, but it is an
/// untyped liveness probe that also reports debug-switch state, and it sits
/// outside `/api` so orchestration probes can reach it without credentials.
/// This is the typed contract the dashboard reads, with a generated
/// TypeScript binding behind it.
pub(crate) async fn get_version() -> Json<VersionDto> {
    Json(VersionDto::current())
}
