/**
 * Native file dialog utilities
 * TRANSPORT_EXCEPTION: Uses Tauri's native file picker dialog.
 * UI components should import from 'services/platform' rather than checking isTauriApp directly.
 */

import { isDesktop } from './detect';

export interface FilePickerResult {
  path: string | null;
  cancelled: boolean;
}

/**
 * Open a native file picker dialog to select a GGUF file.
 * On web, returns a promise that resolves when user selects via HTML input.
 * On desktop, uses Tauri's native dialog.
 */
export async function pickGgufFile(): Promise<FilePickerResult> {
  if (isDesktop()) {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const result = await open({
      title: 'Select GGUF Model',
      filters: [{ name: 'GGUF Models', extensions: ['gguf'] }],
      multiple: false,
    });
    
    if (result === null) {
      return { path: null, cancelled: true };
    }
    
    return { path: result as string, cancelled: false };
  }
  
  // Web fallback: HTML file input (path is filename only, actual upload needed separately)
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.gguf';
    
    input.onchange = (e) => {
      const file = (e.target as HTMLInputElement).files?.[0];
      if (file) {
        resolve({ path: file.name, cancelled: false });
      } else {
        resolve({ path: null, cancelled: true });
      }
    };
    
    input.oncancel = () => {
      resolve({ path: null, cancelled: true });
    };
    
    input.click();
  });
}
