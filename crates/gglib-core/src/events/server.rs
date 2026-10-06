//! Model server lifecycle events.

use crate::ports::model_runtime::RuntimeErrorEnvelope;

use super::AppEvent;

impl AppEvent {
    /// Create a server started event.
    pub fn server_started(model_id: i64, model_name: impl Into<String>, port: u16) -> Self {
        Self::ServerStarted {
            model_id,
            model_name: model_name.into(),
            port,
        }
    }

    /// Create a server stopped event.
    pub fn server_stopped(model_id: i64, model_name: impl Into<String>) -> Self {
        Self::ServerStopped {
            model_id,
            model_name: model_name.into(),
        }
    }

    /// Create a server error event.
    pub fn server_error(
        model_id: Option<i64>,
        model_name: impl Into<String>,
        error: RuntimeErrorEnvelope,
    ) -> Self {
        Self::ServerError {
            model_id,
            model_name: model_name.into(),
            error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::model_runtime::ModelRuntimeError;

    /// The frame `/api/events` carries for `event`, as the bytes it is sent
    /// as: key order included, since that is what a client is handed.
    fn wire(event: &AppEvent) -> String {
        serde_json::to_string(event).unwrap()
    }

    #[test]
    fn a_started_server_is_sent_with_its_model_and_port() {
        assert_eq!(
            wire(&AppEvent::server_started(42, "test-model", 9001)),
            r#"{"type":"server_started","modelId":42,"modelName":"test-model","port":9001}"#
        );
    }

    #[test]
    fn a_stopped_server_is_sent_with_its_model_and_no_port() {
        assert_eq!(
            wire(&AppEvent::server_stopped(42, "test-model")),
            r#"{"type":"server_stopped","modelId":42,"modelName":"test-model"}"#
        );
    }

    #[test]
    fn a_server_error_is_sent_with_the_envelope_of_the_runtime_error() {
        let failed = ModelRuntimeError::SpawnFailed("no binary".to_owned());
        assert_eq!(
            wire(&AppEvent::server_error(
                Some(42),
                "test-model",
                (&failed).into()
            )),
            r#"{"type":"server_error","modelId":42,"modelName":"test-model","error":{"message":"Failed to start model: no binary","type":"server_error","retryable":false}}"#
        );

        let loading = ModelRuntimeError::ModelLoading;
        assert_eq!(
            wire(&AppEvent::server_error(
                Some(42),
                "test-model",
                (&loading).into()
            )),
            r#"{"type":"server_error","modelId":42,"modelName":"test-model","error":{"message":"Model is loading, try again","type":"service_unavailable","retryable":true}}"#
        );
    }

    /// The key is always there: a model that is not known is `null`, and a
    /// client reading the field never finds it missing.
    #[test]
    fn a_server_error_for_no_known_model_sends_a_null_model_id() {
        let failed = ModelRuntimeError::Internal("x".to_owned());
        assert_eq!(
            wire(&AppEvent::server_error(None, "n", (&failed).into())),
            r#"{"type":"server_error","modelId":null,"modelName":"n","error":{"message":"Internal error: x","type":"server_error","retryable":false}}"#
        );
    }
}
