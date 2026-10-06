/**
 * Shared ID types for transport layer. Plain aliases: they name what a number
 * or a string identifies, and the compiler treats one as any other.
 */

// Core entity IDs (database-backed, always numeric)
export type ModelId = number;
export type McpServerId = number;
export type ConversationId = number;
export type MessageId = number;

// Composite/string-based IDs
export type DownloadId = string; // Format: "model_id:quantization"
export type HfModelId = string; // HuggingFace repo path, e.g., "TheBloke/Llama-2-7B-GGUF"
