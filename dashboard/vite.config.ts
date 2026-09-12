import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'
// import path from 'path'

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
      '@extensions': fileURLToPath(new URL('./src/extensions', import.meta.url))
    }
  },

  server: {
    // Listen on all interfaces for LAN access (optional)
    host: '0.0.0.0',
    // Proxy config
    proxy: {
      // Forward /api requests to the Rust backend
      '/api': {
        target: 'http://127.0.0.1:9002', // Rust backend port
        changeOrigin: true,
        // Do NOT add rewrite: (path) => path.replace(/^\/api/, '')
        // unless Rust routes truly omit the /api prefix
      },
      // Proxy WebSocket as well
      '/ws': {
        target: 'ws://127.0.0.1:9002',
        ws: true,
        changeOrigin: true
      }
    }
  },

  test: {
    environment: 'jsdom',
    include: ['src/**/*.{spec,test}.{ts,tsx}'],
  },

  // Build optimization
  build: {
    // Raise chunk size warning threshold (1000kb = 1mb)
    // Internal admin UI; a slightly slower first load is acceptable
    chunkSizeWarningLimit: 1000,

    // Emit source maps (.js.map in dist)
    sourcemap: false,
    // Optional: disable minification for clearer debugging
    minify: true,

    // Rollup code splitting
    rollupOptions: {
      output: {
        manualChunks(id) {
          // Split large node_modules into separate chunks
          if (id.includes('node_modules')) {
            // xterm (large terminal library)
            if (id.includes('xterm')) {
              return 'xterm';
            }
            // Vue core stack
            if (id.includes('vue') || id.includes('pinia') || id.includes('router')) {
              return 'vue-vendor';
            }
            // Icon library
            if (id.includes('lucide')) {
              return 'icons';
            }
            // Remaining third-party deps
            return 'vendor';
          }
        }
      }
    }
  }
})
