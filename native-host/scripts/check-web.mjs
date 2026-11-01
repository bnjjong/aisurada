import { fileURLToPath } from 'node:url';
import { assertScreenBundle } from './screen-contracts.mjs';

const directory = process.argv[2] || fileURLToPath(new URL('../dist', import.meta.url));
console.log('Screen bundle verified:', assertScreenBundle(directory));
