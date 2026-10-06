import { readFile, readdir, stat, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const dist = resolve(dirname(fileURLToPath(import.meta.url)), "../web/dist");
const assets = (await readdir(join(dist, "assets")))
  .filter((name) => /\.(?:js|css|wasm)$/.test(name))
  .map((name) => `/assets/${name}`);
const wasmAssets = [
  "/wasm/jpegli.wasm",
  "/wasm/optimizer_wasm_bg.wasm",
];

const workerAsset = assets.find((asset) => /\/optimizer\.worker-[^/]+\.js$/.test(asset));
if (!workerAsset) throw new Error("Offline manifest is missing the compression worker");
const workerSource = await readFile(join(dist, workerAsset.slice(1)), "utf8");
if (/\bimport\s*\(/.test(workerSource)) throw new Error("Compression worker still loads a module dynamically");
for (const asset of wasmAssets) await stat(join(dist, asset.slice(1)));

const paths = [...assets, ...wasmAssets].sort();
await writeFile(join(dist, "precache-manifest.json"), JSON.stringify(paths));

const indexPath = join(dist, "index.html");
const html = await readFile(indexPath, "utf8");
// The single-entry IIFE uses a classic script so its cached copy runs offline.
const classicHtml = html.replace(/<script type="module" crossorigin src="(\/assets\/index-[^"]+\.js)"><\/script>/, '<script defer src="$1"></script>');
if (classicHtml === html) throw new Error("Production HTML is missing its bundled entry script");
await writeFile(indexPath, classicHtml);
