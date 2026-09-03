#!/usr/bin/env node
import { existsSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = dirname(dirname(fileURLToPath(import.meta.url)));
const suffix = process.platform === "win32" ? ".exe" : "";
const platformBinary = join(packageRoot, "native", `${process.platform}-${process.arch}`, `javora-core${suffix}`);
const localBinary = join(packageRoot, "target", "release", `javora-core${suffix}`);
const binary = existsSync(platformBinary) ? platformBinary : localBinary;

if (!existsSync(binary)) {
  console.error("Javora core binary is not installed. Build it with: npm run build:core");
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit", cwd: process.cwd() });
process.exit(result.status ?? 1);
