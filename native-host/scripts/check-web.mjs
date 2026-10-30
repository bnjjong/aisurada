import { readFileSync } from 'node:fs';
const html = readFileSync(new URL('../web/index.html', import.meta.url), 'utf8');
if (/<style\b|\sstyle\s*=/i.test(html)) throw new Error('CSP requires an index.html without inline styles');
console.log('Native host shell verified');
