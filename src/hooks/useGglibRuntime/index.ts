/**
 * useGglibRuntime - Chat adapter for assistant-ui with backend agentic loop.
 *
 * This module provides:
 * - `useGglibRuntime` - Main hook for creating the chat runtime
 * - `streamAgentChat` - Backend SSE consumer for /api/agent/chat
 *
 * @module useGglibRuntime
 */

// Main hook and utilities
export {
  useGglibRuntime,
  type UseGglibRuntimeOptions,
  type UseGglibRuntimeReturn,
} from './useGglibRuntime';

// Backend SSE consumer
export {
  streamAgentChat,
  type StreamAgentChatOptions,
  type PartialAgentConfig,
} from './streamAgentChat';

// Message types (re-exported from types/messages)
export type {
  GglibMessage,
  MessageContent,
  MessagePart,
  ToolCallPart,
  TextPart,
  ReasoningPart,
  GglibContent,
} from '../../types/messages';

// UI / conversation defaults
export { DEFAULT_SYSTEM_PROMPT } from '../../constants/prompts';
