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
export const REMOTE_CONNECT_PATH = '/api/remote/connect';
export const REMOTE_DISCONNECT_PATH = '/api/remote/disconnect';
export const REMOTE_KILL_PATH = '/api/remote/kill';
