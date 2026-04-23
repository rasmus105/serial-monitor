class SerialMonitor < Formula
  desc "Terminal UI serial monitor with filtering, search, and graphing"
  homepage "https://github.com/rasmus105/serial-monitor"
  license "MIT"
  head "https://github.com/rasmus105/serial-monitor.git", branch: "main"

  depends_on "rust" => :build

  def install
    odie "This formula currently supports only --HEAD installs" unless build.head?

    system "cargo", "install", *std_cargo_args(path: "crates/serial-tui")
    bin.install_symlink bin/"serial-tui" => "serial-monitor"
    doc.install "README.md", "LICENSE"
  end

  test do
    assert_predicate bin/"serial-monitor", :exist?
  end
end
