/// <reference types="vitest/config" />
import { defineConfig } from "vitest/config";
import { loadEnv } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// https://vitejs.dev/config/
//
// defineConfig comes from vitest/config rather than vite so the `test` block
// below is type-checked. This file is covered by tsconfig.node.json; before
// that project existed it was never checked at all, and had accumulated five
// errors — including a `content` option passed to the Tailwind plugin, which
// accepts only `{ optimize }` and was discarding the array silently.
export default defineConfig(({ mode }) => {
  // Load env file based on `mode` in the current working directory.
  // Set the third parameter to '' to load all env regardless of the `VITE_` prefix.
  const env = loadEnv(mode, process.cwd(), '');
  const backendPort = env.VITE_GGLIB_WEB_PORT || '9887';

  return {
    // Tailwind v4 discovers its own sources by walking the project root and
    // skipping anything gitignored, so there is nothing to configure here.
    // Explicit control belongs in src/styles/tailwind.css via `@source`.
    plugins: [react(), tailwindcss()],

    // Build output to web_ui directory (served by gui-web command)
    build: {
      outDir: 'web_ui',
      emptyOutDir: true,
      rolldownOptions: {
        // Two entries: the main app and the tray popover. The popover is its
        // own document so it loads none of the model library or chat
        // code — opening a 360px panel should not pay for the full app shell.
        input: {
          main: 'index.html',
          tray: 'tray.html',
        },
        output: {
          manualChunks(id: string) {
            if (id.includes('@assistant-ui/react')) return 'chat-runtime';
            if (
              id.includes('react-markdown') ||
              id.includes('remark-gfm') ||
              id.includes('rehype-highlight') ||
              id.includes('highlight.js')
            ) return 'markdown';
          },
        },
      },
      chunkSizeWarningLimit: 600,
      // Skip gzipping every emitted asset just to print a size column. With the
      // font subsets this is dozens of files per build for a number nobody acts on.
      reportCompressedSize: false,
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
      // Proxy API requests to the backend server during development
      proxy: {
        '/api': {
          target: `http://localhost:${backendPort}`,
          changeOrigin: true,
          // Disable response buffering so SSE streams are forwarded immediately.
          // Without this the proxy may buffer chunks, delaying event delivery.
          configure: (proxy: { on: (event: string, cb: (res: { headers: Record<string, string | string[] | undefined> }) => void) => void }) => {
            proxy.on('proxyRes', (proxyRes) => {
              const ct = proxyRes.headers['content-type'] || '';
              if (ct.includes('text/event-stream')) {
                // Tell upstream caches / reverse-proxies not to buffer SSE
                proxyRes.headers['X-Accel-Buffering'] = 'no';
                proxyRes.headers['Cache-Control'] = 'no-cache';
              }
            });
          },
        },
      },
    },

    // Test configuration for Vitest
    test: {
      globals: true,
      environment: 'jsdom',
      setupFiles: ['./tests/ts/setup.ts'],
      include: ['tests/ts/**/*.test.{ts,tsx}'],
      coverage: {
        provider: 'v8',
        reporter: ['text', 'json', 'json-summary', 'html'],
        reportsDirectory: './coverage/ts',
        include: ['src/**/*.{ts,tsx}'],
        exclude: [
          'src/main.tsx',
          'src/vite-env.d.ts',
          'src/**/*.d.ts',
        ],
      },
    },
  };
});
