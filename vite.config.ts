import path from 'node:path'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [react()],
  root: 'src',
  build: {
    outDir: '../dist',
    emptyOutDir: true,
  },
  server: {
    port: 3002,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
      '@guidemode/types': path.resolve(__dirname, '../../packages/types/src'),
    },
  },
  test: {
    environment: 'jsdom',
  },
  clearScreen: false,
  envPrefix: ['VITE_', 'TAURI_'],
})
