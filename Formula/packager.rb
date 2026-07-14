class Packager < Formula
  desc "Package and run self-hosted software as local apps"
  homepage "https://github.com/what256/packager"
  version "0.1.1"
  license "MIT"

  on_arm do
    url "https://github.com/what256/packager/releases/download/cli-v0.1.1/packager-cli-v0.1.1-darwin-arm64.tar.gz"
    sha256 "67cc01e1f1d80c4ec899ebad4e99f1f468c7aa6ddaba1c2df328de75c40776ed"
  end

  on_intel do
    url "https://github.com/what256/packager/releases/download/cli-v0.1.1/packager-cli-v0.1.1-darwin-x64.tar.gz"
    sha256 "28b89539bbba62f89b1796a50b87e29c03b1104575d61148455e27ea3652778d"
  end

  def install
    bin.install "packager"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/packager --version")
  end
end
