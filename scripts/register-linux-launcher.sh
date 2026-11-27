#!/usr/bin/env bash
set -euo pipefail

package=$(realpath -- "${1:?Usage: bash scripts/register-linux-launcher.sh PACKAGE_DIRECTORY}")
id=com.naroxeno.cuprum
exe="$package/Cuprum"
icon="$package/$id.png"
entry="$package/$id.desktop"
test -x "$exe" && test -f "$icon" && test -f "$entry" || {
    printf 'Missing executable, icon, or desktop entry in %s\n' "$package" >&2
    exit 1
}

applications="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
mkdir -p -- "$applications"
python3 - "$entry" "$exe" "$icon" "$applications/$id.desktop" <<'PY'
from pathlib import Path
import sys

entry, executable, icon, destination = sys.argv[1:]
escaped = executable.replace("%", "%%")
for char in '\\"$`':
    escaped = escaped.replace(char, "\\" + char)
escaped = escaped.replace("\\", "\\\\")
fields = {
    "Name": "Cuprum",
    "Exec": f'"{escaped}"',
    "Icon": icon.replace("\\", "\\\\"),
}
lines = Path(entry).read_text().splitlines()
with Path(destination).open("w", encoding="utf-8") as launcher:
    for line in lines:
        key, _, _ = line.partition("=")
        launcher.write(f"{key}={fields[key]}\n" if key in fields else line + "\n")
PY
printf 'Registered %s/%s.desktop\n' "$applications" "$id"
