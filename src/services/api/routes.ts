/**
 * API route constants.
 *
 * Centralized route definitions to ensure consistency between
 * HTTP transport and backend. These mirror the Rust contracts
 * in gglib-core::contracts::http.
 */

// Daemon build identity (mirrors gglib_core::contracts::http::daemon::VERSION_PATH)
export const VERSION_PATH = '/api/version';

// Hugging Face routes (nested under /api/models/hf)
export const HF_SEARCH_PATH = '/api/models/hf/search';
export const HF_MODEL_PATH = '/api/models/hf/model';
export const HF_QUANTIZATIONS_PATH = '/api/models/hf/quantizations';
export const HF_TOOL_SUPPORT_PATH = '/api/models/hf/tool-support';

// Remote tunnel (mirrors gglib_core::contracts::http::daemon::REMOTE_*_PATH)
export const REMOTE_ENABLE_PATH = '/api/remote/enable';
export const REMOTE_DISABLE_PATH = '/api/remote/disable';
export const REMOTE_STATUS_PATH = '/api/remote/status';
export const REMOTE_JOIN_PATH = '/api/remote/join';
export const REMOTE_DISCONNECT_PATH = '/api/remote/disconnect';
export const REMOTE_KILL_PATH = '/api/remote/kill';
export const REMOTE_INVITE_PATH = '/api/remote/invite';
export const REMOTE_DEVICES_PATH = '/api/remote/devices';

// The far machine's chats and runs, forwarded through the tunnel for the chat
// page (mirrors REMOTE_CHATS_PATH and REMOTE_RUNS_PATH and the path functions
// beside them in gglib_core::contracts::http::daemon)
export const REMOTE_CHATS_PATH = '/api/remote/chats';
export const REMOTE_RUNS_PATH = '/api/remote/runs';

// The paired machine's models, read through the tunnel for the library
// (mirrors REMOTE_MODELS_PATH and the path functions beside it in
// gglib_core::contracts::http::daemon)
export const REMOTE_MODELS_PATH = '/api/remote/models';

// The images a message carries, by the id an upload answers: this machine's
// store, and the paired machine's through the tunnel for a far chat. `POST`
// takes the image as the raw body; `GET` with `/{id}` answers its bytes
// (mirrors ATTACHMENTS_PATH and REMOTE_ATTACHMENTS_PATH in
// gglib_core::contracts::http::attachments)
export const ATTACHMENTS_PATH = '/api/attachments';
export const REMOTE_ATTACHMENTS_PATH = '/api/remote/attachments';
