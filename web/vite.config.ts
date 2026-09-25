import { defineConfig } from "vite";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const wasmGluePath = fileURLToPath(new URL("./public/wasm/optimizer_wasm.js", import.meta.url));
const wasmGluePlugin = () => ({
  name: "optimizer-wasm-glue",
  resolveId(id: string) { return id === "virtual:optimizer-wasm-glue" ? id : undefined; },
  load(id: string) { return id === "virtual:optimizer-wasm-glue" ? readFileSync(wasmGluePath, "utf8") : undefined; },
});

export default defineConfig({
  plugins: [wasmGluePlugin()],
  worker: {
    format: "es",
    plugins: () => [wasmGluePlugin()],
  },
  build: {
    target: "es2022",
    sourcemap: true,
    rollupOptions: { output: { format: "iife" } },
  },
  server: {
    headers: {
      "Cross-Origin-Opener-Policy": "same-origin",
      "Cross-Origin-Embedder-Policy": "require-corp",
    },
  },
  preview: {
    headers: {
      "Cross-Origin-Opener-Policy": "same-origin",
      "Cross-Origin-Embedder-Policy": "require-corp",
    },
  },
});
