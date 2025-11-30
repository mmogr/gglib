import { ModelsDirectoryInfo, AppSettings, UpdateSettingsRequest, SystemMemoryInfo } from "../types";
import { getApiBase } from "../utils/apiBase";

interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

async function apiFetch(path: string, init?: RequestInit): Promise<Response> {
  const apiBase = await getApiBase();
  return fetch(`${apiBase}${path}`, init);
}

export async function fetchModelsDirectoryInfo(): Promise<ModelsDirectoryInfo> {
  const response = await apiFetch(`/settings/models-directory`);
  const payload: ApiResponse<ModelsDirectoryInfo> = await response.json();

  if (!response.ok || !payload.success || !payload.data) {
    const message = payload?.error || response.statusText || "Failed to load models directory";
    throw new Error(message);
  }

  return payload.data;
}

export async function updateModelsDirectory(path: string): Promise<ModelsDirectoryInfo> {
  const response = await apiFetch(`/settings/models-directory`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ path }),
  });

  let payload: ApiResponse<ModelsDirectoryInfo> | null = null;
  try {
    payload = await response.json();
  } catch (error) {
    throw new Error(`Failed to parse settings response: ${error}`);
  }

  if (!response.ok || !payload?.success || !payload.data) {
    const message = payload?.error || response.statusText || "Failed to update models directory";
    throw new Error(message);
  }

  return payload.data;
}

export async function fetchSettings(): Promise<AppSettings> {
  const response = await apiFetch(`/settings`);
  const payload: ApiResponse<AppSettings> = await response.json();

  if (!response.ok || !payload.success || !payload.data) {
    const message = payload?.error || response.statusText || "Failed to load settings";
    throw new Error(message);
  }

  return payload.data;
}

export async function updateSettings(settings: UpdateSettingsRequest): Promise<AppSettings> {
  const response = await apiFetch(`/settings`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(settings),
  });

  let payload: ApiResponse<AppSettings> | null = null;
  try {
    payload = await response.json();
  } catch (error) {
    throw new Error(`Failed to parse settings response: ${error}`);
  }

  if (!response.ok || !payload?.success || !payload.data) {
    const message = payload?.error || response.statusText || "Failed to update settings";
    throw new Error(message);
  }

  return payload.data;
}

/**
 * Fetch system memory information for "Will it fit?" calculations.
 * Returns RAM and GPU memory info for the current system.
 */
export async function fetchSystemMemoryInfo(): Promise<SystemMemoryInfo> {
  const response = await apiFetch(`/system/memory`);
  const payload: ApiResponse<SystemMemoryInfo> = await response.json();

  if (!response.ok || !payload.success || !payload.data) {
    const message = payload?.error || response.statusText || "Failed to load system memory info";
    throw new Error(message);
  }

  return payload.data;
}
