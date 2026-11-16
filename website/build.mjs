import { cp, mkdir, readdir, rm, stat } from "node:fs/promises";
import path from "node:path";

const root = path.dirname(new URL(import.meta.url).pathname);
const output = path.join(root, "dist");
const excluded = new Set(["api", "node_modules", "dist", ".git", ".vercel", "package.json", "package-lock.json", "pnpm-lock.yaml", "yarn.lock", "worker.js", "wrangler.jsonc", "build.mjs", "vercel.json", "README.md"]);
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
const countFiles = async (dir) => (await Promise.all((await readdir(dir, { withFileTypes: true })).map(async (entry) => entry.isDirectory() ? countFiles(path.join(dir, entry.name)) : 1))).reduce((a, b) => a + b, 0);
console.log(`Prepared ${await countFiles(output)} static assets for Cloudflare Workers.`);
