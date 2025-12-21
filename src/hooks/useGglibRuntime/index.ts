/**
 * useGglibRuntime - Chat adapter for assistant-ui with SSE streaming and tool calling.
 *
 * This module provides:
 * - `useGglibRuntime` - Main hook for creating the chat runtime
 * - `fetchAvailableServers` - Utility to list available llama-server instances
 * - `parseSSEStream` - SSE stream parser (exported for testing)
 * - `createToolCallAccumulator` - Tool call accumulator factory (exported for testing)
 * - `createThinkingContentHandler` - Thinking content handler factory (exported for testing)
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

// SSE parsing (exported for testing and potential reuse)
export {
  parseSSEStream,
  type StreamDelta,
  type ToolCallDelta,
  type ToolCallFunctionDelta,
} from './parseSSEStream';

// Tool call accumulation (exported for testing)
export {
  createToolCallAccumulator,
  type AccumulatedToolCall,
  type ToolCallAccumulator,
  type ToolCallAccumulatorState,
} from './accumulateToolCalls';

// Thinking content handling (exported for testing)
export {
  createThinkingContentHandler,
  type ThinkingContentHandler,
} from './thinkingContentHandler';

// Agent loop utilities (exported for testing and configuration)
export {
  DEFAULT_MAX_TOOL_ITERS,
  TOOL_ENABLED_SYSTEM_PROMPT,
  DEFAULT_SYSTEM_PROMPT,
  FORMAT_REMINDER,
  getSystemPrompt,
  type AgentLoopState,
  type ToolDigest,
  type FinalEnvelope,
  type ChatMessage,
  tryParseFinalEnvelope,
  toolSignature,
  withRetry,
  recordAssistantProgress,
  checkToolLoop,
  buildWorkingMemory,
  upsertWorkingMemory,
  pruneForBudget,
  summarizeToolResult,
} from './agentLoop';
