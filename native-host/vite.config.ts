import { resolve } from 'node:path';
import { readFileSync } from 'node:fs';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import { assertScreenBundle } from './scripts/screen-contracts.mjs';
import { deepLinkBridge } from './scripts/deeplink-bridge.ts';

// Mode switches between empty stub and Claude Part 4 plugs.
export default defineConfig(({ mode }) => {
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
        name: 'aisurada-screen-contracts',
        apply: 'build',
        configResolved(config) {
          outputDirectory = resolve(config.root, config.build.outDir);
        },
        closeBundle() {
          // 1.1: the screen is the Part 4 screen unless the mode is noclaude. The off build must carry no Part 4
          // command name; the on build must carry at least one.
          assertScreenBundle(outputDirectory, { part4: mode !== 'noclaude' });
        },
      },
    ],
    resolve: {
      alias: {
        '@claude-plug': mode === 'noclaude'
          ? resolve(__dirname, 'src/plugs/empty')
          : resolve(__dirname, 'src/plugs/claude'),
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
