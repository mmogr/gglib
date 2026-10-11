# Rendered by scripts/render_homebrew.sh into mmogr/homebrew-gglib on each
# stable release. Edit this template in the gglib repository, not the tap.
class Gglib < Formula
  desc "Local model runtime and OpenAI-compatible proxy for llama.cpp"
  homepage "https://github.com/mmogr/gglib"
  version "@VERSION@"
  license "AGPL-3.0-only"

  on_macos do
    on_arm do
      url "https://github.com/mmogr/gglib/releases/download/v#{version}/gglib-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "@SHA256_CLI_MACOS@"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/mmogr/gglib/releases/download/v#{version}/gglib-#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "@SHA256_CLI_LINUX@"
    end
    depends_on "alsa-lib"
  end

  def install
    bin.install "gglib"
    generate_completions_from_executable(bin/"gglib", "completions")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/gglib --version")
  end
end
