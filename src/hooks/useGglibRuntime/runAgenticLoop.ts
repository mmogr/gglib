/**
 * Agentic loop orchestration - manages multi-iteration tool calling.
 * 
 * Creates one assistant message per iteration. Tool results update
 * the tool-call parts directly (no separate tool messages).
 * 
 * @module runAgenticLoop
 */

import { appLogger } from '../../services/platform';
import type { GglibMessage, GglibContent } from '../../types/messages';
import type { ToolDefinition } from '../../services/tools';
import { getToolRegistry } from '../../services/tools';
import { streamModelResponse } from './streamModelResponse';
import type { ReasoningTimingTracker } from './reasoningTiming';
import {
  type AgentLoopState,
  type ToolDigest,
  DEFAULT_MAX_TOOL_ITERS,
  MAX_STAGNATION_STEPS,
  toolSignature,
  tryParseFinalEnvelope,
  recordAssistantProgress,
  checkToolLoop,
  buildWorkingMemory,
  upsertWorkingMemory,
  pruneForBudget,
  summarizeToolResult,
  withRetry,
} from './agentLoop';

/**
 * Convert GglibMessage[] to API message format for LLM
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function convertToApiMessages(messages: GglibMessage[]): any[] {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return messages.map((msg): any => {
    if (msg.role === 'system' || msg.role === 'user') {
      // Extract text content
      const content = Array.isArray(msg.content) 
        ? msg.content
            .filter((p: any) => p.type === 'text')
            .map((p: any) => p.text)
            .join('')
        : msg.content;
      return {
        role: msg.role,
        content,
      };
    } else if (msg.role === 'assistant') {
      // Extract text and tool calls
      const textParts = Array.isArray(msg.content) 
        ? msg.content.filter((p: any) => p.type === 'text')
        : [];
      const text = textParts.map((p: any) => p.text || '').join('');
      
      const toolCallParts = Array.isArray(msg.content)
        ? msg.content.filter((p: any) => p.type === 'tool-call')
        : [];
      const toolCalls = toolCallParts.map((p: any) => ({
        id: p.toolCallId,
        type: 'function',
        function: {
          name: p.toolName,
          arguments: p.argsText || JSON.stringify(p.args || {}),
        },
      }));
      
      return {
        role: 'assistant',
        content: text || null,
        ...(toolCalls.length > 0 && { tool_calls: toolCalls }),
      };
    }
    return null;
  }).filter(Boolean);
}

export interface RunAgenticLoopOptions {
  turnId: string;
  getMessages: () => GglibMessage[];
  setMessages: React.Dispatch<React.SetStateAction<GglibMessage[]>>;
  selectedServerPort: number;
  maxToolIterations?: number;
  maxStagnationSteps?: number;
  abortSignal?: AbortSignal;
  conversationId?: number;
  mkAssistantMessage: (custom?: any) => GglibMessage;
  timingTracker?: ReasoningTimingTracker;
  setCurrentStreamingAssistantMessageId?: (id: string | null) => void;
}

/**
 * Run the agentic loop - creates one assistant message per iteration.
 * 
 * Each iteration:
 * 1. Creates a new assistant message
 * 2. Streams LLM response into that specific message
 * 3. If tool calls are made, executes them and updates tool-call parts with results
 * 4. Continues to next iteration or stops
 */
