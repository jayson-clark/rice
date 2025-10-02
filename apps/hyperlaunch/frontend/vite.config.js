import { defineConfig } from 'vite';
import path from 'path';

export default defineConfig({
  root: '.',
  base: './',
  resolve: {
    alias: {
      // ensure subpath imports resolve properly
      '@tauri-apps/api/tauri': path.resolve(__dirname, 'node_modules/@tauri-apps/api/tauri.js'),
      '@tauri-apps/api/window': path.resolve(__dirname, 'node_modules/@tauri-apps/api/window.js')
    }
  },
  optimizeDeps: {
    include: [
      '@tauri-apps/api/tauri',
      '@tauri-apps/api/window'
    ]
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true
  }
});
