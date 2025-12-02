import { useLocalRuntime, type ChatModelAdapter } from '@assistant-ui/react';
import { TauriService } from '../services/tauri';
import { getApiBase } from '../utils/apiBase';
import { 
  embedThinkingContent, 
  parseStreamingThinkingContent 
} from '../utils/thinkingParser';
import { getToolRegistry, type ToolDefinition } from '../services/tools';

export interface ServerInfo {
  model_id: number;
  model_name: string;
  port: number;
  status: string;
}

export interface GglibRuntimeOptions {
  selectedServerPort?: number;
  onError?: (error: Error) => void;
  /** Maximum iterations for tool calling agentic loop (default: 10) */
  maxToolIterations?: number;
  /** Whether to enable tool calling (default: true if tools are registered) */
  enableToolCalling?: boolean;
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
 * - Supports tool calling with automatic tool execution loop
 * - Works in both Tauri desktop and web modes
 *
 * @param options - Configuration options for the runtime
 * @returns The assistant-ui runtime instance
 */
export function useGglibRuntime(options: GglibRuntimeOptions = {}) {
  const { 
    selectedServerPort, 
    onError,
    maxToolIterations = 10,
    enableToolCalling = true,
  } = options;

  // Get tool definitions if tool calling is enabled
  const getToolDefinitions = (): ToolDefinition[] => {
    if (!enableToolCalling) return [];
    const registry = getToolRegistry();
    return registry.getEnabledDefinitions();
  };

  // Create adapter for gglib direct server API
  const gglibAdapter: ChatModelAdapter = {
    async *run({ messages, abortSignal }) {
      if (!selectedServerPort) {
        const error = new Error('No server selected. Please serve a model first.');
        onError?.(error);
        throw error;
      }

      // Convert assistant-ui messages to API format
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const convertMessages = (msgs: typeof messages): any[] => {
        return msgs.map((m) => {
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          const msg = m as any;
          
          // Handle tool messages (from agentic loop)
          if (msg.role === 'tool') {
            return {
              role: 'tool',
              tool_call_id: msg.toolCallId,
              content: msg.content
                ?.filter((c: { type: string }) => c.type === 'tool-result')
                ?.map((c: { result?: unknown }) => JSON.stringify(c.result))
                ?.join('\n') || '',
            };
          }
          
          // Handle assistant messages with tool calls
          const toolCalls = msg.content
            ?.filter((c: { type: string }) => c.type === 'tool-call')
            ?.map((c: { toolCallId: string; toolName: string; args?: unknown }) => ({
              id: c.toolCallId,
              type: 'function',
              function: {
                name: c.toolName,
                arguments: JSON.stringify(c.args || {}),
              },
            })) || [];
          
          const textContent = msg.content
            ?.filter((c: { type: string }) => c.type === 'text')
            ?.map((c: { text: string }) => c.text)
            ?.join('\n') || '';
          
          if (toolCalls.length > 0) {
            return {
              role: msg.role,
              content: textContent || null,
              tool_calls: toolCalls,
            };
          }
          
          return {
            role: msg.role,
            content: textContent,
          };
        });
      };

      // Track conversation state for agentic loop
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      let conversationMessages: any[] = convertMessages(messages);
      let iteration = 0;
      const toolDefinitions = getToolDefinitions();
      const hasTools = toolDefinitions.length > 0;
      
      console.log('🚀 Starting chat request to port:', selectedServerPort);
      console.log('📝 Initial messages:', messages.length);
      console.log('🔧 Tools available:', hasTools ? toolDefinitions.map(t => t.function.name) : 'none');

      // Agentic loop: continue until we get a non-tool-call response or hit max iterations
      while (iteration < maxToolIterations) {
        iteration++;
        console.log(`\n📍 Iteration ${iteration}/${maxToolIterations}`);

        const requestBody = {
          port: selectedServerPort,
          model: 'default',
          messages: conversationMessages,
          stream: true,
          ...(hasTools && { tools: toolDefinitions }),
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

        // Stream the response for this iteration
        const reader = response.body.getReader();
        let mainContent = '';
        let thinkingContent = '';
        const thinkingStartTime = Date.now();
        let thinkingEndTime: number | null = null;
        let hasReceivedMainContent = false;
        
        // Timing state for inline <think> tags
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
            
            // Handle reasoning_content
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
              if (!hasReceivedMainContent && thinkingContent) {
                thinkingEndTime = Date.now();
                hasReceivedMainContent = true;
              }
              
              mainContent += delta.content;
              console.log('💬 Content delta:', delta.content);
            }
            
            // Build display content with duration embedded
            let displayContent = '';
            
            if (thinkingContent) {
              const currentEndTime = thinkingEndTime ?? Date.now();
              const currentDurationSeconds = (currentEndTime - thinkingStartTime) / 1000;
              displayContent = embedThinkingContent(thinkingContent, mainContent, currentDurationSeconds);
            } else {
              const parsed = parseStreamingThinkingContent(mainContent);
              if (parsed.thinking) {
                if (inlineThinkingStartTime === null) {
                  inlineThinkingStartTime = Date.now();
                }
                if (parsed.isThinkingComplete && inlineThinkingEndTime === null) {
                  inlineThinkingEndTime = Date.now();
                }
                const currentEndTime = inlineThinkingEndTime ?? Date.now();
                const currentDuration = (currentEndTime - inlineThinkingStartTime) / 1000;
                displayContent = embedThinkingContent(parsed.thinking, parsed.content, currentDuration);
              } else {
                displayContent = mainContent;
              }
            }
            
            // Yield partial content to update UI
            yield {
              content: [{ type: 'text' as const, text: displayContent }],
            };
          }
        } catch (error) {
          console.error('❌ Stream error:', error);
          onError?.(error instanceof Error ? error : new Error(String(error)));
          throw error;
        }

        // Calculate thinking duration
        let thinkingDurationSeconds: number | null = null;
        if (thinkingContent) {
          const endTime = thinkingEndTime ?? Date.now();
          thinkingDurationSeconds = (endTime - thinkingStartTime) / 1000;
        }

        // Build final content
        let finalContent = '';
        if (thinkingContent) {
          finalContent = embedThinkingContent(thinkingContent, mainContent, thinkingDurationSeconds);
        } else if (inlineThinkingStartTime !== null) {
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
        
        console.log('✅ Iteration complete');
        console.log('   Content:', finalContent.substring(0, 100) + (finalContent.length > 100 ? '...' : ''));
        console.log('   Tool calls:', accumulatedToolCalls.length > 0 ? accumulatedToolCalls.map(tc => tc.function.name) : 'none');
        console.log('   Finish reason:', finishReason);

        // If no tool calls or not a tool_calls finish reason, we're done
        if (finishReason !== 'tool_calls' || accumulatedToolCalls.length === 0) {
          // Final yield with complete content
          yield {
            content: finalContent 
              ? [{ type: 'text' as const, text: finalContent }]
              : [{ type: 'text' as const, text: '' }],
          };
          return;
        }

        // === Tool Calling: Execute tools and continue loop ===
        console.log('🔧 Executing tool calls...');
        
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const contentParts: any[] = [];
        
        // Add any text content from this iteration
        if (finalContent) {
          contentParts.push({ type: 'text' as const, text: finalContent });
        }
        
        // Process each tool call
        const toolResultMessages: { role: string; tool_call_id: string; content: string }[] = [];
        
        for (const tc of accumulatedToolCalls) {
          const toolCallPart = {
            type: 'tool-call' as const,
            toolCallId: tc.id,
            toolName: tc.function.name,
            args: JSON.parse(tc.function.arguments || '{}'),
          };
          contentParts.push(toolCallPart);
          
          // Yield tool call to show in UI
          yield {
            content: contentParts.slice(),
          };
          
          // Execute the tool
          console.log(`🔧 Executing tool: ${tc.function.name}`);
          const registry = getToolRegistry();
          const result = await registry.executeRawCall({
            id: tc.id,
            type: 'function',
            function: tc.function,
          });
          
          console.log(`   Result:`, result);
          
          // Add tool result part
          const toolResultPart = {
            type: 'tool-result' as const,
            toolCallId: tc.id,
            result: result.success ? result.data : { error: result.error },
          };
          contentParts.push(toolResultPart);
          
          // Yield with tool result
          yield {
            content: contentParts.slice(),
          };
          
          // Add to messages for next iteration
          toolResultMessages.push({
            role: 'tool',
            tool_call_id: tc.id,
            content: JSON.stringify(result.success ? result.data : { error: result.error }),
          });
        }
        
        // Add assistant message with tool calls to conversation
        conversationMessages.push({
          role: 'assistant',
          content: finalContent || null,
          tool_calls: accumulatedToolCalls.map(tc => ({
            id: tc.id,
            type: 'function',
            function: {
              name: tc.function.name,
              arguments: tc.function.arguments,
            },
          })),
        });
        
        // Add tool result messages
        conversationMessages.push(...toolResultMessages);
        
        console.log('🔄 Continuing agentic loop with tool results...');
      }
      
      // Max iterations reached
      console.warn(`⚠️ Max tool iterations (${maxToolIterations}) reached`);
      yield {
        content: [{ 
          type: 'text' as const, 
          text: `[Maximum tool calling iterations (${maxToolIterations}) reached. The conversation was truncated.]` 
        }],
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
