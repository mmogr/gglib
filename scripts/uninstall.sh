#!/usr/bin/env bash

# What `make uninstall` runs.
#
# Three questions, each with No as the default, all asked before anything is
# deleted:
#
#   1. the program: the installed binary and this checkout's build output
#   2. gglib's data: the data folder (library database, settings, device
#      keys, logs, the llama.cpp and sd-server installs), what an older build
#      left in this checkout, and the desktop app with what it keeps
#   3. the downloaded models, which sit outside the data folder and are large
#
# The data folder and the models directory are whatever `gglib config paths`
# says, asked of the binary before it is removed. With no binary to ask, the
# data folder is the platform default (or `GGLIB_DATA_DIR`) and the models
# are left alone.
#
# Only the default data folder is deleted whole, and only when it is a real
# folder rather than a link. One named another way (`GGLIB_DATA_DIR`, or a
# checkout an older binary still calls its data folder) loses only the names
# gglib writes there, as this checkout does. A data folder that is the home
# folder or a folder above it or above this checkout is left alone and named,
# and so is a models directory that is, or is above, the home folder, this
# checkout or anything listed as data. The models directory is otherwise
# taken at its word: it is deleted whole on a typed y, whatever is in it.
#
# A packaged install (Homebrew, a .deb, the AUR) is its package manager's to
# remove; this says so where it finds one and does not touch it.

set -u

# A path with its symlinks and `..` resolved, or nothing when it is not there.
canon() {
    if [ -d "$1" ]; then
        (cd "$1" 2>/dev/null && pwd -P)
    elif [ -e "$1" ]; then
        printf '%s/%s\n' "$(cd "$(dirname "$1")" 2>/dev/null && pwd -P)" "$(basename "$1")"
    fi
}

# Whether the first folder is the second or holds it. Asked of the
# filesystem, one parent at a time, so a path that differs only in case on a
# case-insensitive disk is still the same folder.
holds() {
    local path="$2"
    while :; do
        if [ "$1" -ef "$path" ]; then return 0; fi
        if [ "$path" = "/" ] || [ -z "$path" ]; then return 1; fi
        path="$(dirname "$path")"
    done
}

REPO="$(canon "$(dirname "${BASH_SOURCE[0]}")/..")"
HOME_DIR="$(canon "$HOME")"
INSTALLED_BIN="$HOME/.cargo/bin/gglib"
APP_NAME="GGLib GUI"
# The bundle identifier, then the names an unbundled build of the desktop app
# has run under; the OS keys a webview's cache by either.
APP_IDS="com.gglib.gui gglib-app gglib-gui gglib-tauri"
# What gglib writes at the top of its data folder, and so what a build from
# before every build shared one folder left at the top of its checkout.
STATE_NAMES="data .llama .sd .python .gglib-runtime .conda logs pids daemon.lock"

if [ -z "$REPO" ] || [ -z "$HOME_DIR" ]; then
    echo "✗ Could not work out where this checkout or the home folder is."
    exit 1
fi

# ── What is where ──────────────────────────────────────────────────────────

# The copy `make install` put down, else this checkout's build, else any
# other gglib on PATH: they all share one data folder and one daemon.
GGLIB=""
for candidate in "$INSTALLED_BIN" "$REPO/target/release/gglib" "$REPO/target/debug/gglib" \
    "$(command -v gglib 2>/dev/null || true)"; do
    if [ -n "$candidate" ] && [ -x "$candidate" ]; then
        GGLIB="$candidate"
        break
    fi
done

PATHS=""
if [ -n "$GGLIB" ]; then
    PATHS="$("$GGLIB" config paths 2>/dev/null || true)"
fi

# One value of `gglib config paths`, which prints `key = value` lines.
path_of() {
    printf '%s\n' "$PATHS" | sed -n "s/^$1 = //p" | head -n 1
}

if [ "$(uname -s)" = "Darwin" ]; then
    DEFAULT_ROOT="$HOME/Library/Application Support/gglib"
else
    DEFAULT_ROOT="${XDG_DATA_HOME:-$HOME/.local/share}/gglib"
fi

