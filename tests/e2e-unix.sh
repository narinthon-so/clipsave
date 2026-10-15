#!/usr/bin/env bash
# End-to-end test on Linux (X11, run under xvfb-run) and macOS:
# clipboard -> clipsave -> PNG on disk, plus `install`/`uninstall` on Linux.
# Overwrites the clipboard.
#
#   xvfb-run -a bash tests/e2e-unix.sh target/release/clipsave   # Linux
#   bash tests/e2e-unix.sh target/release/clipsave               # macOS
set -euo pipefail

bin=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
fixture=$PWD/tests/fixtures/alpha.png
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
dir="$work/ไทย folder"
mkdir -p "$dir"

pass() { echo "PASS  $*"; }

copy_png() {
  case "$(uname)" in
    Darwin) osascript -e "set the clipboard to (read (POSIX file \"$fixture\") as «class PNGf»)" ;;
    *) xclip -selection clipboard -t image/png -i "$fixture"; sleep 0.3 ;;
  esac
}

copy_text() {
  case "$(uname)" in
    Darwin) printf hello | pbcopy ;;
    *) printf hello | xclip -selection clipboard; sleep 0.3 ;;
  esac
}

# 1. image on the clipboard -> PNG with alpha intact, path printed
copy_png
out=$("$bin" save "$dir" --name หน้าแรก)
[ "$out" = "$dir/หน้าแรก.png" ] || { echo "FAIL  printed path: $out"; exit 1; }
python3 tests/check_png.py "$out"
pass "save prints $out"

# 2. collision -> suffix
out2=$("$bin" "$dir" -n หน้าแรก)
[ "$out2" = "$dir/หน้าแรก-1.png" ] || { echo "FAIL  collision: $out2"; exit 1; }
pass "collision gets -1 suffix"

# 3. text on the clipboard -> exit 1, nothing written
copy_text
before=$(ls "$dir" | wc -l)
set +e; "$bin" "$dir" 2>/dev/null; code=$?; set -e
[ "$code" -eq 1 ] || { echo "FAIL  text clipboard exit $code"; exit 1; }
[ "$(ls "$dir" | wc -l)" -eq "$before" ] || { echo "FAIL  text clipboard wrote a file"; exit 1; }
pass "text clipboard: exit 1, nothing written"

[ "$(uname)" = Darwin ] && exit 0

# 4. Linux install into a throwaway HOME with fake file managers on PATH,
#    then run the generated Nautilus script the way Nautilus does (cwd = folder).
export HOME="$work/home" XDG_DATA_HOME=
mkdir -p "$HOME/fakebin"
for fm in nautilus nemo dolphin; do printf '#!/bin/sh\n' >"$HOME/fakebin/$fm"; chmod +x "$HOME/fakebin/$fm"; done
PATH="$HOME/fakebin:$PATH" "$bin" install
share="$HOME/.local/share"
for f in "nautilus/scripts/Paste image here" nemo/actions/clipsave.nemo_action \
         kio/servicemenus/clipsave.desktop kservices5/ServiceMenus/clipsave.desktop; do
  [ -x "$share/$f" ] || { echo "FAIL  missing or not executable: $f"; exit 1; }
done
[ -x "$HOME/.local/bin/clipsave" ] || { echo "FAIL  binary not installed"; exit 1; }
pass "install wrote all menu entries"

copy_png
before=$(ls "$dir" | wc -l)
(cd "$dir" && NAUTILUS_SCRIPT_SELECTED_FILE_PATHS= "$share/nautilus/scripts/Paste image here")
[ "$(ls "$dir" | wc -l)" -eq $((before + 1)) ] || { echo "FAIL  nautilus script wrote nothing"; exit 1; }
pass "nautilus script saves into the current folder"

"$bin" uninstall
[ ! -e "$share/nemo/actions/clipsave.nemo_action" ] && [ ! -e "$HOME/.local/bin/clipsave" ] \
  || { echo "FAIL  uninstall left files"; exit 1; }
pass "uninstall removed everything"
