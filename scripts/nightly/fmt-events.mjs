// Reads `muse exec --json` events on stdin and prints one readable line per step.
import readline from "node:readline";
const dim = (s) => `\x1b[90m${s}\x1b[0m`, bold = (s) => `\x1b[1m${s}\x1b[0m`, green = (s) => `\x1b[32m${s}\x1b[0m`;
const t0 = Date.now();
const clock = () => { const s = Math.round((Date.now() - t0) / 1000); return dim(`${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`); };
const tasks = new Map();
let lastTodos = "";
for await (const line of readline.createInterface({ input: process.stdin })) {
  let d; try { d = JSON.parse(line); } catch { continue; }
  const p = d.payload ?? {}, e = p.event ?? {};
  switch (d.payload_type) {
    case "task.lifecycle.proposed":
      tasks.set(e.task_id, e.task_kind); break;
    case "task.lifecycle.output": {
      const kind = tasks.get(e.task_id) ?? "";
      if (!kind.startsWith("tool.") || kind === "tool.write_todos") break;
      let c; try { c = JSON.parse(e.chunk); } catch { c = null; }
      const what = c?.description ?? c?.command ?? c?.path ?? "";
      if (what) console.log(`${clock()}  ${kind.slice(5).padEnd(12)} ${String(what).split("\n")[0].slice(0, 110)}`);
      break;
    }
    case "todo.snapshot.updated": {
      const items = p.facts?.items ?? [];
      const s = items.map((i) => `${i.status === "completed" ? green("✓") : i.status === "in_progress" ? "▸" : dim("·")} ${i.text}`).join("\n           ");
      if (s !== lastTodos) { console.log(`${clock()}  ${bold("plan")}\n           ${s}`); lastTodos = s; }
      break;
    }
    case "run.usage": {
      const m = (p.models ?? [])[0] ?? {};
      console.log(`${clock()}  ${bold("usage")}      ${(m.input_tokens ?? 0).toLocaleString()} in · ${(m.output_tokens ?? 0).toLocaleString()} out · ${p.calls ?? 0} model calls`);
      break;
    }
    case "run.terminal.completed":
      console.log(`${clock()}  ${bold(p.terminal ?? "done")}`); if (p.text) console.log(p.text); break;
  }
}
