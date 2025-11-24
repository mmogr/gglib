import { invoke } from "@tauri-apps/api/core";

const isTauriApp =
  typeof (window as any).__TAURI_INTERNALS__ !== "undefined" ||
  typeof (window as any).__TAURI__ !== "undefined";

let cachedBase: string | null = null;
let resolvingPromise: Promise<string> | null = null;

async function resolveBase(): Promise<string> {
  if (cachedBase) {
    return cachedBase;
  }

  if (resolvingPromise) {
    return resolvingPromise;
  }

  resolvingPromise = (async () => {
    if (isTauriApp) {
      try {
        const port = await invoke<number>("get_gui_api_port");
        cachedBase = `http://localhost:${port}/api`;
      } catch (error) {
        console.error("Failed to resolve embedded API port", error);
        cachedBase = "http://localhost:8888/api";
      }
    } else {
      cachedBase = `${window.location.protocol}//${window.location.host}/api`;
    }
    return cachedBase;
  })();

  const result = await resolvingPromise;
  resolvingPromise = null;
  return result;
}

export async function getApiBase(): Promise<string> {
  return resolveBase();
}

export function invalidateApiBaseCache(): void {
  cachedBase = null;
  resolvingPromise = null;
}
