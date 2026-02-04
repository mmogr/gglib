/**
 * Simplified model streaming - one LLM call, no agentic loop.
 * 
 * This function handles a single streaming request to the LLM and returns
 * the complete response including content, tool calls, and finish reason.
 * 
 * @module streamModelResponse
 */

import { appLogger } from '../../services/platform';
import { getAuthenticatedFetchConfig } from '../../services/transport/api/client';
import type { ToolDefinition } from '../../services/tools';
import { parseSSEStream } from './parseSSEStream';
import { createToolCallAccumulator, type AccumulatedToolCall } from './accumulateToolCalls';
import { createThinkingContentHandler } from './thinkingContentHandler';
import { PartsAccumulator } from './partsAccumulator';
import type { GglibContent } from '../../types/messages';
import { DEFAULT_SYSTEM_PROMPT, TOOL_ENABLED_SYSTEM_PROMPT, withRetry } from './agentLoop';
import type { ReasoningTimingTracker } from './reasoningTiming';

function hotSwapDefaultSystemPrompt(messages: any[], hasTools: boolean): any[] {
  if (!hasTools) return messages;

  // Only swap when the stored system prompt is *exactly* the default prompt.
  // This preserves user customizations (even minor edits/whitespace changes).
  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i];
    if (msg?.role === 'system' && typeof msg.content === 'string' && msg.content === DEFAULT_SYSTEM_PROMPT) {
      const cloned = messages.slice();
      cloned[i] = { ...msg, content: TOOL_ENABLED_SYSTEM_PROMPT };
      return cloned;
    }
  }

  return messages;
}

export interface StreamModelResponseOptions {
  serverPort: number;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  messages: any[];
  toolDefinitions: ToolDefinition[];
  abortSignal?: AbortSignal;
  onContentUpdate: (content: GglibContent) => void;
  messageId?: string;
  timingTracker?: ReasoningTimingTracker;
}

export interface StreamModelResponseResult {
  content: GglibContent;
  textContent: string;
  toolCalls: AccumulatedToolCall[];
  finishReason: string | null;
}

/**
 * Stream a single LLM response with real-time content updates.
 * 
 * @param options - Configuration including server port, messages, tools, and update callback
 * @returns Promise resolving to complete response with content, tool calls, and finish reason
 */
export async function streamModelResponse(
  options: StreamModelResponseOptions
): Promise<StreamModelResponseResult> {
  const { serverPort, messages, toolDefinitions, abortSignal, onContentUpdate, messageId, timingTracker } = options;

  const hasTools = toolDefinitions.length > 0;
  const effectiveMessages = hotSwapDefaultSystemPrompt(messages, hasTools);

  const requestBody = {
    port: serverPort,
    model: 'default',
    messages: effectiveMessages,
    stream: true,
    ...(hasTools && { tools: toolDefinitions }),
    // Inference parameters can be added here if needed
    // They will be resolved via hierarchy: Request → Model → Global → Hardcoded
  };

  appLogger.debug('hook.runtime', 'Streaming model request', {
    port: serverPort,
    messageCount: messages.length,
    toolCount: toolDefinitions.length,
  });

  const { baseUrl, headers: authHeaders } = await getAuthenticatedFetchConfig();

  // Retry on transient errors
  const response = await withRetry(
    async () => {
      const res = await fetch(`${baseUrl}/api/chat`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...authHeaders,
        },
        body: JSON.stringify(requestBody),
        signal: abortSignal,
      });
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${await res.text()}`);
      }
      return res;
    },
    { maxRetries: 1, baseDelayMs: 300 }
  );

  if (!response.body) {
    throw new Error('Response body is null - streaming not supported');
  }

  const reader = response.body.getReader();
  let mainContent = '';
  let hasReceivedMainContent = false;

  // Accumulate parts for this message
  const partsAcc = new PartsAccumulator();
  const toolCallAccumulator = createToolCallAccumulator();
  const thinkingHandler = createThinkingContentHandler();
  let finishReason: string | null = null;

  try {
    for await (const delta of parseSSEStream(reader, abortSignal)) {
      // Track finish reason
      if (delta.finishReason) {
        finishReason = delta.finishReason;
      }

      // Handle reasoning content
      if (delta.reasoningContent) {
        // Track timing: start segment on first reasoning
        if (messageId && timingTracker) {
          timingTracker.onReasoning(messageId);
        }
        
        thinkingHandler.handleReasoningDelta(delta.reasoningContent, partsAcc);
        if (!hasReceivedMainContent && !thinkingHandler.isThinking()) {
          appLogger.debug('hook.runtime', 'Reasoning started');
        }
      }

      // Handle tool calls (accumulate by index)
      if (delta.toolCalls) {
        // Track timing: tool call is a boundary (ends reasoning segment)
        if (messageId && timingTracker) {
          timingTracker.onBoundary(messageId);
        }
        
        for (const tc of delta.toolCalls) {
          toolCallAccumulator.push(tc);
          
          // Get accumulated state and add to parts accumulator
          const { toolCalls: accumulated } = toolCallAccumulator.getState();
          for (const accTc of accumulated) {
            // Try to parse args - may be incomplete during streaming
            let parsedArgs: Record<string, unknown> | undefined;
            try {
              parsedArgs = JSON.parse(accTc.function.arguments || '{}');
            } catch {
              // Arguments are incomplete - keep as undefined for now, will be updated on next chunk
            }
            
            partsAcc.setToolCall({
              type: 'tool-call',
              toolCallId: accTc.id,
              toolName: accTc.function.name,
              args: parsedArgs as any, // May be undefined during streaming
              argsText: accTc.function.arguments, // Required by assistant-ui
            });
          }
        }
      }

      // Handle main content
      if (delta.content) {
        if (!hasReceivedMainContent && thinkingHandler.isThinking()) {
          appLogger.debug('hook.runtime', 'Reasoning completed');
          
          // Track timing: text is a boundary (ends reasoning segment)
          if (messageId && timingTracker) {
            timingTracker.onBoundary(messageId);
          }
          
          thinkingHandler.markMainContentStarted();
          hasReceivedMainContent = true;
        }

        mainContent += delta.content;
        thinkingHandler.handleContentDelta(delta.content, mainContent, partsAcc);
      }

      // Update content via callback
      onContentUpdate(partsAcc.snapshot());
    }
  } catch (error) {
    appLogger.error('hook.runtime', 'Stream error', { error });
    throw error;
  }

  // Get final results
  const finalSnapshot = partsAcc.snapshot();
  const finalContent = finalSnapshot
    .filter(p => p.type === 'text')
    .map(p => 'text' in p ? p.text : '')
    .join('');

  const { toolCalls: accumulatedToolCalls } = toolCallAccumulator.getState();

  // Track timing: end of message (finalize any open segment)
  if (messageId && timingTracker) {
    timingTracker.onEndOfMessage(messageId);
  }

  appLogger.debug('hook.runtime', 'Stream complete', {
    contentLength: finalContent.length,
    toolCalls: accumulatedToolCalls.length,
    finishReason,
  });

  return {
    content: finalSnapshot,
    textContent: finalContent,
    toolCalls: accumulatedToolCalls,
    finishReason,
  };
}
