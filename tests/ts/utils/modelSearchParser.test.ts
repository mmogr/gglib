import { describe, it, expect } from "vitest";
import {
  parseModelSearchIntent,
  getButtonTextForIntent,
  getButtonVariantForIntent,
  type ModelSearchIntent,
} from "../../../src/utils/modelSearchParser";

describe("parseModelSearchIntent", () => {
  describe("search intent (default)", () => {
    it("returns search intent for simple text", () => {
      const result = parseModelSearchIntent("llama");
      expect(result).toEqual({ kind: "search", query: "llama" });
    });

    it("returns search intent for multi-word query", () => {
      const result = parseModelSearchIntent("llama 3.2 instruct");
      expect(result).toEqual({ kind: "search", query: "llama 3.2 instruct" });
    });

    it("returns search intent for empty input", () => {
      const result = parseModelSearchIntent("");
      expect(result).toEqual({ kind: "search", query: "" });
    });

    it("returns search intent for whitespace-only input", () => {
      const result = parseModelSearchIntent("   ");
      expect(result).toEqual({ kind: "search", query: "" });
    });

    it("returns search intent for single token (not a repo)", () => {
      const result = parseModelSearchIntent("foo");
      expect(result).toEqual({ kind: "search", query: "foo" });
    });

    it("returns search intent for too many segments (foo/bar/baz)", () => {
      const result = parseModelSearchIntent("foo/bar/baz");
      expect(result).toEqual({ kind: "search", query: "foo/bar/baz" });
    });
  });

  describe("repo intent", () => {
    it("detects simple repo pattern", () => {
      const result = parseModelSearchIntent("bartowski/Llama-3.2-3B");
      expect(result).toEqual({ kind: "repo", repo: "bartowski/Llama-3.2-3B" });
    });

    it("allows dots in namespace", () => {
      const result = parseModelSearchIntent("user.name/model-repo");
      expect(result).toEqual({ kind: "repo", repo: "user.name/model-repo" });
    });

    it("allows dots in repo name", () => {
      const result = parseModelSearchIntent("TheBloke/Llama-2.7B-GGUF");
      expect(result).toEqual({ kind: "repo", repo: "TheBloke/Llama-2.7B-GGUF" });
    });

    it("allows underscores in namespace and repo", () => {
      const result = parseModelSearchIntent("user_name/model_repo");
      expect(result).toEqual({ kind: "repo", repo: "user_name/model_repo" });
    });

    it("allows hyphens in namespace and repo", () => {
      const result = parseModelSearchIntent("my-user/my-model-gguf");
      expect(result).toEqual({ kind: "repo", repo: "my-user/my-model-gguf" });
    });

    it("allows mixed special chars", () => {
      const result = parseModelSearchIntent("Org.Name_Test/Model-Name_v1.2");
      expect(result).toEqual({
        kind: "repo",
        repo: "Org.Name_Test/Model-Name_v1.2",
      });
    });

    it("strips surrounding double quotes", () => {
      const result = parseModelSearchIntent('"bartowski/Llama-3.2-3B"');
      expect(result).toEqual({ kind: "repo", repo: "bartowski/Llama-3.2-3B" });
    });

    it("strips surrounding single quotes", () => {
      const result = parseModelSearchIntent("'bartowski/Llama-3.2-3B'");
      expect(result).toEqual({ kind: "repo", repo: "bartowski/Llama-3.2-3B" });
    });

    it("trims whitespace", () => {
      const result = parseModelSearchIntent("  bartowski/Llama-3.2-3B  ");
      expect(result).toEqual({ kind: "repo", repo: "bartowski/Llama-3.2-3B" });
    });
  });

  describe("download intent (repo:quant)", () => {
    it("detects simple download pattern", () => {
      const result = parseModelSearchIntent("bartowski/Llama-3.2-3B:Q4_K_M");
      expect(result).toEqual({
        kind: "download",
        repo: "bartowski/Llama-3.2-3B",
        quant: "Q4_K_M",
      });
    });

    it("handles Q8_0 quantization", () => {
      const result = parseModelSearchIntent("user/model:Q8_0");
      expect(result).toEqual({
        kind: "download",
        repo: "user/model",
        quant: "Q8_0",
      });
    });

    it("handles IQ4_XS quantization", () => {
      const result = parseModelSearchIntent("user/model:IQ4_XS");
      expect(result).toEqual({
        kind: "download",
        repo: "user/model",
        quant: "IQ4_XS",
      });
    });

    it("handles F16 quantization", () => {
      const result = parseModelSearchIntent("user/model:F16");
      expect(result).toEqual({
        kind: "download",
        repo: "user/model",
        quant: "F16",
      });
    });

    it("handles complex quant string Q4_K_S", () => {
      const result = parseModelSearchIntent("MaziyarPanahi/Qwen3-8B-GGUF:Q4_K_S");
      expect(result).toEqual({
        kind: "download",
        repo: "MaziyarPanahi/Qwen3-8B-GGUF",
        quant: "Q4_K_S",
      });
    });

    it("allows dots in repo with quant", () => {
      const result = parseModelSearchIntent("user.name/model.v2:Q4_K_M");
      expect(result).toEqual({
        kind: "download",
        repo: "user.name/model.v2",
        quant: "Q4_K_M",
      });
    });
  });

  describe("download intent - invalid patterns (should NOT match)", () => {
    it("rejects empty quant (foo/bar:)", () => {
      const result = parseModelSearchIntent("foo/bar:");
      expect(result).toEqual({ kind: "search", query: "foo/bar:" });
    });

    it("rejects quant with spaces (foo/bar:Q4 K M)", () => {
      const result = parseModelSearchIntent("foo/bar:Q4 K M");
      expect(result).toEqual({ kind: "search", query: "foo/bar:Q4 K M" });
    });

    it("rejects quant with slashes (foo/bar:Q4/K/M)", () => {
      const result = parseModelSearchIntent("foo/bar:Q4/K/M");
      expect(result).toEqual({ kind: "search", query: "foo/bar:Q4/K/M" });
    });

    it("rejects multiple colons", () => {
      const result = parseModelSearchIntent("foo/bar:Q4:K:M");
      expect(result).toEqual({ kind: "search", query: "foo/bar:Q4:K:M" });
    });
  });

  describe("url intent - HuggingFace URLs", () => {
    it("detects simple HF URL", () => {
      const result = parseModelSearchIntent(
        "https://huggingface.co/bartowski/Llama-3.2-3B"
      );
      expect(result).toEqual({
        kind: "url",
        url: "https://huggingface.co/bartowski/Llama-3.2-3B",
        repo: "bartowski/Llama-3.2-3B",
        quant: undefined,
      });
    });

    it("detects HF URL with tree/main path", () => {
      const result = parseModelSearchIntent(
        "https://huggingface.co/user/repo/tree/main"
      );
      expect(result).toEqual({
        kind: "url",
        url: "https://huggingface.co/user/repo/tree/main",
        repo: "user/repo",
        quant: undefined,
      });
    });

    it("detects HF URL with blob path and extracts quant from filename", () => {
      const result = parseModelSearchIntent(
        "https://huggingface.co/bartowski/Llama-3.2-3B-GGUF/blob/main/Llama-3.2-3B.Q4_K_M.gguf"
      );
      expect(result).toEqual({
        kind: "url",
        url: "https://huggingface.co/bartowski/Llama-3.2-3B-GGUF/blob/main/Llama-3.2-3B.Q4_K_M.gguf",
        repo: "bartowski/Llama-3.2-3B-GGUF",
        quant: "Q4_K_M",
      });
    });

    it("detects HF URL with resolve path and extracts quant", () => {
      const result = parseModelSearchIntent(
        "https://huggingface.co/user/model/resolve/main/model-Q8_0.gguf"
      );
      expect(result).toEqual({
        kind: "url",
        url: "https://huggingface.co/user/model/resolve/main/model-Q8_0.gguf",
        repo: "user/model",
        quant: "Q8_0",
      });
    });

    it("handles http:// URLs", () => {
      const result = parseModelSearchIntent(
        "http://huggingface.co/user/repo"
      );
      expect(result).toEqual({
        kind: "url",
        url: "http://huggingface.co/user/repo",
        repo: "user/repo",
        quant: undefined,
      });
    });

    it("extracts IQ quant from filename", () => {
      const result = parseModelSearchIntent(
        "https://huggingface.co/user/model/blob/main/model_IQ4_XS.gguf"
      );
      expect(result).toEqual({
        kind: "url",
        url: "https://huggingface.co/user/model/blob/main/model_IQ4_XS.gguf",
        repo: "user/model",
        quant: "IQ4_XS",
      });
    });

    it("extracts F16 from filename", () => {
      const result = parseModelSearchIntent(
        "https://huggingface.co/user/model/blob/main/model-F16.gguf"
      );
      expect(result).toEqual({
        kind: "url",
        url: "https://huggingface.co/user/model/blob/main/model-F16.gguf",
        repo: "user/model",
        quant: "F16",
      });
    });
  });

  describe("url intent - non-HuggingFace URLs (should fall back to search)", () => {
    it("treats non-HF URLs as search", () => {
      const result = parseModelSearchIntent("https://example.com/model.gguf");
      expect(result).toEqual({
        kind: "search",
        query: "https://example.com/model.gguf",
      });
    });

    it("treats GitHub URLs as search", () => {
      const result = parseModelSearchIntent(
        "https://github.com/user/repo/releases/download/v1/model.gguf"
      );
      expect(result).toEqual({
        kind: "search",
        query: "https://github.com/user/repo/releases/download/v1/model.gguf",
      });
    });

    it("treats random URLs as search", () => {
      const result = parseModelSearchIntent("https://mysite.com/files/");
      expect(result).toEqual({
        kind: "search",
        query: "https://mysite.com/files/",
      });
    });
  });
});

