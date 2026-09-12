import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { delimiter, dirname, join, resolve } from "node:path";
import { homedir } from "node:os";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const output = join(root, "web", "public", "wasm");
mkdirSync(output, { recursive: true });
const binaryExtension = process.platform === "win32" ? ".exe" : "";
const cargoDirectory = join(homedir(), ".cargo", "bin");
const wasmPackBinary = join(cargoDirectory, `wasm-pack${binaryExtension}`);
const wasmPack = existsSync(wasmPackBinary) ? wasmPackBinary : "wasm-pack";
const cargo = join(cargoDirectory, `cargo${binaryExtension}`);
const wasmBindgen = join(cargoDirectory, `wasm-bindgen${binaryExtension}`);
const windowsGnuToolchain = join(homedir(), ".rustup", "toolchains", "stable-x86_64-pc-windows-gnu");
const rustToolchain =
  process.env.RUSTUP_TOOLCHAIN ??
  (process.platform === "win32" && existsSync(windowsGnuToolchain)
    ? "stable-x86_64-pc-windows-gnu"
    : undefined);

const env = {
  ...process.env,
  PATH: `${cargoDirectory}${delimiter}${process.env.PATH ?? ""}`,
  ...(rustToolchain ? { RUSTUP_TOOLCHAIN: rustToolchain } : {}),
};
const run = (command, args) => spawnSync(command, args, {
  cwd: root,
  stdio: "inherit",
  shell: process.platform === "win32",
  env,
});

let result;
if (existsSync(cargo) && existsSync(wasmBindgen)) {
  console.log("Budowanie WASM przez cargo + wasm-bindgen…");
  result = run(cargo, ["build", "--manifest-path", join(root, "crates", "optimizer-wasm", "Cargo.toml"), "--target", "wasm32-unknown-unknown", "--release"]);
  if (result.status === 0) {
    const module = join(root, "target", "wasm32-unknown-unknown", "release", "optimizer_wasm.wasm");
    result = run(wasmBindgen, [module, "--target", "web", "--out-dir", output, "--out-name", "optimizer_wasm"]);
  }
} else {
  console.warn("Nie znaleziono wasm-bindgen-cli; używam fallbacku wasm-pack.");
  result = run(wasmPack, ["build", join(root, "crates", "optimizer-wasm"), "--target", "web", "--out-dir", output, "--release"]);
}

if (result.error?.code === "ENOENT") {
  console.error("Nie znaleziono wasm-bindgen-cli ani wasm-pack. Zainstaluj: cargo install wasm-bindgen-cli --version 0.2.128 --locked");
  process.exit(1);
}
process.exit(result.status ?? 1);
