import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { delimiter, dirname, join, resolve } from "node:path";
import { homedir } from "node:os";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const output = join(root, "web", "public", "wasm");
mkdirSync(output, { recursive: true });
const cargoBinary = join(homedir(), ".cargo", "bin", process.platform === "win32" ? "wasm-pack.exe" : "wasm-pack");
const wasmPack = existsSync(cargoBinary) ? cargoBinary : "wasm-pack";
const windowsGnuToolchain = join(homedir(), ".rustup", "toolchains", "stable-x86_64-pc-windows-gnu");
const rustToolchain =
  process.env.RUSTUP_TOOLCHAIN ??
  (process.platform === "win32" && existsSync(windowsGnuToolchain)
    ? "stable-x86_64-pc-windows-gnu"
    : undefined);

const result = spawnSync(
  wasmPack,
  ["build", join(root, "crates", "optimizer-wasm"), "--target", "web", "--out-dir", output, "--release"],
  {
    cwd: root,
    stdio: "inherit",
    shell: process.platform === "win32",
    env: {
      ...process.env,
      PATH: `${dirname(cargoBinary)}${delimiter}${process.env.PATH ?? ""}`,
      ...(rustToolchain ? { RUSTUP_TOOLCHAIN: rustToolchain } : {}),
    },
  },
);

if (result.error?.code === "ENOENT") {
  console.error("Nie znaleziono wasm-pack. Zainstaluj: cargo install wasm-pack");
  process.exit(1);
}
process.exit(result.status ?? 1);
