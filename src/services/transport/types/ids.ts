/**
 * Shared ID types for transport layer.
 * Using branded types for type safety while keeping numeric/string underlying types.
 */

// Core entity IDs (database-backed, always numeric)
export type ModelId = number;
export type TagId = number;
export type McpServerId = number;
export type ConversationId = number;
export type MessageId = number;

// Composite/string-based IDs
export type DownloadId = string; // Format: "model_id:quantization"
export type HfModelId = string; // HuggingFace repo path, e.g., "TheBloke/Llama-2-7B-GGUF"

// Server identification (uses ModelId since one server per model)
export type ServerId = ModelId;
