import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  // Build output to web_ui directory (served by gui-web command)
  build: {
    outDir: 'web_ui',
    emptyOutDir: true,
    rollupOptions: {
      output: {
        manualChunks: {
          'chat-runtime': [
            '@assistant-ui/react',
            '@assistant-ui/react-ai-sdk',
            '@ai-sdk/openai',
            'ai',
          ],
          markdown: [
            'react-markdown',
            'remark-gfm',
            'rehype-highlight',
            'highlight.js',
          ],
        },
      },
    },
    chunkSizeWarningLimit: 1024,
  },

  // Use relative paths for assets so they work in both dev and production
  base: './',

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent vite from obscuring rust errors
  clearScreen: false,
  // 2. Tauri expects a fixed port, fail if that port is not available
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      // 3. tell vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