DATA_ROOT="$(path_of data_root)"
RESOURCE_ROOT="$(path_of resource_root)"
MODELS_DIR="$(path_of models_dir)"
case "$DATA_ROOT" in
    /*) ;;
    *)
        DATA_ROOT="${GGLIB_DATA_DIR:-$DEFAULT_ROOT}"
        RESOURCE_ROOT="${GGLIB_RESOURCE_DIR:-$DATA_ROOT}"
        MODELS_DIR=""
        ;;
esac
DATA_ROOT="$(canon "$DATA_ROOT")"
RESOURCE_ROOT="$(canon "$RESOURCE_ROOT")"
MODELS_DIR="$(canon "$MODELS_DIR")"
# A default folder that is a link is somebody's arrangement, not a folder to
# delete whole.
if [ -L "$DEFAULT_ROOT" ]; then
    DEFAULT_ROOT=""
else
    DEFAULT_ROOT="$(canon "$DEFAULT_ROOT")"
fi

PROGRAM=()
DATA=()
NOTES=()

# Lists a path under the given heading when it is there, once.
add() {
    local list="$1" path="$2" seen
    if [ ! -e "$path" ] && [ ! -L "$path" ]; then
        return
    fi
    if [ "$list" = PROGRAM ]; then
        PROGRAM+=("$path")
        return
    fi
    if [ "${#DATA[@]}" -gt 0 ]; then
        for seen in "${DATA[@]}"; do
            if [ "$seen" = "$path" ]; then return; fi
        done
    fi
    DATA+=("$path")
}

# Whether a folder is the home folder, or above it or above this checkout:
# nothing in such a folder is gglib's to delete by name.
shared() {
    holds "$1" "$HOME_DIR" || { ! [ "$1" -ef "$REPO" ] && holds "$1" "$REPO"; }
}

# Lists the given names at the top of a folder that is not gglib's alone.
# Elsewhere than this checkout, the names are gglib's only where gglib has
# plainly been: a library database, or one of its two installs.
add_names() {
    local root="$1" name
    shift
    if [ -z "$root" ]; then
        return
    fi
    if shared "$root"; then
        NOTES+=("Left alone, since it is not a folder of gglib's own: $root")
        return
    fi
    if ! [ "$root" -ef "$REPO" ] && [ ! -e "$root/data/gglib.db" ] \
        && [ ! -e "$root/.llama" ] && [ ! -e "$root/.sd" ]; then
        return
    fi
    for name in "$@"; do
        add DATA "$root/$name"
    done
}

add PROGRAM "$INSTALLED_BIN"
for dir in target node_modules web_ui src-tauri/gen; do
    add PROGRAM "$REPO/$dir"
done

# shellcheck disable=SC2086 # STATE_NAMES is a list of plain names
if [ -n "$DATA_ROOT" ] && [ "$DATA_ROOT" = "$DEFAULT_ROOT" ] && ! shared "$DATA_ROOT"; then
    add DATA "$DATA_ROOT"
elif [ -n "$DATA_ROOT" ] && [ -e "$DATA_ROOT/.git" ]; then
    # A source checkout: its `.env` is the checkout's own.
    add_names "$DATA_ROOT" $STATE_NAMES
else
    # Elsewhere `.env` is the settings file gglib keeps in its data folder.
    add_names "$DATA_ROOT" $STATE_NAMES .env
fi
# Only the two installs follow `GGLIB_RESOURCE_DIR`.
if [ -n "$RESOURCE_ROOT" ] && ! [ "$RESOURCE_ROOT" -ef "$DATA_ROOT" ]; then
    add_names "$RESOURCE_ROOT" .llama .sd
fi
# shellcheck disable=SC2086
add_names "$REPO" $STATE_NAMES

BREW_APP=0
if command -v brew >/dev/null 2>&1 && brew list --cask gglib-gui >/dev/null 2>&1; then
    BREW_APP=1
    NOTES+=("The desktop app itself came from Homebrew: brew uninstall --cask gglib-gui")
fi

if [ "$(uname -s)" = "Darwin" ]; then
    if [ "$BREW_APP" = 0 ]; then
        add DATA "/Applications/$APP_NAME.app"
        add DATA "$HOME/Applications/$APP_NAME.app"
    fi
    # The login item "Start at login" registers.
    add DATA "$HOME/Library/LaunchAgents/$APP_NAME.plist"
    add DATA "$HOME/Library/Caches/gglib"
    for id in $APP_IDS; do
        add DATA "$HOME/Library/Caches/$id"
        add DATA "$HOME/Library/WebKit/$id"
        add DATA "$HOME/Library/HTTPStorages/$id"
        add DATA "$HOME/Library/HTTPStorages/$id.binarycookies"
        add DATA "$HOME/Library/Saved Application State/$id.savedState"
        add DATA "$HOME/Library/Preferences/$id.plist"
    done
else
    # The login item, which is written under ~/.config whatever XDG says.
    add DATA "$HOME/.config/autostart/$APP_NAME.desktop"
    for id in $APP_IDS; do
        add DATA "${XDG_DATA_HOME:-$HOME/.local/share}/$id"
        add DATA "${XDG_CACHE_HOME:-$HOME/.cache}/$id"
        add DATA "${XDG_CONFIG_HOME:-$HOME/.config}/$id"
    done
fi

# The models directory gets a question of its own unless the data above
# already holds it. It is left alone when deleting it would take the home
# folder, this checkout or any of that data with it.
MODELS=""
MODELS_IN_DATA=0
if [ -n "$MODELS_DIR" ] && [ -d "$MODELS_DIR" ]; then
    MODELS_SHARED=0
    if holds "$MODELS_DIR" "$HOME_DIR" || holds "$MODELS_DIR" "$REPO" \
        || [ -e "$MODELS_DIR/.git" ]; then
        MODELS_SHARED=1
    fi
    if [ "${#DATA[@]}" -gt 0 ]; then
        for path in "${DATA[@]}"; do
            if [ -d "$path" ] && [ ! -L "$path" ] && holds "$path" "$MODELS_DIR"; then
                MODELS_IN_DATA=1
            elif holds "$MODELS_DIR" "$path"; then
                MODELS_SHARED=1
            fi
        done
    fi
    if [ "$MODELS_IN_DATA" = 1 ]; then
        :
    elif [ "$MODELS_SHARED" = 1 ]; then
        NOTES+=("Left alone, since more than models is in it: $MODELS_DIR")
    else
        MODELS="$MODELS_DIR"
    fi
fi

# Every other gglib on PATH: a packaged install this did not put there.
while IFS= read -r other; do
    if [ -n "$other" ] && [ "$other" != "$INSTALLED_BIN" ]; then
        NOTES+=("Another gglib stays at $other; its package manager removes it.")
    fi
done < <(which -a gglib 2>/dev/null | sort -u)

# ── Ask ────────────────────────────────────────────────────────────────────

show() {
    local path
    for path in "$@"; do
        printf '    %6s  %s\n' "$(du -sh "$path" 2>/dev/null | cut -f1)" "$path"
    done
}

# Yes only for a typed y; no answer, as when nobody is at a terminal, is No.
ask() {
    local reply=""
    printf '%s [y/N] ' "$1"
    read -r reply || true
    [ "$reply" = "y" ] || [ "$reply" = "Y" ]
}

DO_PROGRAM=0
DO_DATA=0
DO_MODELS=0

if [ "${#PROGRAM[@]}" -gt 0 ]; then
    echo "The program: the installed binary and this checkout's build output."
    show "${PROGRAM[@]}"
    if ask "Remove these?"; then DO_PROGRAM=1; fi
    echo
fi

if [ "${#DATA[@]}" -gt 0 ]; then
    echo "gglib's data: your library, chats, settings, device keys and logs, the"
    echo "llama.cpp and image runtime installs, and the desktop app. Every gglib"
    echo "on this machine shares these, a packaged install included."
    show "${DATA[@]}"
    if [ "$MODELS_IN_DATA" = 1 ]; then
        echo "  Your models are inside one of these: $MODELS_DIR"
    fi
    if ask "Delete these too? This cannot be undone."; then DO_DATA=1; fi
    echo
fi

if [ -n "$MODELS" ]; then
    echo "Your downloaded models:"
    show "$MODELS"
    if ask "Delete the models as well? This cannot be undone."; then DO_MODELS=1; fi
    echo
fi

notes() {
    local note
    if [ "${#NOTES[@]}" -gt 0 ]; then
        for note in "${NOTES[@]}"; do
            echo "  $note"
        done
    fi
}

if [ "$DO_PROGRAM$DO_DATA$DO_MODELS" = "000" ]; then
    echo "Nothing removed."
    notes
    exit 0
fi

# ── Remove ─────────────────────────────────────────────────────────────────

# A daemon or a desktop app still running holds the database and would write
# the folder back, so data goes only once neither is. With a binary the
# daemon is asked to stop; with none, any gglib still running is a refusal.
if [ "$DO_DATA" = 1 ] || [ "$DO_MODELS" = 1 ]; then
    if pgrep -x gglib-app >/dev/null 2>&1; then
        echo "✗ The desktop app is running. Quit it, then run this again."
        echo "  Nothing was removed."
        exit 1
    fi
    if [ -n "$GGLIB" ]; then
        if ! "$GGLIB" daemon stop; then
            echo "✗ The daemon was not stopped (see above). Nothing was removed."
            exit 1
        fi
    elif pgrep -x gglib >/dev/null 2>&1; then
        echo "✗ A gglib is still running and there is no gglib here to stop it with."
        echo "  Stop it, then run this again. Nothing was removed."
        exit 1
    fi
fi

if [ "$DO_DATA" = 1 ]; then
    for path in "${DATA[@]}"; do
        rm -rf -- "$path"
    done
    echo "✓ Deleted gglib's data"
fi

if [ "$DO_MODELS" = 1 ]; then
    rm -rf -- "$MODELS"
    echo "✓ Deleted the models"
fi

if [ "$DO_PROGRAM" = 1 ]; then
    for path in "${PROGRAM[@]}"; do
        rm -rf -- "$path"
    done
    echo "✓ Removed the program"
fi

if [ "$DO_DATA" = 0 ] && [ -n "$DATA_ROOT" ] && [ -d "$DATA_ROOT" ]; then
    echo "  Your data is still in $DATA_ROOT"
fi
if [ "$DO_MODELS" = 0 ] && [ -n "$MODELS" ]; then
    echo "  Your models are still in $MODELS"
elif [ "$DO_DATA" = 0 ] && [ "$MODELS_IN_DATA" = 1 ]; then
    echo "  Your models are still in $MODELS_DIR"
fi
notes
