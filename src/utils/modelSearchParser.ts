/**
 * What the library's search text asks of Hugging Face (`places/library/hf`).
 *
 * Supports 4 intent types:
 * - search: Normal text search query
 * - repo: Exact HuggingFace repository (user/repo)
 * - download: Exact repo with quantization (user/repo:quant)
 * - url: HuggingFace URL, with the repository and the file it names
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
 * Download pattern: namespace/repo:label
 * The label is the variant's as the repository's listing gives it (`Q4_K_M`,
 * `UD-Q4_K_XL`, `FastMTP-32K`): non-empty, with no spaces, slashes or colons.
 * The daemon matches it to a variant; nothing here reads a quant from it.
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
  /^https?:\/\/huggingface\.co\/([a-zA-Z0-9._-]+)\/([a-zA-Z0-9._-]+)(\/.*)?$/;

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
 * A GGUF file's URL in a repository: `/blob/<rev>/<path>.gguf` or
 * `/resolve/<rev>/<path>.gguf`, before any query or fragment.
 */
const HF_FILE_PATH_PATTERN = /^\/(?:blob|resolve)\/[^/]+\/(?:.*\/)?([^/?#]+)\.gguf(?:[?#].*)?$/i;

/** llama.cpp's shard numbering at the end of a stem: `-00001-of-00008`. */
const SHARD_SUFFIX_PATTERN = /-\d+-of-\d+$/;

/** What a request's label may hold, as for `repo:label`. */
const REQUEST_PATTERN = /^[a-zA-Z0-9._-]+$/;

/**
 * The name a file's URL asks for its variant by: the file's base name less
 * `.gguf` and a shard's numbering. No quant is read from it: the daemon
 * matches it to the variant whose file it names.
 */
function fileStemOf(path: string): string | undefined {
  const match = path.match(HF_FILE_PATH_PATTERN);
  if (!match) {
    return undefined;
  }
  let base: string;
  try {
    base = decodeURIComponent(match[1]);
  } catch {
    return undefined;
  }
  const stem = base.replace(SHARD_SUFFIX_PATTERN, "");
  return REQUEST_PATTERN.test(stem) ? stem : undefined;
}

/**
 * Extract the repo from a HuggingFace URL, and when the URL is a GGUF file's,
 * the file's stem to ask for its variant by.
 */
function parseHuggingFaceUrl(url: string): { repo: string; quant?: string } | null {
  const match = url.match(HF_URL_PATTERN);
  if (!match) {
    return null;
  }

  const [, namespace, repoName, rest] = match;
  return {
    repo: `${namespace}/${repoName}`,
    quant: rest ? fileStemOf(rest) : undefined,
  };
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
