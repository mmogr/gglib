/**
 * API route constants.
 *
 * Centralized route definitions to ensure consistency between
 * HTTP transport and backend. These mirror the Rust contracts
 * in gglib-core::contracts::http.
 */

// Hugging Face routes (nested under /api/models/hf)
export const HF_SEARCH_PATH = '/api/models/hf/search';
export const HF_MODEL_PATH = '/api/models/hf/model';
export const HF_QUANTIZATIONS_PATH = '/api/models/hf/quantizations';
export const HF_TOOL_SUPPORT_PATH = '/api/models/hf/tool-support';
