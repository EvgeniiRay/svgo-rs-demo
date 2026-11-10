#!/usr/bin/env node
// Audits the docs site: Lighthouse on every page (served from the folder), plus a check of
// every local link and image. Writes <out>/audit.md (score table and failed checks) and
// <out>/scores.json, and exits 1 when a page is under the bar or a local link is broken.
// Usage: node scripts/ci/site-audit.mjs <site-dir> <out-dir>
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";

const [site = "site", out = "audit-out"] = process.argv.slice(2);
const BAR = { performance: 0.9, accessibility: 0.95, "best-practices": 0.9, seo: 0.95 };
const NAMES = { performance: "Performance", accessibility: "Accessibility", "best-practices": "Best practices", seo: "SEO" };
mkdirSync(out, { recursive: true });

const pages = readdirSync(site).filter((f) => f.endsWith(".html")).sort();

// Local links and images that point at a missing file.
const broken = [];
for (const page of pages) {
  const html = readFileSync(path.join(site, page), "utf8");
  for (const m of html.matchAll(/(?:href|src)="([^"#?]+)[^"]*"/g)) {
    const ref = m[1];
    if (/^(https?:|mailto:|data:|\/\/)/.test(ref)) continue;
    if (!existsSync(path.join(site, ref))) broken.push(`${page} → ${ref}`);
  }
}

// Lighthouse, desktop preset, one run per page, through Lighthouse CI's static server.
rmSync(".lighthouseci", { recursive: true, force: true });
let lhci = ["npx", "-y", "@lhci/cli@0.14.0"];
try { execFileSync("lhci", ["--version"], { stdio: "ignore" }); lhci = ["lhci"]; } catch {}
execFileSync(lhci[0], [...lhci.slice(1), "collect", `--staticDistDir=${site}`, "--numberOfRuns=1",
  ...pages.map((p) => `--url=http://localhost/${p}`),
  "--settings.preset=desktop", "--settings.chromeFlags=--headless=new --no-sandbox"], { stdio: ["ignore", "inherit", "inherit"] });

const results = [];
for (const f of readdirSync(".lighthouseci").filter((f) => f.startsWith("lhr-") && f.endsWith(".json"))) {
  const lhr = JSON.parse(readFileSync(path.join(".lighthouseci", f), "utf8"));
  const page = new URL(lhr.finalDisplayedUrl || lhr.finalUrl).pathname.replace(/^\//, "") || "index.html";
  const scores = Object.fromEntries(Object.keys(BAR).map((k) => [k, lhr.categories[k].score]));
  const failed = [];
  for (const k of Object.keys(BAR)) {
    for (const ref of lhr.categories[k].auditRefs) {
      const a = lhr.audits[ref.id];
      if (!a || ref.weight === 0 || a.score === null || a.score >= 0.9) continue;
      if (!["binary", "numeric", "metricSavings"].includes(a.scoreDisplayMode)) continue;
      const items = (a.details?.items || []).slice(0, 5).map((i) => i.node?.snippet || i.url || i.source?.url || "").filter(Boolean);
      failed.push({ category: k, id: a.id, title: a.title, displayValue: a.displayValue || "", items });
    }
  }
  results.push({ page, scores, failed });
}
results.sort((a, b) => a.page.localeCompare(b.page));

const pct = (s) => Math.round(s * 100);
const under = results.some((r) => Object.entries(BAR).some(([k, bar]) => r.scores[k] < bar));
const lines = [];
lines.push("| Page | " + Object.values(NAMES).join(" | ") + " |");
lines.push("| --- |" + " ---: |".repeat(4));
for (const r of results) lines.push(`| ${r.page} | ` + Object.keys(BAR).map((k) => (r.scores[k] < BAR[k] ? `**${pct(r.scores[k])}**` : `${pct(r.scores[k])}`)).join(" | ") + " |");
lines.push("");
lines.push(`Bar: Performance ${pct(BAR.performance)}, Accessibility ${pct(BAR.accessibility)}, Best practices ${pct(BAR["best-practices"])}, SEO ${pct(BAR.seo)}. Broken local links: ${broken.length}.`);
const failedLines = [];
for (const r of results) for (const f of r.failed) failedLines.push(`- ${r.page}, ${NAMES[f.category]}: ${f.title}${f.displayValue ? ` (${f.displayValue})` : ""}${f.items.length ? `\n  - ${f.items.join("\n  - ")}` : ""}`);
for (const b of broken) failedLines.push(`- Broken link: ${b}`);
writeFileSync(path.join(out, "scores.md"), lines.join("\n") + "\n");
writeFileSync(path.join(out, "audit.md"), lines.join("\n") + "\n\n### Failed checks\n\n" + (failedLines.join("\n") || "None.") + "\n");
writeFileSync(path.join(out, "scores.json"), JSON.stringify({ pass: !under && broken.length === 0, broken, results }, null, 2));
console.log(lines.join("\n"));
process.exit(!under && broken.length === 0 ? 0 : 1);
