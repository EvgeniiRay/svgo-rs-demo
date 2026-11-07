// Generates src/lead_data.txt for the lead-ported plugins
// (removeDeprecatedAttrs, removeUnknownsAndDefaults) from the JS
// reference collections, resolving groups exactly as the JS plugins do.
// Usage: node scripts/generate_lead_data.mjs
import { writeFileSync } from 'node:fs';
import {
  attrsGroups,
  attrsGroupsDefaults,
  attrsGroupsDeprecated,
  elems,
  elemsGroups,
  inheritableAttrs,
  presentationNonInheritableGroupAttrs,
} from '../reference/svgo/plugins/_collections.js';

const lines = [];
const csv = (it) => [...it].join(',');

// removeUnknownsAndDefaults resolved tables
for (const [name, config] of Object.entries(elems)) {
  const allowedChildren = new Set(config.content ?? []);
  for (const g of config.contentGroups ?? []) {
    for (const e of elemsGroups[g] ?? []) allowedChildren.add(e);
  }
  const allowedAttrs = new Set(config.attrs ?? []);
  const defaults = new Map(Object.entries(config.defaults ?? {}));
  for (const g of config.attrsGroups ?? []) {
    for (const a of attrsGroups[g] ?? []) allowedAttrs.add(a);
    for (const [k, v] of Object.entries(attrsGroupsDefaults[g] ?? {})) {
      defaults.set(k, v);
    }
  }
  const def = [...defaults].map(([k, v]) => `${k}\u001f${v}`).join(';');
  lines.push(
    ['E', name, csv(allowedAttrs), def, csv(allowedChildren)].join('\t'),
  );
}

// removeDeprecatedAttrs: per-element groups + own deprecated sets
for (const [name, config] of Object.entries(elems)) {
  const safe = config.deprecated?.safe ?? [];
  const unsafe = config.deprecated?.unsafe ?? [];
  lines.push(
    ['D', name, csv(config.attrsGroups ?? []), csv(safe), csv(unsafe)].join('\t'),
  );
}
for (const [group, dep] of Object.entries(attrsGroupsDeprecated)) {
  lines.push(['G', group, csv(dep.safe ?? []), csv(dep.unsafe ?? [])].join('\t'));
}

lines.push(['P', csv(attrsGroups.presentation ?? [])].join('\t'));
lines.push(['I', csv(inheritableAttrs ?? [])].join('\t'));
lines.push(['N', csv(presentationNonInheritableGroupAttrs ?? [])].join('\t'));

writeFileSync(new URL('../src/lead_data.txt', import.meta.url), lines.join('\n') + '\n');
console.log(`wrote ${lines.length} lines`);
