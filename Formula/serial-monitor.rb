class SerialMonitor < Formula
  desc "Terminal UI serial monitor with filtering, search, and graphing"
  homepage "https://github.com/rasmus105/serial-monitor"
  license "MIT"
  version "0.1.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/rasmus105/serial-monitor/releases/download/v0.1.0/serial-monitor-v0.1.0-macos-aarch64.tar.gz"
      sha256 "6a16e4dc8a2c0290a1b75d393f22f924ab74cfaaba5cf1019f15edc82eb9c58a"
    end
  end

  head "https://github.com/rasmus105/serial-monitor.git", branch: "main"

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
