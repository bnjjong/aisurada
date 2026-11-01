import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const root = path.resolve(__dirname, '..');
const nodeModules = path.join(root, 'node_modules');

const packages = [
  { name: 'state', ext: 'ts' },
  { name: 'ask', ext: 'tsx' },
  { name: 'session', ext: 'tsx' },
  { name: 'projects', ext: 'tsx' },
  { name: 'terminal', ext: 'ts' },
];

for (const pkg of packages) {
  const dir = path.join(nodeModules, '@claude-plug', pkg.name);
  fs.mkdirSync(dir, { recursive: true });
  const pkgJson = path.join(dir, 'package.json');
  fs.writeFileSync(
    pkgJson,
    JSON.stringify(
      {
        name: `@claude-plug/${pkg.name}`,
        version: '0.1.0',
        type: 'module',
        main: `../../../src/plugs/empty/${pkg.name}.${pkg.ext}`,
      },
      null,
      2
    ) + '\n'
  );
}
