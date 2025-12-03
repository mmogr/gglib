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
 * @module useGglibRuntime
 */

import { useLocalRuntime, type ChatModelAdapter } from '@assistant-ui/react';
import { listServers } from '../../services/tauri';
import { getApiBase } from '../../utils/apiBase';
import { getToolRegistry, type ToolDefinition } from '../../services/tools';

import { parseSSEStream } from './parseSSEStream';
import {
  createToolCallAccumulator,
  type AccumulatedToolCall,
} from './accumulateToolCalls';
import { createThinkingContentHandler } from './thinkingContentHandler';

// =============================================================================
// Types
// =============================================================================

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
// Message Conversion
// =============================================================================

/**
 * Convert assistant-ui messages to API format.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function convertMessages(msgs: any[]): any[] {
  return msgs.map((msg) => {
    // Handle tool messages (from agentic loop)
    if (msg.role === 'tool') {
      return {
        role: 'tool',
        tool_call_id: msg.toolCallId,
        content:
          msg.content
            ?.filter((c: { type: string }) => c.type === 'tool-result')
            ?.map((c: { result?: unknown }) => JSON.stringify(c.result))
            ?.join('\n') || '',
      };
    }

    // Handle assistant messages with tool calls
    const toolCalls =
      msg.content
        ?.filter((c: { type: string }) => c.type === 'tool-call')
        ?.map(
          (c: { toolCallId: string; toolName: string; args?: unknown }) => ({
            id: c.toolCallId,
            type: 'function',
            function: {
              name: c.toolName,
              arguments: JSON.stringify(c.args || {}),
            },
          })
        ) || [];

    const textContent =
      msg.content
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
}

// =============================================================================
// Hook
// =============================================================================

/**
 * Custom runtime hook that bridges assistant-ui with gglib's served models.
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
        const error = new Error(
          'No server selected. Please serve a model first.'
        );
        onError?.(error);
        throw error;
      }

      // Track conversation state for agentic loop
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      let conversationMessages: any[] = convertMessages([...messages]);
      let iteration = 0;
      const toolDefinitions = getToolDefinitions();
      const hasTools = toolDefinitions.length > 0;

      console.log('🚀 Starting chat request to port:', selectedServerPort);
      console.log('📝 Initial messages:', messages.length);
      console.log(
        '🔧 Tools available:',
        hasTools ? toolDefinitions.map((t) => t.function.name) : 'none'
      );

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
          const error = new Error(
            `API error (${response.status}): ${errorText}`
          );
          onError?.(error);
          throw error;
        }

        if (!response.body) {
          const error = new Error(
            'Response body is null - streaming not supported'
          );
          onError?.(error);
          throw error;
        }

        // Stream the response for this iteration
        const reader = response.body.getReader();
        let mainContent = '';
        let hasReceivedMainContent = false;

        // Use extracted utilities
        const toolCallAccumulator = createToolCallAccumulator();
        const thinkingHandler = createThinkingContentHandler();
        let finishReason: string | null = null;

        try {
          for await (const delta of parseSSEStream(reader, abortSignal)) {
            // Track finish reason
            if (delta.finishReason) {
              finishReason = delta.finishReason;
            }

            // Handle reasoning_content
            if (delta.reasoningContent) {
              thinkingHandler.handleReasoningDelta(delta.reasoningContent);
              console.log('💭 Thinking delta:', delta.reasoningContent);
            }

            // Handle tool calls (accumulate by index)
            if (delta.toolCalls) {
              for (const tc of delta.toolCalls) {
                toolCallAccumulator.push(tc);
                console.log('🔧 Tool call delta:', tc);
              }
            }

            // Handle main content
            if (delta.content) {
              if (
                !hasReceivedMainContent &&
                thinkingHandler.getState().thinkingContent
              ) {
                thinkingHandler.markMainContentStarted();
                hasReceivedMainContent = true;
              }

              mainContent += delta.content;
              thinkingHandler.handleContentDelta(delta.content, mainContent);
              console.log('💬 Content delta:', delta.content);
            }

            // Build and yield display content
            const displayContent =
              thinkingHandler.buildDisplayContent(mainContent);
            yield {
              content: [{ type: 'text' as const, text: displayContent }],
            };
          }
        } catch (error) {
          console.error('❌ Stream error:', error);
          onError?.(error instanceof Error ? error : new Error(String(error)));
          throw error;
        }

        // Build final content
        const finalContent = thinkingHandler.buildFinalContent(mainContent);

        // Get accumulated tool calls
        const { toolCalls: accumulatedToolCalls } =
          toolCallAccumulator.getState();

        console.log('✅ Iteration complete');
        console.log(
          '   Content:',
          finalContent.substring(0, 100) +
            (finalContent.length > 100 ? '...' : '')
        );
        console.log(
          '   Tool calls:',
          accumulatedToolCalls.length > 0
            ? accumulatedToolCalls.map((tc) => tc.function.name)
            : 'none'
        );
        console.log('   Finish reason:', finishReason);

        // If no tool calls or not a tool_calls finish reason, we're done
        if (
          finishReason !== 'tool_calls' ||
          accumulatedToolCalls.length === 0
        ) {
          // Final yield with complete content
          yield {
            content: finalContent
              ? [{ type: 'text' as const, text: finalContent }]
              : [{ type: 'text' as const, text: '' }],
          };
          return;
        }

        // === Tool Calling: Execute tools and continue loop ===
        const result = yield* executeToolCalls(
          accumulatedToolCalls,
          finalContent
        );

        // Add messages for next iteration
        conversationMessages.push(result.assistantMessage);
        conversationMessages.push(...result.toolResultMessages);

        console.log('🔄 Continuing agentic loop with tool results...');
      }

      // Max iterations reached
      console.warn(`⚠️ Max tool iterations (${maxToolIterations}) reached`);
      yield {
        content: [
          {
            type: 'text' as const,
            text: `[Maximum tool calling iterations (${maxToolIterations}) reached. The conversation was truncated.]`,
          },
        ],
      };
    },
  };

  const runtime = useLocalRuntime(gglibAdapter);
  return runtime;
}

// =============================================================================
// Tool Execution
// =============================================================================

/**
 * Execute accumulated tool calls and yield results for UI updates.
 */
