import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// UI is served from the same origin as the API in production (SPEC-001 UI notes);
// in dev, requests are proxied so cookies and EventSource work without CORS.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://localhost:8080',
      '/healthz': 'http://localhost:8080',
    },
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/setupTests.ts'],
    globals: true,
  },
});
