import { resolve } from 'node:path';
import { readFileSync } from 'node:fs';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import { assertScreenBundle } from './scripts/screen-contracts.mjs';
import { deepLinkBridge } from './scripts/deeplink-bridge.ts';

// The matching Java engine and Rust host both exclude Part 4.
export default defineConfig(() => {
  const host = process.env.TAURI_DEV_HOST;
  const tauri = JSON.parse(readFileSync(new URL('./src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
  let outputDirectory: string;
  return {
    base: '/',
    clearScreen: false,
    plugins: [
      react(),
      deepLinkBridge(),
      {
        name: 'withlog-screen-contracts',
        apply: 'build',
        configResolved(config) {
          outputDirectory = resolve(config.root, config.build.outDir);
        },
        closeBundle() {
          assertScreenBundle(outputDirectory);
        },
      },
    ],
    resolve: {
      alias: {
        '@claude-plug': resolve(__dirname, 'src/plugs/empty'),
      },
    },
    server: {
      host: host || 'localhost',
      port: 1420,
      strictPort: true,
      hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
      watch: { ignored: ['**/src-tauri/**'] },
    },
    // Development HMR needs its own policy. Preview uses the bundle policy so
    // production-only CSP mistakes can be caught before opening the macOS app.
    preview: {
      headers: { 'Content-Security-Policy': tauri.app.security.csp },
    },
    build: {
      // The app runs inside WKWebView; do not inherit an ever-moving default.
      target: 'safari15',
      outDir: 'dist',
    },
  };
});
