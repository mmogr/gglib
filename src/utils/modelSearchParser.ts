/**
 * Parser utility for detecting user intent from HuggingFace Browser search input.
 *
 * Supports 4 intent types:
 * - search: Normal text search query
 * - repo: Exact HuggingFace repository (user/repo)
 * - download: Exact repo with quantization (user/repo:quant)
 * - url: HuggingFace URL with optional repo/quant extraction
 */

export type ModelSearchIntent =
  | { kind: "search"; query: string }
  | { kind: "repo"; repo: string }
  | { kind: "download"; repo: string; quant: string }
  | { kind: "url"; url: string; repo?: string; quant?: string };

/**
 * HuggingFace repo ID pattern: namespace/repo
 * Allows alphanumeric, dots, underscores, hyphens in both parts
 */
const REPO_PATTERN = /^([a-zA-Z0-9._-]+)\/([a-zA-Z0-9._-]+)$/;

/**
 * Download pattern: namespace/repo:quant
 * Quant must be non-empty and contain only valid characters (no spaces, slashes)
 */
const DOWNLOAD_PATTERN = /^([a-zA-Z0-9._-]+)\/([a-zA-Z0-9._-]+):([a-zA-Z0-9._-]+)$/;

/**
 * HuggingFace URL patterns:
 * - https://huggingface.co/user/repo
 * - https://huggingface.co/user/repo/tree/main
 * - https://huggingface.co/user/repo/blob/main/filename.gguf
 * - https://huggingface.co/user/repo/resolve/main/filename.gguf
 */
const HF_URL_PATTERN =
  /^https?:\/\/huggingface\.co\/([a-zA-Z0-9._-]+)\/([a-zA-Z0-9._-]+)(?:\/.*)?$/;

/**
 * Extract quantization from a GGUF filename
 * Matches patterns like: model.Q4_K_M.gguf, model-Q8_0.gguf, model_IQ4_XS.gguf
 */
const GGUF_QUANT_PATTERN = /[._-](Q[0-9]+_[A-Z0-9_]+|IQ[0-9]+_[A-Z0-9_]+|F16|F32|BF16)\.gguf$/i;

/**
 * Normalize input by trimming whitespace and removing surrounding quotes
 */
function normalizeInput(input: string): string {
  let normalized = input.trim();

  // Remove surrounding single or double quotes
  if (
    (normalized.startsWith('"') && normalized.endsWith('"')) ||
    (normalized.startsWith("'") && normalized.endsWith("'"))
  ) {
    normalized = normalized.slice(1, -1).trim();
  }

  return normalized;
}

/**
 * Check if input looks like a URL (starts with http:// or https://)
 */
function isUrl(input: string): boolean {
  return /^https?:\/\//i.test(input);
}

/**
 * Extract repo and optional quant from a HuggingFace URL
 */
function parseHuggingFaceUrl(
  url: string
): { repo: string; quant?: string } | null {
  const match = url.match(HF_URL_PATTERN);
  if (!match) {
    return null;
  }

  const [, namespace, repoName] = match;
  const repo = `${namespace}/${repoName}`;

  // Try to extract quant from filename in URL path
  const quantMatch = url.match(GGUF_QUANT_PATTERN);
  const quant = quantMatch ? quantMatch[1].toUpperCase() : undefined;

  return { repo, quant };
}

/**
 * Parse search input and determine user intent.
 *
 * @param input - Raw search bar input
 * @returns Discriminated union describing the detected intent
 *
 * @example
 * parseModelSearchIntent("llama")
 * // => { kind: "search", query: "llama" }
 *
 * @example
 * parseModelSearchIntent("bartowski/Llama-3.2-3B")
 * // => { kind: "repo", repo: "bartowski/Llama-3.2-3B" }
 *
 * @example
 * parseModelSearchIntent("bartowski/Llama-3.2-3B:Q4_K_M")
 * // => { kind: "download", repo: "bartowski/Llama-3.2-3B", quant: "Q4_K_M" }
 *
 * @example
 * parseModelSearchIntent("https://huggingface.co/user/repo")
 * // => { kind: "url", url: "...", repo: "user/repo" }
 */
export function parseModelSearchIntent(input: string): ModelSearchIntent {
  const normalized = normalizeInput(input);

  // Empty input falls back to search
  if (!normalized) {
    return { kind: "search", query: "" };
  }

  // Check for URL first
  if (isUrl(normalized)) {
    const hfParsed = parseHuggingFaceUrl(normalized);
    if (hfParsed) {
      return {
        kind: "url",
        url: normalized,
        repo: hfParsed.repo,
        quant: hfParsed.quant,
      };
    }
    // Non-HF URLs fall back to search (future: could support direct .gguf links)
    return { kind: "search", query: normalized };
  }

  // Check for download pattern (repo:quant)
  const downloadMatch = normalized.match(DOWNLOAD_PATTERN);
  if (downloadMatch) {
    const [, namespace, repoName, quant] = downloadMatch;
    return {
      kind: "download",
      repo: `${namespace}/${repoName}`,
      quant,
    };
  }

  // Check for exact repo pattern (namespace/repo)
  const repoMatch = normalized.match(REPO_PATTERN);
  if (repoMatch) {
    const [, namespace, repoName] = repoMatch;
    return {
      kind: "repo",
      repo: `${namespace}/${repoName}`,
    };
  }

  // Default: treat as search query
  return { kind: "search", query: normalized };
}

/**
 * Get display text for the search button based on current intent
 */
export function getButtonTextForIntent(intent: ModelSearchIntent): string {
  switch (intent.kind) {
    case "download":
      return "⬇️ Download";
    case "repo":
      return "View Model";
    case "url":
      // If we extracted repo info from URL, show appropriate action
      if (intent.quant) {
        return "⬇️ Download";
      }
      if (intent.repo) {
        return "View Model";
      }
      return "Search";
    case "search":
    default:
      return "Search";
  }
}

/**
 * Get button variant/style class based on current intent
 */
export function getButtonVariantForIntent(
  intent: ModelSearchIntent
): "default" | "primary" | "accent" {
  switch (intent.kind) {
    case "download":
      return "accent";
    case "repo":
      return "primary";
    case "url":
      if (intent.quant || intent.repo) {
        return intent.quant ? "accent" : "primary";
      }
      return "default";
    case "search":
    default:
      return "default";
  }
}
