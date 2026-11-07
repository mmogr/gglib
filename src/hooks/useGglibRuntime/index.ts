/**
 * useGglibRuntime - Chat adapter for assistant-ui with backend agentic loop.
 *
 * This module provides:
 * - `useGglibRuntime` - Main hook for creating the chat runtime
 *
 * @module useGglibRuntime
 */

// Main hook and utilities
export {
  useGglibRuntime,
  type UseGglibRuntimeOptions,
  type UseGglibRuntimeReturn,
} from './useGglibRuntime';

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