describe("getButtonTextForIntent", () => {
  it('returns "Search" for search intent', () => {
    const intent: ModelSearchIntent = { kind: "search", query: "llama" };
    expect(getButtonTextForIntent(intent)).toBe("Search");
  });

  it('returns "View Model" for repo intent', () => {
    const intent: ModelSearchIntent = { kind: "repo", repo: "user/model" };
    expect(getButtonTextForIntent(intent)).toBe("View Model");
  });

  it('returns "⬇️ Download" for download intent', () => {
    const intent: ModelSearchIntent = {
      kind: "download",
      repo: "user/model",
      quant: "Q4_K_M",
    };
    expect(getButtonTextForIntent(intent)).toBe("⬇️ Download");
  });

  it('returns "View Model" for url intent with repo only', () => {
    const intent: ModelSearchIntent = {
      kind: "url",
      url: "https://huggingface.co/user/model",
      repo: "user/model",
    };
    expect(getButtonTextForIntent(intent)).toBe("View Model");
  });

  it('returns "⬇️ Download" for url intent with repo and quant', () => {
    const intent: ModelSearchIntent = {
      kind: "url",
      url: "https://huggingface.co/user/model/blob/main/model.Q4_K_M.gguf",
      repo: "user/model",
      quant: "Q4_K_M",
    };
    expect(getButtonTextForIntent(intent)).toBe("⬇️ Download");
  });

  it('returns "Search" for url intent without repo', () => {
    const intent: ModelSearchIntent = {
      kind: "url",
      url: "https://huggingface.co/invalid",
    };
    expect(getButtonTextForIntent(intent)).toBe("Search");
  });
});

