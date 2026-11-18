# Rendered by scripts/render_homebrew.sh into mmogr/homebrew-tap on each
# stable release. Edit this template in the gglib repository, not the tap.
#
# The desktop app and the gglib command together: the command is the one the
# app carries in Contents/MacOS, linked onto PATH. The command alone, and
# Linux, is the gglib-cli formula.
cask "gglib" do
  version "@VERSION@"
  sha256 "@SHA256_GUI_DMG@"

  url "https://github.com/mmogr/gglib/releases/download/v#{version}/gglib-gui-#{version}-aarch64-apple-darwin.dmg"
  name "gglib"
  desc "Local model runtime for llama.cpp: the desktop app and the gglib command"
  homepage "https://github.com/mmogr/gglib"

  depends_on arch: :arm64
  depends_on macos: :big_sur

  app "gglib.app"
  binary "#{appdir}/gglib.app/Contents/MacOS/gglib"
  generate_completions_from_executable "#{appdir}/gglib.app/Contents/MacOS/gglib", "completions"

  # The app is not signed with a Developer ID, so Gatekeeper refuses a
  # quarantined copy. Preflight steps run before the app is moved, so the
  # command runs at once: here for its completions, then from PATH.
  # scripts/macos-install.command does the same by hand for the .tar.gz.
  preflight_steps do
    run "/usr/bin/xattr", args: ["-dr", "com.apple.quarantine", "{{staged_path}}/gglib.app"]
  end
end
