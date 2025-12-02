import { useLocalRuntime, type ChatModelAdapter } from '@assistant-ui/react';
import { TauriService } from '../services/tauri';
import { getApiBase } from '../utils/apiBase';
import { 
  embedThinkingContent, 
  parseStreamingThinkingContent 
} from '../utils/thinkingParser';

export interface ServerInfo {
  model_id: number;
  model_name: string;
  port: number;
  status: string;
}

export interface GglibRuntimeOptions {
  selectedServerPort?: number;
  onError?: (error: Error) => void;
}

// =============================================================================
// Tool Calling Types
// =============================================================================

/** Streaming delta for tool call function details */
interface ToolCallFunctionDelta {
  /** Function name (sent in first chunk) */
  name?: string;
  /** Partial arguments JSON string (accumulated across chunks) */
  arguments?: string;
}

/** Streaming delta for a single tool call */
interface ToolCallDelta {
  /** Index of the tool call (for parallel tool calls) */
  index: number;
  /** Tool call ID (sent in first chunk) */
  id?: string;
  /** Tool type - always "function" (sent in first chunk) */
  type?: string;
  /** Function delta */
  function?: ToolCallFunctionDelta;
}

/** Accumulated tool call (after all deltas combined) */
export interface AccumulatedToolCall {
  /** Unique ID for this tool call */
  id: string;
  /** Tool type - always "function" */
  type: string;
  /** Function call details */
  function: {
    /** Name of the function to call */
    name: string;
    /** JSON string of arguments */
    arguments: string;
  };
}

/** Delta content from SSE stream */
interface StreamDelta {
  /** Main content delta */
  content: string | null;
  /** Reasoning/thinking content delta (from reasoning models) */
  reasoningContent: string | null;
  /** Tool call deltas (for function calling) */
  toolCalls: ToolCallDelta[] | null;
  /** Finish reason from the chunk (null during streaming, set on final chunk) */
  finishReason: string | null;
}

/**
 * Parse SSE events from a streaming response.
 * Handles OpenAI-compatible streaming format with `data:` prefixed lines.
 * Extracts `content`, `reasoning_content`, and `tool_calls` from delta objects.
 */
async function* parseSSEStream(
  reader: ReadableStreamDefaultReader<Uint8Array>,
  abortSignal?: AbortSignal
): AsyncGenerator<StreamDelta, void, unknown> {
  const decoder = new TextDecoder();
  let buffer = '';

  try {
    while (true) {
      if (abortSignal?.aborted) {
        break;
      }

      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });

      // Process complete lines
      const lines = buffer.split('\n');
      buffer = lines.pop() || ''; // Keep incomplete line in buffer

      for (const line of lines) {
        const trimmed = line.trim();
        if (!trimmed) continue;

        // Handle nested SSE (our backend wraps with data:)
        let dataLine = trimmed;
        if (dataLine.startsWith('data:')) {
          dataLine = dataLine.slice(5).trim();
        }

        // Check for stream termination
        if (dataLine === '[DONE]') {
          return;
        }

        // Handle inner data: prefix from llama-server SSE
        if (dataLine.startsWith('data:')) {
          dataLine = dataLine.slice(5).trim();
          if (dataLine === '[DONE]') {
            return;
          }
        }

        // Skip empty data
        if (!dataLine) continue;

        // Parse JSON chunk and extract content deltas
        try {
          const chunk = JSON.parse(dataLine);
          const choice = chunk.choices?.[0];
          const delta = choice?.delta;
          const finishReason = choice?.finish_reason ?? null;
          
          // Extract content, reasoning_content, and tool_calls from delta
          const contentDelta = delta?.content ?? null;
          const reasoningDelta = delta?.reasoning_content ?? null;
          const toolCallsDelta: ToolCallDelta[] | null = delta?.tool_calls ?? null;
          
          // Yield if we have any content or tool calls, or if we have a finish_reason
          if (contentDelta || reasoningDelta || toolCallsDelta || finishReason) {
            yield {
              content: contentDelta,
              reasoningContent: reasoningDelta,
              toolCalls: toolCallsDelta,
              finishReason,
            };
          }
        } catch {
          // Skip non-JSON lines (e.g., comments, keep-alive)
          console.debug('Skipping non-JSON SSE line:', dataLine);
        }
      }
    }
  } finally {
    reader.releaseLock();
  }
}

/**
 * Accumulate streaming tool call deltas by index.
 * Tool calls stream incrementally with partial JSON arguments.
 * This function merges deltas into complete tool calls.
 */
