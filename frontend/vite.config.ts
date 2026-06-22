import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  // Tauri dev server: don't open the browser and disable HMR polling on macOS.
  server: {
    port: 5173,
    strictPort: true,
    open: false,
    watch: {
      // Use polling on macOS to avoid inotify issues within the Tauri dev runner.
      usePolling: false,
    },
  },
  // Ensure the app loads assets correctly from the Tauri webview.
  base: './',
  build: {
    outDir: 'dist',
  },
})
