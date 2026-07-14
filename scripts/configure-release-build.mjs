import { readFileSync, writeFileSync } from "node:fs";

const [repository, thumbprint = ""] = process.argv.slice(2);
if (!repository) throw new Error("repository is required");

const path = "src-tauri/tauri.conf.json";
const config = JSON.parse(readFileSync(path, "utf8"));
config.plugins.updater.endpoints = [
  `https://github.com/${repository}/releases/latest/download/latest.json`
];
if (thumbprint) {
  config.bundle.windows = {
    certificateThumbprint: thumbprint,
    digestAlgorithm: "sha256",
    timestampUrl: "http://timestamp.digicert.com",
    webviewInstallMode: { type: "downloadBootstrapper" }
  };
}
writeFileSync(path, `${JSON.stringify(config, null, 2)}\n`);
