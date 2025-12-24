import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// https://vitejs.dev/config/
export default defineConfig(async () => ({
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
    // Proxy API requests to the backend server during development
    proxy: {
      '/api': {
        target: 'http://localhost:9887',
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
}));
