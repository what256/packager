#!/usr/bin/env node

const { spawnSync } = require("node:child_process");
const { existsSync } = require("node:fs");
const { dirname, join } = require("node:path");

const key = `${process.platform}-${process.arch}`;
const packages = {
  "darwin-arm64": "packager-cli-darwin-arm64",
  "darwin-x64": "packager-cli-darwin-x64",
  "win32-arm64": "packager-cli-win32-arm64",
  "win32-x64": "packager-cli-win32-x64"
};

const packageName = packages[key];
if (!packageName) {
  console.error(`Packager does not publish a native CLI for ${key} yet.`);
  process.exit(1);
}

let packageJson;
try {
  packageJson = require.resolve(`${packageName}/package.json`);
} catch (error) {
  console.error(
    `The optional native package ${packageName} is missing. ` +
      "Reinstall without --no-optional, or download a standalone binary from the GitHub release."
  );
  process.exit(1);
}

const executable = join(
  dirname(packageJson),
  "bin",
  process.platform === "win32" ? "packager.exe" : "packager"
);
if (!existsSync(executable)) {
  console.error(`${packageName} is installed but its native executable is missing.`);
  process.exit(1);
}

const result = spawnSync(executable, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  console.error(`Could not start Packager: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
