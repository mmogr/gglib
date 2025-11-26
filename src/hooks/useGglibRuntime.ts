import { useLocalRuntime, type ChatModelAdapter } from '@assistant-ui/react';
import { TauriService } from '../services/tauri';
import { getApiBase } from '../utils/apiBase';

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

/**
 * Parse SSE events from a streaming response.
 * Handles OpenAI-compatible streaming format with `data:` prefixed lines.
 */
async function* parseSSEStream(
  reader: ReadableStreamDefaultReader<Uint8Array>,
  abortSignal?: AbortSignal
): AsyncGenerator<string, void, unknown> {
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

        // Parse JSON chunk and extract content delta
        try {
          const chunk = JSON.parse(dataLine);
          const delta = chunk.choices?.[0]?.delta?.content;
          if (delta) {
            yield delta;
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
      let fullContent = '';

      try {
        for await (const delta of parseSSEStream(reader, abortSignal)) {
          fullContent += delta;
          console.log('💬 Streaming delta:', delta);
          
          // Yield partial content to update the UI progressively
          yield {
            content: [{ type: 'text' as const, text: fullContent }],
          };
        }
      } catch (error) {
        // On error, discard partial content (as discussed)
        console.error('❌ Stream error:', error);
        onError?.(error instanceof Error ? error : new Error(String(error)));
        throw error;
      }

      console.log('✅ Streaming complete, total content:', fullContent);

      // Final yield with complete content (no return value needed for async generator)
      yield {
        content: [{ type: 'text' as const, text: fullContent }],
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
