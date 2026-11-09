// Run JS SVGO (default preset) over every corpus icon into an output dir.
// Usage: node scripts/run_js_corpus.mjs [corpusDir] [outDir]
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { optimize } from '../reference/svgo/lib/svgo.js';

const corpus = process.argv[2] ?? 'corpus';
const out = process.argv[3] ?? 'parity-js';
mkdirSync(out, { recursive: true });
const files = readdirSync(corpus).filter((f) => f.endsWith('.svg')).sort();
for (const file of files) {
  const svg = readFileSync(join(corpus, file), 'utf-8');
  const result = optimize(svg, { path: join(corpus, file) });
  writeFileSync(join(out, file), result.data);
}
console.log(`optimized ${files.length} files`);
