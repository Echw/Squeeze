import { spawn, spawnSync } from "node:child_process";
import { readdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const run = (file, args) => spawn(process.execPath, [join(root, "scripts", file), ...args], { cwd: root, stdio: "inherit" });
const ensure = spawnSync(process.execPath, [join(root, "scripts", "ensure-wasm.mjs")], { cwd: root, stdio: "inherit" });
if (ensure.status !== 0) process.exit(ensure.status ?? 1);

const npmArgs = ["--prefix", "web", "run", "dev"];
const npmCli = process.env.npm_execpath ?? join(dirname(process.execPath), "node_modules", "npm", "bin", "npm-cli.js");
const vite = process.platform === "win32"
  ? spawn(process.execPath, [npmCli, ...npmArgs], { cwd: root, stdio: "inherit" })
  : spawn("npm", npmArgs, { cwd: root, stdio: "inherit" });
let timer;
let rebuilding = false;
let rebuildAgain = false;
let activeBuild;
const pendingReasons = new Set();
const watchedInputs = [
  join(root, "crates", "optimizer-core", "src"),
  join(root, "crates", "optimizer-wasm", "src"),
  join(root, "crates", "optimizer-core", "Cargo.toml"),
  join(root, "crates", "optimizer-wasm", "Cargo.toml"),
  join(root, "Cargo.toml"),
  join(root, "Cargo.lock"),
];
let observedMtime = newest(watchedInputs);

const schedule = (reason) => {
  // Cargo may update the lockfile while it is already building the exact
  // dependencies it read. That is not a second source edit to compile.
  if (rebuilding && reason === "Cargo.lock") return;
  pendingReasons.add(reason);
  clearTimeout(timer);
  timer = setTimeout(rebuild, 160);
};
const poller = setInterval(() => {
  const nextMtime = newest(watchedInputs);
  if (nextMtime <= observedMtime) return;
  observedMtime = nextMtime;
  schedule("źródła Rust/WASM");
}, 750);

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
  clearInterval(poller);
  activeBuild?.kill(signal);
  vite.kill(signal);
}
process.on("SIGINT", () => stop("SIGINT"));
process.on("SIGTERM", () => stop("SIGTERM"));
vite.on("exit", (code) => {
  clearInterval(poller);
  process.exit(code ?? 0);
});

function newest(paths) {
  return Math.max(...paths.map(modifiedAt));
}

function modifiedAt(path) {
  const stat = statSync(path);
  if (!stat.isDirectory()) return stat.mtimeMs;
  let latest = stat.mtimeMs;
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    if (entry.name.startsWith(".")) continue;
    latest = Math.max(latest, modifiedAt(join(path, entry.name)));
  }
  return latest;
}
