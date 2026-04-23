#!/usr/bin/env bash

set -euo pipefail

VERSION=${1:?missing version}
ARM_SHA=${2:?missing arm sha}
INTEL_SHA=${3:?missing intel sha}
REPOSITORY=${4:?missing repository}

cat > Formula/serial-monitor.rb <<EOF
class SerialMonitor < Formula
  desc "Terminal UI serial monitor with filtering, search, and graphing"
  homepage "https://github.com/${REPOSITORY}"
  license "MIT"
  version "${VERSION#v}"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/${REPOSITORY}/releases/download/${VERSION}/serial-monitor-${VERSION}-macos-aarch64.tar.gz"
      sha256 "${ARM_SHA}"
    else
      url "https://github.com/${REPOSITORY}/releases/download/${VERSION}/serial-monitor-${VERSION}-macos-x86_64.tar.gz"
      sha256 "${INTEL_SHA}"
    end
  end

  head "https://github.com/${REPOSITORY}.git", branch: "main"

  head do
    depends_on "rust" => :build
  end

  def install
    if build.head?
      system "cargo", "install", *std_cargo_args(path: "crates/serial-tui")
      bin.install_symlink bin/"serial-tui" => "serial-monitor"
      doc.install "README.md", "LICENSE"
      return
    end

    odie "Stable installs are currently available only on macOS" unless OS.mac?

    bin.install "serial-monitor"
    doc.install "README.md", "LICENSE"
  end

  test do
    assert_predicate bin/"serial-monitor", :exist?
  end
end
EOF
