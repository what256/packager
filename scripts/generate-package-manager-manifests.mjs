import { createHash } from "node:crypto";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [repository, version, artifactsRoot, releaseTag = `v${version}`] = process.argv.slice(2);
if (!repository || !version || !artifactsRoot) {
  throw new Error("repository, version, and artifacts directory are required");
}
if (!/^[0-9A-Za-z._-]+$/.test(releaseTag)) {
  throw new Error("release tag contains unsupported characters");
}

const assets = {};
function collectArchives(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      collectArchives(path);
      continue;
    }
    if (!entry.name.endsWith(".tar.gz") && !entry.name.endsWith(".zip")) continue;
    if (assets[entry.name]) {
      throw new Error(`Duplicate release asset ${entry.name}`);
    }
    assets[entry.name] = createHash("sha256").update(readFileSync(path)).digest("hex");
  }
}
collectArchives(artifactsRoot);

const base = `https://github.com/${repository}/releases/download/${releaseTag}`;
const macArm = `packager-cli-v${version}-darwin-arm64.tar.gz`;
const macX64 = `packager-cli-v${version}-darwin-x64.tar.gz`;
const winArm = `packager-cli-v${version}-win32-arm64.zip`;
const winX64 = `packager-cli-v${version}-win32-x64.zip`;
for (const name of [macArm, macX64, winArm, winX64]) {
  if (!assets[name]) throw new Error(`Missing release asset ${name}`);
}

const formula = `class Packager < Formula
  desc "Package and run self-hosted software as local apps"
  homepage "https://github.com/${repository}"
  version "${version}"
  license "MIT"

  on_arm do
    url "${base}/${macArm}"
    sha256 "${assets[macArm]}"
  end

  on_intel do
    url "${base}/${macX64}"
    sha256 "${assets[macX64]}"
  end

  def install
    bin.install "packager"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/packager --version")
  end
end
`;
writeFileSync(join(artifactsRoot, "packager.rb"), formula);

const scoop = {
  version,
  description: "Package and run self-hosted software as local apps",
  homepage: `https://github.com/${repository}`,
  license: "MIT",
  architecture: {
    "64bit": { url: `${base}/${winX64}`, hash: assets[winX64] },
    arm64: { url: `${base}/${winArm}`, hash: assets[winArm] }
  },
  bin: "packager.exe"
};
writeFileSync(join(artifactsRoot, "packager.json"), `${JSON.stringify(scoop, null, 2)}\n`);
writeFileSync(
  join(artifactsRoot, "SHA256SUMS"),
  `${Object.entries(assets).map(([name, hash]) => `${hash}  ${name}`).sort().join("\n")}\n`
);
