import { spawn, spawnSync } from "node:child_process";
import { watch } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const run = (file, args) => spawn(process.execPath, [join(root, "scripts", file), ...args], { cwd: root, stdio: "inherit" });
const ensure = spawnSync(process.execPath, [join(root, "scripts", "ensure-wasm.mjs")], { cwd: root, stdio: "inherit" });
if (ensure.status !== 0) process.exit(ensure.status ?? 1);

const npm = process.platform === "win32" ? "npm.cmd" : "npm";
const vite = spawn(npm, ["--prefix", "web", "run", "dev"], { cwd: root, stdio: "inherit" });
let timer;
let rebuilding = false;
let rebuildAgain = false;
let activeBuild;
const pendingReasons = new Set();

const schedule = (reason) => {
  // Cargo may update the lockfile while it is already building the exact
  // dependencies it read. That is not a second source edit to compile.
  if (rebuilding && reason === "Cargo.lock") return;
  pendingReasons.add(reason);
  clearTimeout(timer);
  timer = setTimeout(rebuild, 160);
};
const watchRustSources = (path) => watch(path, { recursive: true }, (_event, filename) => {
  if (!filename || filename.endsWith(".rs")) schedule("Rust");
});
const watchers = [
  watchRustSources(join(root, "crates", "optimizer-core", "src")),
  watchRustSources(join(root, "crates", "optimizer-wasm", "src")),
  watch(join(root, "crates", "optimizer-core", "Cargo.toml"), () => schedule("optimizer-core/Cargo.toml")),
  watch(join(root, "crates", "optimizer-wasm", "Cargo.toml"), () => schedule("optimizer-wasm/Cargo.toml")),
  watch(join(root, "Cargo.toml"), () => schedule("Cargo.toml")),
  watch(join(root, "Cargo.lock"), () => schedule("Cargo.lock")),
];

function rebuild() {
  if (rebuilding) {
    rebuildAgain = true;
    return;
  }
  rebuilding = true;
  const reasons = [...pendingReasons];
  pendingReasons.clear();
  console.log(`\n[WASM] Przebudowa po zmianie: ${reasons.join(", ") || "źródła"}.`);
  activeBuild = run("build-wasm.mjs", []);
  activeBuild.on("exit", (code) => {
    activeBuild = undefined;
    rebuilding = false;
    if (code !== 0) console.error("WASM nie został przebudowany. Vite pozostaje uruchomiony z poprzednim artefaktem.");
    if (rebuildAgain) {
      rebuildAgain = false;
      rebuild();
    }
  });
}

function stop(signal) {
  for (const watcher of watchers) watcher.close();
  activeBuild?.kill(signal);
  vite.kill(signal);
}
process.on("SIGINT", () => stop("SIGINT"));
process.on("SIGTERM", () => stop("SIGTERM"));
vite.on("exit", (code) => {
  for (const watcher of watchers) watcher.close();
  process.exit(code ?? 0);
});
