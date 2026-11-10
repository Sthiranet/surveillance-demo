import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// `npm run dev` serves the UI on :5173 and proxies the API (including the SSE stream) to surveillance-director on :3000.
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: { '/api': { target: 'http://localhost:3000', changeOrigin: true } },
  },
});
