import { readdirSync, readFileSync } from "node:fs";

const expected = process.argv[2] ?? JSON.parse(readFileSync("package.json", "utf8")).version;
const versions = new Map();
versions.set("package.json", JSON.parse(readFileSync("package.json", "utf8")).version);
versions.set(
  "src-tauri/tauri.conf.json",
  JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")).version
);
const cargo = readFileSync("Cargo.toml", "utf8").match(
  /\[workspace\.package\][\s\S]*?version\s*=\s*"([^"]+)"/
);
if (!cargo) throw new Error("Cannot find workspace package version");
versions.set("Cargo.toml", cargo[1]);
for (const directory of readdirSync("npm")) {
  const path = `npm/${directory}/package.json`;
  versions.set(path, JSON.parse(readFileSync(path, "utf8")).version);
}
const mismatches = [...versions].filter(([, version]) => version !== expected);
if (mismatches.length) {
  throw new Error(
    `Expected version ${expected}: ${mismatches
      .map(([path, version]) => `${path}=${version}`)
      .join(", ")}`
  );
}
console.log(`All source versions match ${expected}.`);
