/**
 * Transport layer mappers.
 * 
 * These functions map between frontend types and backend request DTOs,
 * serving as the single source of truth for request shape construction.
 */

import type { ServeConfig } from '../transport/types/models';
import type { CreateConversationParams, SaveMessageParams } from '../transport/types/chat';

/**
 * Request shape matching Rust's StartServerRequest.
 * Must stay in sync with gglib-gui/src/types.rs::StartServerRequest
 */
export interface StartServerRequest {
  contextLength?: number;
  port?: number;
  mlock: boolean;
  jinja?: boolean;
  reasoningFormat?: string;
  // Inference parameters as nested object (matches Rust's inference_params field)
  inferenceParams?: {
    temperature?: number;
    topP?: number;
    topK?: number;
    maxTokens?: number;
    repeatPenalty?: number;
  };
}

/**
 * Request shape matching Rust's CreateConversationRequest.
 * Must stay in sync with gglib-axum/src/handlers/chat.rs
 */
export interface CreateConversationRequest {
  title?: string | null;
  model_id?: number | null;
  system_prompt?: string | null;
}

/**
 * Request shape matching Rust's SaveMessageRequest.
 * Must stay in sync with gglib-axum/src/handlers/chat.rs
 */
export interface SaveMessageRequest {
  conversation_id: number;
  role: string;
  content: string;
}

/**
 * Request shape matching Rust's UpdateConversationRequest.
 * Must stay in sync with gglib-axum/src/handlers/chat.rs
 */
export interface UpdateConversationRequest {
  title?: string | null;
  system_prompt?: string | null;
}

/**
 * Convert ServeConfig to StartServerRequest for Tauri IPC.
 * 
 * Extracts only the fields needed by StartServerRequest, omitting `id` 
 * (which is passed separately) and legacy `ctx_size` (replaced by context_length).
 * 
 * @param config - Frontend serve configuration
 * @returns StartServerRequest matching Rust type
 */
export function toStartServerRequest(config: ServeConfig): StartServerRequest {
  // Build inference params object only if any values are set
  const hasInferenceParams = config.temperature !== undefined || 
    config.topP !== undefined || 
    config.topK !== undefined || 
    config.maxTokens !== undefined || 
    config.repeatPenalty !== undefined;
  
  const inferenceParams = hasInferenceParams ? {
    temperature: config.temperature,
    topP: config.topP,
    topK: config.topK,
    maxTokens: config.maxTokens,
    repeatPenalty: config.repeatPenalty,
  } : undefined;

  return {
    contextLength: config.contextLength,
    port: config.port,
    mlock: config.mlock ?? false,
    jinja: config.jinja,
    // reasoning_format is auto-detected from model tags on backend
    reasoningFormat: undefined,
    inferenceParams,
  };
}

/**
 * Convert CreateConversationParams to CreateConversationRequest for Tauri IPC.
 */
export function toCreateConversationRequest(params: CreateConversationParams): CreateConversationRequest {
  return {
    title: params.title ?? null,
    model_id: params.modelId ?? null,
    system_prompt: params.systemPrompt ?? null,
  };
}

/**
 * Convert SaveMessageParams to SaveMessageRequest for Tauri IPC.
 */
export function toSaveMessageRequest(params: SaveMessageParams): SaveMessageRequest {
  return {
    conversation_id: params.conversationId,
    role: params.role,
    content: params.content,
  };
}

/**
 * Convert update params to UpdateConversationRequest for Tauri IPC.
 */
export function toUpdateConversationRequest(
  title?: string,
  systemPrompt?: string | null
): UpdateConversationRequest {
  return {
    title: title ?? null,
    system_prompt: systemPrompt,
  };
}
