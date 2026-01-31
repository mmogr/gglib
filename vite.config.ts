import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// https://vitejs.dev/config/
export default defineConfig(async ({ mode }) => {
  // Load env file based on `mode` in the current working directory.
  // Set the third parameter to '' to load all env regardless of the `VITE_` prefix.
  const env = loadEnv(mode, process.cwd(), '');
  const backendPort = env.VITE_GGLIB_WEB_PORT || '9887';

  return {
  plugins: [
    react(),
    tailwindcss({
      content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
    }),
  ],

  // Build output to web_ui directory (served by gui-web command)
  build: {
    outDir: 'web_ui',
    emptyOutDir: true,
    rollupOptions: {
      output: {
        manualChunks: {
          'chat-runtime': [
            '@assistant-ui/react',
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
    // Proxy API requests to the backend server during development
    proxy: {
      '/api': {
        target: `http://localhost:${backendPort}`,
        changeOrigin: true,
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
