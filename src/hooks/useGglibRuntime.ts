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

/** Delta content from SSE stream */
interface StreamDelta {
  /** Main content delta */
  content: string | null;
  /** Reasoning/thinking content delta (from reasoning models) */
  reasoningContent: string | null;
}

/**
 * Parse SSE events from a streaming response.
 * Handles OpenAI-compatible streaming format with `data:` prefixed lines.
 * Extracts both `content` and `reasoning_content` from delta objects.
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
          const delta = chunk.choices?.[0]?.delta;
          
          // Extract both content and reasoning_content from delta
          const contentDelta = delta?.content ?? null;
          const reasoningDelta = delta?.reasoning_content ?? null;
          
          // Only yield if we have some content
          if (contentDelta || reasoningDelta) {
            yield {
              content: contentDelta,
              reasoningContent: reasoningDelta,
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

      try {
        for await (const delta of parseSSEStream(reader, abortSignal)) {
          // Handle reasoning_content (from llama-server with reasoning format enabled)
          if (delta.reasoningContent) {
            thinkingContent += delta.reasoningContent;
            console.log('💭 Thinking delta:', delta.reasoningContent);
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

      console.log('✅ Streaming complete, total content:', finalContent);

      // Final yield with complete content (no return value needed for async generator)
      yield {
        content: [{ type: 'text' as const, text: finalContent }],
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
