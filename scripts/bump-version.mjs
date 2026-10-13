/**
 * Sets one version across the three files that store it.
 *
 *   npm run bump 0.2.0
 */
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const version = process.argv[2];

if (!version || !/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error("usage: npm run bump <version>   e.g. npm run bump 0.2.0");
  process.exit(1);
}

const edits = [
  {
    file: "package.json",
    replace: (text) => text.replace(/("version":\s*")[^"]+(")/, `$1${version}$2`),
  },
  {
    file: "src-tauri/tauri.conf.json",
    replace: (text) => text.replace(/("version":\s*")[^"]+(")/, `$1${version}$2`),
  },
  {
    file: "src-tauri/Cargo.toml",
    replace: (text) => text.replace(/^(version\s*=\s*")[^"]+(")/m, `$1${version}$2`),
  },
];

for (const { file, replace } of edits) {
  const path = join(ROOT, file);
  const before = readFileSync(path, "utf8");
  const after = replace(before);
  if (before === after) {
    console.error(`could not find a version to replace in ${file}`);
    process.exit(1);
  }
  writeFileSync(path, after);
  console.log(`${file} -> ${version}`);
}
