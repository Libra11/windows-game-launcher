import { defineConfig } from 'vite';

export default defineConfig({
  clearScreen: false,
  build: { rollupOptions: { input: { main: 'index.html', achievementOverlay: 'achievement-overlay.html' } } },
  server: { port: 1420, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } }
});
