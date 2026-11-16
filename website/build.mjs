// Build a self-contained Cloudflare Worker bundle with the public CenAI site embedded.
import { cp, mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";

const root = path.dirname(new URL(import.meta.url).pathname);
const output = path.join(root, "dist");
const excluded = new Set(["ADMIN-SETUP.md", "api", "node_modules", "dist", ".git", ".vercel", "package.json", "package-lock.json", "pnpm-lock.yaml", "yarn.lock", "worker.js", "wrangler.jsonc", "build.mjs", "vercel.json", "README.md"]);
await rm(output, { recursive: true, force: true });
await mkdir(output, { recursive: true });

async function copyTree(from, to, relative = "") {
  for (const entry of await readdir(from, { withFileTypes: true })) {
    if (!relative && excluded.has(entry.name)) continue;
    if (entry.name.startsWith(".") && entry.name !== ".well-known") continue;
    const source = path.join(from, entry.name);
    const destination = path.join(to, entry.name);
    if (entry.isDirectory()) {
      await mkdir(destination, { recursive: true });
      await copyTree(source, destination, path.join(relative, entry.name));
    } else if (entry.isFile()) {
      await cp(source, destination);
    }
  }
}
await copyTree(root, output);

const assets = {};
async function collectAssets(directory, prefix = "") {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const absolute = path.join(directory, entry.name);
    const relative = path.posix.join(prefix, entry.name);
    if (entry.isDirectory()) await collectAssets(absolute, relative);
    else if (entry.isFile()) assets["/" + relative] = await readFile(absolute, "utf8");
  }
}
await collectAssets(output);

const template = await readFile(path.join(root, "worker.js"), "utf8");
const marker = "const STATIC_ASSETS = {};";
if (!template.includes(marker)) throw new Error("Worker static asset injection marker is missing");
const bundledWorker = template.replace(marker, "const STATIC_ASSETS = " + JSON.stringify(assets) + ";");
await writeFile(path.join(output, "_worker.js"), bundledWorker, "utf8");

console.log(`Prepared ${Object.keys(assets).length} public assets and a ${Buffer.byteLength(bundledWorker)}-byte standalone Worker bundle.`);