export async function runAgenticLoop(options: RunAgenticLoopOptions): Promise<void> {
  const {
    turnId,
    getMessages,
    setMessages,
    selectedServerPort,
    maxToolIterations = DEFAULT_MAX_TOOL_ITERS,
    maxStagnationSteps = MAX_STAGNATION_STEPS,
    abortSignal,
    conversationId,
    mkAssistantMessage: mkAssistant,
    timingTracker,
    setCurrentStreamingAssistantMessageId,
  } = options;

  let iteration = 0;

  // Get tool definitions
  const toolDefinitions = getToolDefinitions();
  const hasTools = toolDefinitions.length > 0;

  appLogger.debug('hook.runtime', 'Starting agentic loop', {
    maxIterations: maxToolIterations,
    maxStagnation: maxStagnationSteps,
    tools: hasTools ? toolDefinitions.length : 0,
  });
  
  // Log available tool names for debugging
  if (hasTools) {
    appLogger.debug('hook.runtime', 'Available tools', { toolNames: toolDefinitions.map(t => t.function.name) });
  }

  // Initialize agent state
  let agentState: AgentLoopState = {
    iter: 0,
    protocolStrikes: 0,
    stagnation: 0,
    sigHits: new Map(),
    toolDigests: [],
  };

  // Convert current messages to API format
  const rawMessages = getMessages();
  appLogger.debug('hook.runtime', 'Raw messages from store', { 
    messages: rawMessages.map(m => ({ 
      role: m.role, 
      contentType: typeof m.content, 
      contentLength: Array.isArray(m.content) ? m.content.length : m.content?.length 
    }))
  });
  
  let apiMessages = convertToApiMessages(rawMessages);
  appLogger.debug('hook.runtime', 'Converted API messages', { 
    messages: apiMessages.map(m => ({ 
      role: m.role, 
      contentPreview: m.content?.substring?.(0, 50) || m.content 
    }))
  });
  
  // Initialize working memory
  apiMessages = upsertWorkingMemory(apiMessages, buildWorkingMemory(agentState.toolDigests));
  apiMessages = pruneForBudget(apiMessages);

  // AGENTIC LOOP - one iteration = one assistant message
  while (iteration < maxToolIterations) {
    iteration++;
    agentState.iter = iteration;

    appLogger.debug('hook.runtime', 'Starting iteration', { iteration, maxIterations: maxToolIterations });

    // Check stagnation before creating message
    if (agentState.stagnation >= maxStagnationSteps) {
      appLogger.warn('hook.runtime', 'Stagnation detected (repeated output)');
      const stagnationMessage: GglibMessage = {
        ...mkAssistant({ turnId, iteration, conversationId }),
        content: [
          {
            type: 'text',
            text: '[Stopped: assistant repeated itself without making progress.]',
          },
        ],
      };
      setMessages(prev => [...prev, stagnationMessage]);
      break;
    }

    // Create NEW assistant message for this iteration
    const assistantMessage = mkAssistant({ turnId, iteration, conversationId });
    const assistantMessageId = assistantMessage.id!; // mkAssistant always provides id

    // Append new message to store
    setMessages(prev => [...prev, assistantMessage]);

    // Mark this message as currently streaming (for live timer)
    setCurrentStreamingAssistantMessageId?.(assistantMessageId);

    // Stream LLM response INTO this specific message
    const streamResult = await streamModelResponse({
      serverPort: selectedServerPort,
      messages: apiMessages,
      toolDefinitions,
      abortSignal,
      
      // Update THIS message's content by ID
      onContentUpdate: (content: GglibContent) => {
        setMessages(prev =>
          prev.map(m =>
            m.id === assistantMessageId
              ? { ...m, content }
              : m
          )
        );
      },
      
      // Pass timing tracker and message ID for duration tracking
      messageId: assistantMessageId,
      timingTracker,
    });

    // Clear streaming state (stream completed for this message)
    setCurrentStreamingAssistantMessageId?.(null);

    // Mark timing as finalized to trigger final persist with durations
    // This ensures the transcript is regenerated with duration attributes
    // Only set if not already finalized (one-way flag)
    setMessages(prev =>
      prev.map(m => {
        if (m.id !== assistantMessageId) return m;
        const alreadyFinalized = (m.metadata as any)?.custom?.timingFinalized;
        if (alreadyFinalized) return m; // Already finalized, no-op
        
        return {
          ...m,
          metadata: {
            ...m.metadata,
            custom: {
              ...(m.metadata as any)?.custom,
              timingFinalized: true,
            },
          },
        };
      })
    );

    // Track progress for stagnation detection
    agentState = recordAssistantProgress(agentState, streamResult.textContent);

    // Check if we have a final envelope (no tool calls expected)
    if (
      streamResult.finishReason !== 'tool_calls' ||
      streamResult.toolCalls.length === 0
    ) {
      const finalEnvelope = tryParseFinalEnvelope(streamResult.textContent);
      
      if (!finalEnvelope) {
        // Model didn't produce a final envelope - check if this is first iteration
        // On first iteration with no tools called, accept as plain response (model may not support tools)
        if (iteration === 1) {
          appLogger.info('hook.runtime', 'Model responded without tools or final envelope - accepting as plain response');
          // Keep the response as-is (already set by streaming)
          break;
        }
        
        // In subsequent iterations, this is a protocol violation
        agentState.protocolStrikes++;
        appLogger.warn('hook.runtime', 'Protocol violation: expected final envelope', { 
          strikes: agentState.protocolStrikes 
        });
        
        if (agentState.protocolStrikes > 2) {
          // Add error message
          setMessages(prev =>
            prev.map(m =>
              m.id === assistantMessageId
                ? {
                    ...m,
                    content: [
                      ...(Array.isArray(m.content) ? m.content : []),
                      {
                        type: 'text',
                        text: '\n\n[Stopped: protocol violations]',
                      },
                    ] as GglibContent,
                  }
                : m
            )
          );
          break;
        }
        // Continue - maybe next iteration will comply
        continue;
      }
      
      // Success - update with final result
      setMessages(prev =>
        prev.map(m =>
          m.id === assistantMessageId
            ? {
                ...m,
                content: [
                  {
                    type: 'text',
                    text: finalEnvelope.result,
                  },
                ] as GglibContent,
              }
            : m
        )
      );
      appLogger.info('hook.runtime', 'Final envelope received - conversation complete');
      break;
    }

    // Check for tool loops
    const { loopDetected, updatedState } = checkToolLoop(
      agentState,
      streamResult.toolCalls
    );
    agentState = updatedState;

    if (loopDetected) {
      appLogger.warn('hook.runtime', 'Tool loop detected');
      setMessages(prev =>
        prev.map(m =>
          m.id === assistantMessageId
            ? {
                ...m,
                content: [
                  ...(Array.isArray(m.content) ? m.content : []),
                  {
                    type: 'text',
                    text: '\n\n[Stopped: repeating the same tool calls without progress.]',
                  },
                ] as GglibContent,
              }
            : m
        )
      );
      break;
    }

    // Execute tools and UPDATE tool-call parts with results
    appLogger.debug('hook.runtime', 'Executing tools', { toolCount: streamResult.toolCalls.length });
    
    const toolCallsForApiHistory: any[] = [];
    const toolResultsForApiHistory: any[] = [];
    
    for (const toolCall of streamResult.toolCalls) {
      // Execute tool
      const registry = getToolRegistry();
      const result = await withRetry(
        () =>
          registry.executeRawCall({
            id: toolCall.id,
            type: 'function',
            function: toolCall.function,
          }),
        { maxRetries: 2, baseDelayMs: 250 }
      ).catch((e) => ({
        success: false as const,
        error: String((e as { message?: string })?.message ?? e ?? 'Unknown error'),
      }));

      appLogger.debug('hook.runtime', 'Tool executed', { toolName: toolCall.function.name, result });

      // Create digest for working memory
      const digest: ToolDigest = {
        sig: toolSignature(toolCall),
        name: toolCall.function.name,
        ok: result.success,
        summary: summarizeToolResult(toolCall.function.name, result),
      };
      agentState.toolDigests.push(digest);

      // Update the tool-call part with result
      setMessages(prev =>
        prev.map(m => {
          if (m.id !== assistantMessageId) return m;
          
          const updatedContent = Array.isArray(m.content)
            ? m.content.map((p: any) =>
                p.type === 'tool-call' && p.toolCallId === toolCall.id
                  ? {
                      ...p,
                      result: result.success ? result.data : { error: result.error },
                      isError: !result.success,
                    }
                  : p
              )
            : m.content;
          
          return { ...m, content: updatedContent as GglibContent };
        })
      );

      // Add to API messages for next iteration (OpenAI format requires tool results)
      toolCallsForApiHistory.push({
        id: toolCall.id,
        type: 'function',
        function: {
          name: toolCall.function.name,
          arguments: toolCall.function.arguments,
        },
      });

      toolResultsForApiHistory.push({
        role: 'tool',
        tool_call_id: toolCall.id,
        content: JSON.stringify(result.success ? result.data : { error: result.error }),
      });
    }

    // Add to API messages for next iteration
    apiMessages.push({
      role: 'assistant',
      content: streamResult.textContent || null,
      tool_calls: toolCallsForApiHistory,
    });

    apiMessages.push(...toolResultsForApiHistory);

    // Update working memory and prune for next iteration
    apiMessages = upsertWorkingMemory(apiMessages, buildWorkingMemory(agentState.toolDigests));
    apiMessages = pruneForBudget(apiMessages);

    appLogger.debug('hook.runtime', 'Continuing to next iteration');
  }

  if (iteration >= maxToolIterations) {
    appLogger.warn('hook.runtime', 'Max iterations reached', { maxIterations: maxToolIterations });
    const maxIterMessage: GglibMessage = {
      ...mkAssistant({ turnId, iteration, conversationId }),
      content: [
        {
          type: 'text',
          text: `[Maximum tool calling iterations (${maxToolIterations}) reached. The conversation was truncated.]`,
        },
      ],
    };
    setMessages(prev => [...prev, maxIterMessage]);
  }

  appLogger.debug('hook.runtime', 'Agentic loop complete');
}

/**
 * Get enabled tool definitions from the registry.
 */
function getToolDefinitions(): ToolDefinition[] {
  const registry = getToolRegistry();
  return registry.getEnabledDefinitions();
}
