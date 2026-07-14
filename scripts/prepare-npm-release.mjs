import { copyFileSync, readdirSync, readFileSync, writeFileSync } from "node:fs";

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

  copyFileSync("LICENSE", `${root}/${directory}/LICENSE`);
  const isLauncher = manifest.bin?.packager === "bin/packager.js";
  const readme = isLauncher
    ? `# Packager CLI

Install the native Packager command-line interface for macOS or Windows:

\`\`\`sh
npm install --global @what256/packager
packager --help
\`\`\`

This launcher selects the matching optional native package for the current operating system and architecture. Packager currently supports macOS and Windows on ARM64 and x64.

Project documentation and source code: https://github.com/${repository ?? "what256/packager"}
`
    : `# ${manifest.name}

${manifest.description}.

This is a platform-specific binary used by [\`@what256/packager\`](https://www.npmjs.com/package/@what256/packager). Install \`@what256/packager\` instead of depending on this package directly.

Project documentation and source code: https://github.com/${repository ?? "what256/packager"}
`;
  writeFileSync(`${root}/${directory}/README.md`, readme);
}
