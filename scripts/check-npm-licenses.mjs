import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const allowed = new Set(["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib"]);
const workspaceRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const root = join(workspaceRoot, "web", "node_modules");
const packages = new Map();

function visit(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (!entry.isDirectory() || entry.name === ".bin") continue;
    const path = join(directory, entry.name);
    if (entry.name.startsWith("@")) {
      visit(path);
      continue;
    }
    const manifestPath = join(path, "package.json");
    if (existsSync(manifestPath)) {
      const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
      packages.set(`${manifest.name}@${manifest.version}`, String(manifest.license ?? "UNKNOWN"));
    }
    const nested = join(path, "node_modules");
    if (existsSync(nested)) visit(nested);
  }
}

function isAllowed(expression) {
  const identifiers = expression.match(/[A-Za-z]+(?:-[A-Za-z0-9.]+)*/g) ?? [];
  return identifiers.some((identifier) => allowed.has(identifier)) &&
    !identifiers.some((identifier) => ["GPL", "AGPL", "LGPL", "Commercial", "Proprietary"].some((bad) => identifier.includes(bad)));
}

if (!existsSync(root)) {
  console.error("Brak web/node_modules; najpierw uruchom npm --prefix web ci.");
  process.exit(1);
}
visit(root);
const rejected = [...packages].filter(([, license]) => !isAllowed(license));
if (rejected.length) {
  for (const [name, license] of rejected) console.error(`${name}: ${license}`);
  console.error(`Odrzucono ${rejected.length} zależności spoza allowlisty.`);
  process.exit(1);
}
console.log(`Licencje npm: ${packages.size} pakietów zgodnych z allowlistą.`);