function accumulateToolCallDelta(
  accumulator: Map<number, AccumulatedToolCall>,
  delta: ToolCallDelta
): void {
  const existing = accumulator.get(delta.index);
  
  if (!existing) {
    // First delta for this index - initialize
    accumulator.set(delta.index, {
      id: delta.id ?? '',
      type: delta.type ?? 'function',
      function: {
        name: delta.function?.name ?? '',
        arguments: delta.function?.arguments ?? '',
      },
    });
  } else {
    // Merge with existing
    if (delta.id) existing.id = delta.id;
    if (delta.type) existing.type = delta.type;
    if (delta.function?.name) existing.function.name = delta.function.name;
    if (delta.function?.arguments) {
      existing.function.arguments += delta.function.arguments;
    }
  }
}

/**
 * Get accumulated tool calls as an array, sorted by index.
 */
function getAccumulatedToolCalls(
  accumulator: Map<number, AccumulatedToolCall>
): AccumulatedToolCall[] {
  return Array.from(accumulator.entries())
    .sort(([a], [b]) => a - b)
    .map(([, call]) => call);
}

/**
 * Custom runtime hook that bridges assistant-ui with gglib's served models.
 * 
 * This hook creates a runtime that:
 * - Connects directly to running llama-server instances via /api/chat
 * - Forwards requests to the selected server port
 * - Supports streaming responses for real-time token display
 * - Works in both Tauri desktop and web modes
 *
 * @param options - Configuration options for the runtime
 * @returns The assistant-ui runtime instance
 */
