#!/usr/bin/env bash
#
# Install gglib from a site build_linux_repos.sh built, the way a user
# would, on each distro family it serves.
#
#   scripts/verify_linux_repos.sh <site-dir>
#
# Runs the site's own install.sh in a fresh Ubuntu, Arch and Fedora
# container, pointed at <site-dir> through GGLIB_REPO_URL=file:///site, so
# the installer, the key and every signature are checked together, by the
# package managers that will check them for users, before release.yml
# publishes anything. Needs docker.
#
# Ubuntu and Arch install the CLI and run it, then fetch the desktop
# package through the same signed index without its desktop libraries.
# Fedora has no CLI-only rpm, so it installs the desktop app whole, which
# also proves its libraries resolve on the current Fedora.
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <site-dir>" >&2
  exit 2
fi

SITE="$(cd "$1" && pwd)"

on() {
  local image="$1" script="$2"
  echo "::group::$image"
  docker run --rm -v "$SITE:/site:ro" -e GGLIB_REPO_URL=file:///site "$image" \
    bash -euxo pipefail -c "$script"
  echo "::endgroup::"
}

# Each run ends by running install.sh a second time, which must leave the
# repository configured once. The scripts are single-quoted to expand inside
# the container, not here.
# shellcheck disable=SC2016
on ubuntu:24.04 '
  apt-get update
  apt-get install -y --no-install-recommends curl
  sh /site/install.sh --cli
  gglib --version
  cd /tmp && apt-get download gg-lib-gui
  sh /site/install.sh --cli
  test "$(wc -l < /etc/apt/sources.list.d/gglib.list)" = 1
'

# shellcheck disable=SC2016
on archlinux:base-devel '
  pacman-key --init  # the image ships without the local key every install has
  sh /site/install.sh --cli
  gglib --version
  pacman -Sdd --noconfirm gglib-gui-bin
  test -x /usr/bin/gglib-app
  sh /site/install.sh --cli
  test "$(grep -cx "\[gglib\]" /etc/pacman.conf)" = 1
'

on fedora:latest '
  # Fedora ships an openh264 repository hosted by Cisco; the desktop
  # libraries take the stub in Fedora'"'"'s own repository instead, so this
  # check does not hang on a host it has no stake in. Weak dependencies
  # are left out for the same reason, and for the download.
  sed -i "s/^enabled=1/enabled=0/" /etc/yum.repos.d/fedora-cisco-openh264.repo
  echo install_weak_deps=False >> /etc/dnf/dnf.conf
  sh /site/install.sh
  gglib --version
  sh /site/install.sh
  rpm -q gg-lib-gui
'
