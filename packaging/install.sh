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
# Before it installs anything it lists each gglib already on PATH, with its
# version, and the desktop app if one is installed, and says which gglib
# `gglib` will run afterwards. That is all it does with them: it asks
# nothing and stops for nothing.
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

# Every package puts gglib in this directory.
packaged=/usr/bin

# A directory with its links resolved, or nothing when it is not one. A
# leading // is folded to /, as some shells keep it.
real_dir() {
  d="$(CDPATH='' cd -P "$1" 2>/dev/null && pwd -P)" || d=
  while :; do
    case "$d" in //*) d="${d#/}" ;; *) break ;; esac
  done
  printf '%s\n' "$d"
}

# A file with its links resolved, as far as this system can say.
real_file() {
  r=
  if command -v realpath >/dev/null 2>&1; then r="$(realpath "$1" 2>/dev/null)" || r=; fi
  if [ -z "$r" ]; then r="$(real_dir "${1%/*}")/${1##*/}"; fi
  printf '%s\n' "$r"
}

# The version a gglib gives, or "version unknown". It is asked --version
# alone, which every release answers before it opens its library, with
# nothing on stdin. timeout kills it if it has not answered in two seconds;
# without timeout (it comes with coreutils) the copy is not run at all. The
# version is the first word that starts with a digit, if it is short and
# plain.
gglib_version() {
  out=
  if command -v timeout >/dev/null 2>&1; then
    # `|| exit 1` keeps the subshell, whose stderr is closed, as the shell
    # that waits for timeout: a shell says "Killed" of a child that was.
    out="$(
      exec 2>/dev/null
      timeout -s KILL 2 "$1" --version </dev/null || exit 1
    )" || out=
  fi
  line="${out%%
*}"
  version=
  set -f
  for word in $line; do
    case "$word" in
      [0-9]*) version="$word"; break ;;
    esac
  done
  set +f
  case "$version" in
    '' | *[!0-9A-Za-z.+_-]*) version= ;;
  esac
  if [ -n "$version" ] && [ "${#version}" -le 40 ]; then
    printf '%s\n' "$version"
  else
    echo "version unknown"
  fi
}

# What is here already, walking PATH in the order the shell does: each
# gglib (once, however many entries or links reach it) and the desktop app,
# which is never run. `ahead` is a gglib found before $packaged, which
# `gglib` will go on running.
packaged_real="$(real_dir "$packaged")"
reached=0
found=
apps=
seen=:
first=
ahead=
others=0
old_ifs=$IFS
set -f
IFS=:
for dir in ${PATH:-}; do
  IFS=$old_ifs
  set +f
  [ -n "$dir" ] || continue
  if [ -n "$packaged_real" ] && [ "$(real_dir "$dir")" = "$packaged_real" ]; then reached=1; fi
  for name in gglib gglib-gui gg-lib-gui; do
    [ -f "$dir/$name" ] && [ -x "$dir/$name" ] || continue
    r="$(real_file "$dir/$name")"
    case "$seen" in *":$r:"*) continue ;; esac
    seen="$seen$r:"
    if [ "$name" != gglib ]; then
      apps="$apps
gglib:   $dir/$name"
      continue
    fi
    found="$found
gglib:   $dir/gglib ($(gglib_version "$dir/gglib"))"
    if [ "$r" != "$packaged_real/gglib" ]; then
      others=1
      if [ -z "$first" ] && [ "$reached" = 0 ]; then ahead="$dir/gglib"; fi
    fi
    if [ -z "$first" ]; then first="$dir/gglib"; fi
  done
done
IFS=$old_ifs
set +f

if [ -n "$found" ]; then
  say "already on PATH, in the order the shell tries them:$found"
fi
if [ -n "$apps" ]; then
  say "the desktop app is already installed:$apps"
fi
if [ "$reached" = 0 ]; then
  say "$packaged is not on PATH, so the gglib this installs runs only as $packaged/gglib${first:+; \`gglib\` will still run $first}."
elif [ -n "$ahead" ]; then
  say "$ahead comes before $packaged on PATH, so \`gglib\` will still run it, not the $packaged/gglib this installs."
elif [ "$others" = 1 ]; then
  say "$packaged/gglib, which this installs, comes first on PATH, so \`gglib\` will run it, not the others listed."
fi
if [ "$others" = 1 ]; then
  say "every gglib here, and the desktop app, uses the same data folder."
fi

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
  pkg=gglib-gui
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
  as_root dnf install ${yes:+-y} gglib-gui <"$tty"
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