export function useGglibRuntime(options: GglibRuntimeOptions = {}) {
  const { selectedServerPort, onError } = options;

  // Create adapter for gglib direct server API
  const gglibAdapter: ChatModelAdapter = {
    async *run({ messages, abortSignal }) {
      if (!selectedServerPort) {
        const error = new Error('No server selected. Please serve a model first.');
        onError?.(error);
        throw error;
      }

      console.log('🚀 Sending streaming chat request to port:', selectedServerPort);
      console.log('📝 Messages:', messages);

      const requestBody = {
        port: selectedServerPort,
        model: 'default', // llama-server ignores this but we include for consistency
        messages: messages.map((m) => ({
          role: m.role,
          content: m.content
            .filter((c) => c.type === 'text')
            .map((c) => c.text)
            .join('\n'),
        })),
        stream: true, // Enable streaming
      };

      console.log('📤 Request body:', JSON.stringify(requestBody, null, 2));

      const apiBase = await getApiBase();
      const response = await fetch(`${apiBase}/chat`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(requestBody),
        signal: abortSignal,
      });

      console.log('📥 Response status:', response.status);

      if (!response.ok) {
        const errorText = await response.text();
        console.error('❌ API error:', errorText);
        const error = new Error(`API error (${response.status}): ${errorText}`);
        onError?.(error);
        throw error;
      }

      if (!response.body) {
        const error = new Error('Response body is null - streaming not supported');
        onError?.(error);
        throw error;
      }

      // Stream the response
      const reader = response.body.getReader();
      let mainContent = '';
      let thinkingContent = '';
      const thinkingStartTime = Date.now();
      let thinkingEndTime: number | null = null;
      let hasReceivedMainContent = false;
      
      // Timing state for inline <think> tags (when --reasoning-format is not used)
      let inlineThinkingStartTime: number | null = null;
      let inlineThinkingEndTime: number | null = null;
      
      // Tool call accumulation state
      const toolCallAccumulator = new Map<number, AccumulatedToolCall>();
      let finishReason: string | null = null;

      try {
        for await (const delta of parseSSEStream(reader, abortSignal)) {
          // Track finish reason
          if (delta.finishReason) {
            finishReason = delta.finishReason;
          }
          
          // Handle reasoning_content (from llama-server with reasoning format enabled)
          if (delta.reasoningContent) {
            thinkingContent += delta.reasoningContent;
            console.log('💭 Thinking delta:', delta.reasoningContent);
          }
          
          // Handle tool calls (accumulate by index)
          if (delta.toolCalls) {
            for (const tc of delta.toolCalls) {
              accumulateToolCallDelta(toolCallAccumulator, tc);
              console.log('🔧 Tool call delta:', tc);
            }
          }
          
          // Handle main content
          if (delta.content) {
            // If this is our first main content and we had thinking, record thinking end time
            if (!hasReceivedMainContent && thinkingContent) {
              thinkingEndTime = Date.now();
              hasReceivedMainContent = true;
            }
            
            mainContent += delta.content;
            console.log('💬 Content delta:', delta.content);
          }
          
          // Build display content with duration embedded in every yield
          let displayContent = '';
          
          if (thinkingContent) {
            // Calculate current thinking duration for live display
            const currentEndTime = thinkingEndTime ?? Date.now();
            const currentDurationSeconds = (currentEndTime - thinkingStartTime) / 1000;
            // Embed with current duration (updates live, final value on completion)
            displayContent = embedThinkingContent(thinkingContent, mainContent, currentDurationSeconds);
          } else {
            // Check if main content contains inline <think> tags (fallback for --reasoning-format none)
            const parsed = parseStreamingThinkingContent(mainContent);
            if (parsed.thinking) {
              // Start tracking time when we first detect inline thinking
              if (inlineThinkingStartTime === null) {
                inlineThinkingStartTime = Date.now();
              }
              
              // Track end time when thinking completes (closing tag received)
              if (parsed.isThinkingComplete && inlineThinkingEndTime === null) {
                inlineThinkingEndTime = Date.now();
              }
              
              // Calculate current duration for live display
              const currentEndTime = inlineThinkingEndTime ?? Date.now();
              const currentDuration = (currentEndTime - inlineThinkingStartTime) / 1000;
              
              // Re-embed thinking with duration metadata
              displayContent = embedThinkingContent(parsed.thinking, parsed.content, currentDuration);
            } else {
              displayContent = mainContent;
            }
          }
          
          // Yield partial content to update the UI progressively
          yield {
            content: [{ type: 'text' as const, text: displayContent }],
          };
        }
      } catch (error) {
        // On error, discard partial content (as discussed)
        console.error('❌ Stream error:', error);
        onError?.(error instanceof Error ? error : new Error(String(error)));
        throw error;
      }

      // Calculate thinking duration if we had thinking content
      let thinkingDurationSeconds: number | null = null;
      if (thinkingContent) {
        const endTime = thinkingEndTime ?? Date.now();
        thinkingDurationSeconds = (endTime - thinkingStartTime) / 1000;
      }

      // Build final content with embedded thinking tags for persistence
      let finalContent = '';
      if (thinkingContent) {
        finalContent = embedThinkingContent(thinkingContent, mainContent, thinkingDurationSeconds);
      } else if (inlineThinkingStartTime !== null) {
        // Inline <think> tags path - embed duration for persistence
        const parsed = parseStreamingThinkingContent(mainContent);
        if (parsed.thinking) {
          const endTime = inlineThinkingEndTime ?? Date.now();
          const inlineDurationSeconds = (endTime - inlineThinkingStartTime) / 1000;
          finalContent = embedThinkingContent(parsed.thinking, parsed.content, inlineDurationSeconds);
        } else {
          finalContent = mainContent;
        }
      } else {
        finalContent = mainContent;
      }

      // Get accumulated tool calls
      const accumulatedToolCalls = getAccumulatedToolCalls(toolCallAccumulator);
      
      console.log('✅ Streaming complete');
      console.log('   Content:', finalContent);
      console.log('   Tool calls:', accumulatedToolCalls.length > 0 ? accumulatedToolCalls : 'none');
      console.log('   Finish reason:', finishReason);

      // Build final content parts
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const contentParts: any[] = [];
      
      // Add text content if present
      if (finalContent) {
        contentParts.push({ type: 'text' as const, text: finalContent });
      }
      
      // Add tool calls if present (finish_reason: "tool_calls")
      if (finishReason === 'tool_calls' && accumulatedToolCalls.length > 0) {
        for (const tc of accumulatedToolCalls) {
          contentParts.push({
            type: 'tool-call' as const,
            toolCallId: tc.id,
            toolName: tc.function.name,
            args: JSON.parse(tc.function.arguments || '{}'),
          });
        }
      }

      // Final yield with complete content
      yield {
        content: contentParts.length > 0 
          ? contentParts 
          : [{ type: 'text' as const, text: '' }],
      };
    },
  };

  const runtime = useLocalRuntime(gglibAdapter);
  return runtime;
}

/**
 * Fetch available servers (served models) from gglib
 * 
 * @returns Promise resolving to array of server info objects
 */
export async function fetchAvailableServers(): Promise<ServerInfo[]> {
  try {
    return await TauriService.listServers();
  } catch (error) {
    console.error('Error fetching servers:', error);
    return [];
  }
}
