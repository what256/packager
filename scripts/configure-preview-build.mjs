import { readFileSync, writeFileSync } from "node:fs";

const [path = "src-tauri/tauri.conf.json"] = process.argv.slice(2);
const config = JSON.parse(readFileSync(path, "utf8"));

// Preview artifacts deliberately skip updater and operating-system code signing.
// Official tag builds keep these settings and fail when credentials are absent.
config.bundle.createUpdaterArtifacts = false;
config.bundle.macOS = {
  ...config.bundle.macOS,
  // A complete ad-hoc signature keeps the preview bundle internally valid.
  // Gatekeeper still treats it as untrusted because it has no Developer ID.
  signingIdentity: "-"
};
if (config.bundle.windows) {
  delete config.bundle.windows.certificateThumbprint;
  delete config.bundle.windows.digestAlgorithm;
  delete config.bundle.windows.timestampUrl;
}

writeFileSync(path, `${JSON.stringify(config, null, 2)}\n`);