describe("getButtonVariantForIntent", () => {
  it('returns "default" for search intent', () => {
    const intent: ModelSearchIntent = { kind: "search", query: "llama" };
    expect(getButtonVariantForIntent(intent)).toBe("default");
  });

  it('returns "primary" for repo intent', () => {
    const intent: ModelSearchIntent = { kind: "repo", repo: "user/model" };
    expect(getButtonVariantForIntent(intent)).toBe("primary");
  });

  it('returns "accent" for download intent', () => {
    const intent: ModelSearchIntent = {
      kind: "download",
      repo: "user/model",
      quant: "Q4_K_M",
    };
    expect(getButtonVariantForIntent(intent)).toBe("accent");
  });

  it('returns "primary" for url intent with repo only', () => {
    const intent: ModelSearchIntent = {
      kind: "url",
      url: "https://huggingface.co/user/model",
      repo: "user/model",
    };
    expect(getButtonVariantForIntent(intent)).toBe("primary");
  });

  it('returns "accent" for url intent with quant', () => {
    const intent: ModelSearchIntent = {
      kind: "url",
      url: "https://huggingface.co/user/model/blob/main/model.Q4_K_M.gguf",
      repo: "user/model",
      quant: "Q4_K_M",
    };
    expect(getButtonVariantForIntent(intent)).toBe("accent");
  });
});
