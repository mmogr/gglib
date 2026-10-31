#!/bin/sh
#
# Install gglib from its own package repository on Debian, Ubuntu, Arch or
# Fedora, so the system's package manager keeps it up to date from then on.
#
#   curl -fsSL https://mmogr.github.io/gglib/install.sh | sh
#   curl -fsSL https://mmogr.github.io/gglib/install.sh | sh -s -- --cli
#
# It adds gglib's signing key and repository for whichever of apt, pacman or
# dnf the system has, then installs the desktop app, which brings the CLI
# with it. --cli installs the CLI alone. After that, `apt upgrade`,
# `pacman -Syu` or `dnf upgrade` updates gglib with everything else.
#
# Running it again changes nothing that is already set up. Each package
# manager's steps are the few lines in its function below, for anyone who
# would rather run them by hand.
#
# The repositories carry stable releases for x86_64. GGLIB_REPO_URL points the
# script at another copy of them; release.yml uses that to test a build.
set -eu

REPO_URL="${GGLIB_REPO_URL:-https://mmogr.github.io/gglib}"
KEY=gglib-archive-keyring

say() { printf 'gglib: %s\n' "$*" >&2; }
die() { say "$*"; exit 1; }

cli=0
for arg in "$@"; do
  case "$arg" in
    --cli) cli=1 ;;
    *) die "unknown option $arg (the only one is --cli)" ;;
  esac
done

if [ "$(id -u)" -eq 0 ]; then
  as_root() { "$@"; }
elif command -v sudo >/dev/null 2>&1; then
  as_root() { sudo "$@"; }
else
  die "run this as root, or install sudo"
fi

[ "$(uname -m)" = x86_64 ] || die "the Linux packages are built for x86_64; this machine is $(uname -m)"

# Piped into sh, the script is the shell's stdin, so the package managers
# read their prompts from the terminal instead, and you see and confirm what
# they will do. With no terminal (a container, CI) they go ahead unasked.
if (: </dev/tty) 2>/dev/null; then
  tty=/dev/tty
  yes=
else
  tty=/dev/null
  yes=1
fi

fetch() { curl -fsSL "$REPO_URL/$1"; }

with_apt() {
  fetch "$KEY.gpg" | as_root tee "/usr/share/keyrings/$KEY.gpg" >/dev/null
  echo "deb [signed-by=/usr/share/keyrings/$KEY.gpg] $REPO_URL/apt stable main" \
    | as_root tee /etc/apt/sources.list.d/gglib.list >/dev/null
  as_root apt-get update
  pkg=gg-lib-gui
  if [ "$cli" = 1 ]; then pkg=gglib; fi
  as_root apt-get install ${yes:+-y} "$pkg" <"$tty"
}

with_pacman() {
  key="$(mktemp)"
  fetch "$KEY.asc" >"$key"
  fpr="$(gpg --batch --with-colons --import-options show-only --import "$key" | awk -F: '$1 == "fpr" { print $10; exit }')"
  [ -n "$fpr" ] || die "could not read the signing key from $REPO_URL/$KEY.asc"
  as_root pacman-key --add "$key"
  rm -f "$key"
  as_root pacman-key --lsign-key "$fpr"
  if ! grep -qx '\[gglib\]' /etc/pacman.conf; then
    printf '\n[gglib]\nSigLevel = Required DatabaseRequired\nServer = %s/arch\n' "$REPO_URL" \
      | as_root tee -a /etc/pacman.conf >/dev/null
  fi
  # The AUR's package names: gglib-bin provides gglib, gglib-gui-bin
  # gglib-gui. -Syu rather than -Sy: Arch does not support partial
  # upgrades, and the desktop app's libraries come from the same sync as the
  # rest of the system.
  pkg=gglib-gui-bin
  if [ "$cli" = 1 ]; then pkg=gglib-bin; fi
  as_root pacman -Syu --needed ${yes:+--noconfirm} "$pkg" <"$tty"
}

with_dnf() {
  # The rpms are the release's own, unsigned; the signed repomd.xml lists each
  # one's sha256, which is what dnf checks them against.
  printf '[gglib]\nname=gglib\nbaseurl=%s/rpm\nenabled=1\ngpgcheck=0\nrepo_gpgcheck=1\ngpgkey=%s/%s.asc\n' \
    "$REPO_URL" "$REPO_URL" "$KEY" | as_root tee /etc/yum.repos.d/gglib.repo >/dev/null
  if [ "$cli" = 1 ]; then
    say "there is no CLI-only rpm; installing the desktop app, which carries the CLI"
  fi
  as_root dnf install ${yes:+-y} gg-lib-gui <"$tty"
}

if command -v apt-get >/dev/null 2>&1; then
  with_apt
  upgrade="sudo apt update && sudo apt upgrade"
elif command -v pacman >/dev/null 2>&1; then
  with_pacman
  upgrade="sudo pacman -Syu"
elif command -v dnf >/dev/null 2>&1; then
  with_dnf
  upgrade="sudo dnf upgrade"
else
  die "found none of apt-get, pacman or dnf; https://github.com/mmogr/gglib#install lists the other ways to install"
fi

say "installed. \`$upgrade\` keeps it up to date."
