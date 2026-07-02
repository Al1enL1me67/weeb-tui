class WeebTui < Formula
  desc "TUI for anime and manga streaming"
  homepage "https://github.com/silent9669/weeb-tui"
  url "https://github.com/silent9669/weeb-tui/archive/refs/tags/v4.0.0.tar.gz"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"
  license "MIT"
  head "https://github.com/silent9669/weeb-tui.git", branch: "main"

  depends_on "pkgconf" => :build
  depends_on "rust" => :build
  depends_on "chafa"
  depends_on "mpv"

  on_linux do
    depends_on "openssl@3"
  end

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "weeb-tui", shell_output("#{bin}/weeb-tui --help")
  end
end
