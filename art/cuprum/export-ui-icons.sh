#!/usr/bin/env bash
set -euo pipefail

here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
output="$here/../../crates/app/src/shell/icon_masks.rs"
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT

magick -size 48x60 'xc:#ffffff01' "PNG32:$scratch/canvas.png"
SDL_VIDEODRIVER=dummy XDG_CONFIG_HOME="$scratch/config" HOME="$scratch" \
    libresprite -b "$scratch/canvas.png" --script "$here/paint-ui-icons.js" \
    --save-as "$here/ui-icons.aseprite" --save-as "$scratch/sheet.png"
magick "$scratch/sheet.png" -alpha extract -depth 8 "gray:$scratch/mask"
python3 - "$here/paint-ui-icons.js" "$scratch/mask" "$output" <<'PYTHON'
from pathlib import Path
import re
import sys

drawing, mask, output = map(Path, sys.argv[1:])
names = re.findall(r'^    \["([a-z]+)", ', drawing.read_text(), re.MULTILINE)
pixels = mask.read_bytes()
assert len(pixels) == 48 * 60
lines = ["// Generated from the LibreSprite sheet by art/cuprum/export-ui-icons.sh.", ""]
for index, name in enumerate(names):
    left = index % 4 * 12 + 2
    top = index // 4 * 12 + 2
    rows = []
    for y in range(top, top + 8):
        row = pixels[y * 48 + left : y * 48 + left + 8]
        assert set(row) <= {0, 255}
        rows.append(sum(1 << (7 - x) for x, pixel in enumerate(row) if pixel))
    assert any(rows), name
    values = ", ".join(f"0x{row:02x}" for row in rows)
    lines.append(f"pub const {name.upper()}: [u8; 8] = [{values}];")
Path(output).write_text("\n".join(lines) + "\n")
PYTHON
