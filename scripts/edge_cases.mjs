// Edge cases: run the input SVG of every upstream SVGO plugin fixture through the full default preset,
// in JS SVGO and in svgo-rs, and compare the outputs byte for byte.
// Usage: node scripts/edge_cases.mjs [path-to-svgo-rs]
// Prints one summary line and writes the mismatches, grouped by plugin, to edge-cases.txt.
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { optimize } from '../reference/svgo/lib/svgo.js';

const dir = 'reference/svgo/test/plugins';
const bin = process.argv[2] ?? join('target', 'release', process.platform === 'win32' ? 'svgo-rs.exe' : 'svgo-rs');
const files = readdirSync(dir).filter((f) => f.endsWith('.svg.txt')).sort();
let accepted = 0;
const same = [];
const differ = [];
for (const file of files) {
  // A fixture is [description, ===,] input, @@@, expected output[, @@@, plugin params].
  const text = readFileSync(join(dir, file), 'utf-8').replace(/\r\n/g, '\n');
  const body = /^={3,}$/m.test(text) ? text.split(/^={3,}$/m)[1] : text;
  const input = body.split(/\s*@@@\s*/)[0].trim();
  let js;
  try {
    js = optimize(input, { path: file }).data;
  } catch {
    continue; // JS SVGO rejects this input, so it is not an edge case for the port
  }
  accepted += 1;
  const rs = spawnSync(bin, ['-'], { input, encoding: 'utf-8', maxBuffer: 64 << 20 });
  (rs.status === 0 && rs.stdout === js ? same : differ).push(file);
}
const byPlugin = {};
for (const file of differ) (byPlugin[file.split('.')[0]] ??= []).push(file);
const lines = Object.entries(byPlugin)
  .sort((a, b) => b[1].length - a[1].length)
  .map(([plugin, list]) => `${plugin} (${list.length}): ${list.join(', ')}`);
writeFileSync('edge-cases.txt', lines.join('\n') + '\n');
console.log(`edge cases: ${same.length}/${accepted} identical to JS SVGO (${differ.length} differ; ${files.length - accepted} of ${files.length} fixture inputs skipped because JS SVGO throws on them); see edge-cases.txt`);