async function* executeToolCalls(
  toolCalls: AccumulatedToolCall[],
  textContent: string
): AsyncGenerator<
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  { content: any[] },
  {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    assistantMessage: any;
    toolResultMessages: { role: string; tool_call_id: string; content: string }[];
  },
  unknown
> {
  console.log('🔧 Executing tool calls...');

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const contentParts: any[] = [];

  // Add any text content from this iteration
  if (textContent) {
    contentParts.push({ type: 'text' as const, text: textContent });
  }

  // Process each tool call
  const toolResultMessages: {
    role: string;
    tool_call_id: string;
    content: string;
  }[] = [];

  for (const tc of toolCalls) {
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
      content: JSON.stringify(
        result.success ? result.data : { error: result.error }
      ),
    });
  }

  // Build assistant message with tool calls
  const assistantMessage = {
    role: 'assistant',
    content: textContent || null,
    tool_calls: toolCalls.map((tc) => ({
      id: tc.id,
      type: 'function',
      function: {
        name: tc.function.name,
        arguments: tc.function.arguments,
      },
    })),
  };

  return { assistantMessage, toolResultMessages };
}

// =============================================================================
// Utilities
// =============================================================================

/**
 * Fetch available servers (served models) from gglib
 *
 * @returns Promise resolving to array of server info objects
 */
export async function fetchAvailableServers(): Promise<ServerInfo[]> {
  try {
    return (await listServers()) as ServerInfo[];
  } catch (error) {
    console.error('Error fetching servers:', error);
    return [];
  }
}
