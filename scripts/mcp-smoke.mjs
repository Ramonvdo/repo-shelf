// Spawns `repo-shelf mcp` over stdio (as an agent would) and calls tools.
// Usage: node scripts/mcp-smoke.mjs <exe> [toolName jsonArgs]...
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";

const [exe, ...rest] = process.argv.slice(2);
const calls = [];
for (let i = 0; i < rest.length; i += 2) calls.push([rest[i], JSON.parse(rest[i + 1] ?? "{}")]);

const child = spawn(exe, ["mcp"], { stdio: ["pipe", "pipe", "inherit"], env: process.env });
const lines = createInterface({ input: child.stdout });
const pending = new Map();
lines.on("line", (line) => {
  const msg = JSON.parse(line);
  pending.get(msg.id)?.(msg);
});
let nextId = 1;
const request = (method, params) =>
  new Promise((resolve) => {
    const id = nextId++;
    pending.set(id, resolve);
    child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
  });

const timer = setTimeout(() => { console.error("TIMEOUT"); process.exit(2); }, 60000);
const init = await request("initialize", {
  protocolVersion: "2025-06-18",
  capabilities: {},
  clientInfo: { name: "smoke", version: "0" },
});
console.log("server:", JSON.stringify(init.result?.serverInfo), "instructions:", init.result?.instructions ?? "-");
child.stdin.write(JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" }) + "\n");
const tools = await request("tools/list", {});
console.log("tools:", tools.result.tools.map((t) => t.name).join(", "));
for (const [name, args] of calls) {
  const res = await request("tools/call", { name, arguments: args });
  const text = res.error ? JSON.stringify(res.error) : res.result.content.map((c) => c.text).join("\n");
  console.log(`\n--- ${name} ${JSON.stringify(args)}${res.result?.isError ? " [isError]" : ""}\n${text}`);
}
clearTimeout(timer);
child.kill();
