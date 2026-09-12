import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const output = join(root, "web", "public", "wasm");
const artifacts = [join(output, "optimizer_wasm.js"), join(output, "optimizer_wasm_bg.wasm")];
const inputs = [join(root, "Cargo.toml"), join(root, "Cargo.lock"), join(root, "crates")];

if (artifacts.every(existsSync) && oldest(artifacts) >= newest(inputs)) {
  console.log("WASM jest aktualny.");
  process.exit(0);
}

console.log("Budowanie WASM: brak artefaktów albo zmienił się silnik Rust.");
const result = spawnSync(process.execPath, [join(root, "scripts", "build-wasm.mjs")], {
  cwd: root,
  stdio: "inherit",
});
process.exit(result.status ?? 1);

function newest(paths) {
  return Math.max(...paths.map(modifiedAt));
}

function oldest(paths) {
  return Math.min(...paths.map(modifiedAt));
}

function modifiedAt(path) {
  const stat = statSync(path);
  if (!stat.isDirectory()) return stat.mtimeMs;
  let latest = stat.mtimeMs;
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    if (entry.name === "target" || entry.name.startsWith(".")) continue;
    latest = Math.max(latest, modifiedAt(join(path, entry.name)));
  }
  return latest;
}
