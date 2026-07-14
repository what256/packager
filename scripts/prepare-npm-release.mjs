import { readdirSync, readFileSync, writeFileSync } from "node:fs";

const [version, repository] = process.argv.slice(2);
if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version ?? "")) {
  throw new Error("A valid npm version is required");
}

const root = "npm";
for (const directory of readdirSync(root)) {
  const path = `${root}/${directory}/package.json`;
  const manifest = JSON.parse(readFileSync(path, "utf8"));
  manifest.version = version;
  if (repository) {
    manifest.repository = {
      type: "git",
      url: `git+https://github.com/${repository}.git`
    };
  }
  if (manifest.optionalDependencies) {
    for (const name of Object.keys(manifest.optionalDependencies)) {
      manifest.optionalDependencies[name] = version;
    }
  }
  writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`);
}
