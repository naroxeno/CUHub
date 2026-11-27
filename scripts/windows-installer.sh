#!/bin/sh
# Complete the Windows package: add license/readme/attribution, rebuild the zip,
# and recompile the Inno Setup installer with install/uninstall cleanup.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

dist=dist/windows
stage=$dist/Cuprum
if [ ! -d "$stage" ]; then
    echo "missing $stage; run nsh package --target windows --out $dist first" >&2
    exit 1
fi

install -m 0644 LICENSE "$stage/LICENSE"
install -m 0644 README.md "$stage/README.md"
install -m 0644 README.zh-CN.md "$stage/README.zh-CN.md"
mkdir -p "$stage/docs/third-party"
install -m 0644 docs/ARCHITECTURE.md docs/FEATURES.md docs/HANDOVER.md "$stage/docs/"
install -m 0644 docs/third-party/terracotta.md "$stage/docs/third-party/terracotta.md"
install -m 0644 LICENSE "$dist/LICENSE"

rm -f "$dist/Cuprum.zip"
if command -v zip >/dev/null 2>&1; then
    ( cd "$dist" && zip -qr Cuprum.zip Cuprum )
elif command -v powershell.exe >/dev/null 2>&1; then
    powershell.exe -NoProfile -Command \
        "Compress-Archive -LiteralPath '$stage' -DestinationPath '$dist/Cuprum.zip' -CompressionLevel Optimal"
else
    echo "zip or powershell.exe is required to build Cuprum.zip" >&2
    exit 1
fi

if command -v python3 >/dev/null 2>&1 && python3 --version >/dev/null 2>&1; then
python3 - "$dist/Cuprum.iss" <<'PY'
import pathlib
import re
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text(encoding="utf-8")

match = re.search(r"(?m)^AppVersion=(.+)$", text)
if match is None:
    raise SystemExit("AppVersion missing from Cuprum.iss")
version = match.group(1).strip()
parts = version.split(".")
while len(parts) < 4:
    parts.append("0")
version_info = ".".join(parts[:4])

metadata = f"""AppPublisher=Naro-Xeno
AppPublisherURL=https://github.com/naroxeno/Cuprum
AppSupportURL=https://github.com/naroxeno/Cuprum/issues
AppUpdatesURL=https://github.com/naroxeno/Cuprum/releases
AppCopyright=Copyright (C) 2026 Naro-Xeno
VersionInfoCompany=Naro-Xeno
VersionInfoDescription=Cuprum installer
VersionInfoProductName=Cuprum
VersionInfoProductVersion={version}
VersionInfoVersion={version_info}
LicenseFile=LICENSE
"""
if "AppPublisher=" not in text:
    text = re.sub(r"(?m)^AppVerName=.*\n", lambda m: m.group(0) + metadata, text, count=1)

if "LicenseFile=" not in text:
    raise SystemExit("LicenseFile missing from Cuprum.iss")

install_delete = """[InstallDelete]
Type: filesandordirs; Name: "{app}\\nsh"
Type: files; Name: "{app}\\LICENSE"
Type: files; Name: "{app}\\README.md"
Type: files; Name: "{app}\\README.zh-CN.md"
Type: filesandordirs; Name: "{app}\\docs"
"""
text = re.sub(r"(?ms)^\[InstallDelete\]\n.*?(?=\n\[)", lambda m: install_delete, text, count=1)

uninstall_delete = """[UninstallDelete]
Type: filesandordirs; Name: "{app}\\nsh"
Type: files; Name: "{app}\\LICENSE"
Type: files; Name: "{app}\\README.md"
Type: files; Name: "{app}\\README.zh-CN.md"
Type: filesandordirs; Name: "{app}\\docs"
Type: dirifempty; Name: "{app}"
"""
if "[UninstallDelete]" not in text:
    text = text.replace("\n[Run]", "\n" + uninstall_delete + "\n[Run]", 1)

if "[UninstallDelete]" not in text:
    raise SystemExit("[Run] section missing from Cuprum.iss")

path.write_text(text, encoding="utf-8")
PY
elif command -v powershell.exe >/dev/null 2>&1; then
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File \
        scripts/patch-windows-installer.ps1 "$dist/Cuprum.iss"
else
    echo "python3 or powershell.exe is required to update Cuprum.iss" >&2
    exit 1
fi

rm -rf "$dist/Output"
if command -v iscc >/dev/null 2>&1; then
    iscc_cmd=iscc
elif [ -x "$LOCALAPPDATA/Programs/Inno Setup 7/ISCC.exe" ]; then
    iscc_cmd="$LOCALAPPDATA/Programs/Inno Setup 7/ISCC.exe"
else
    echo "Inno Setup 7 is required to build Cuprum-setup.exe" >&2
    exit 1
fi
( cd "$dist" && "$iscc_cmd" Cuprum.iss )
if [ ! -f "$dist/Output/Cuprum-setup.exe" ]; then
    echo "iscc did not produce Output/Cuprum-setup.exe" >&2
    exit 1
fi
mv -f "$dist/Output/Cuprum-setup.exe" "$dist/Cuprum-setup.exe"
rm -rf "$dist/Output"

echo "windows package: $dist/Cuprum, $dist/Cuprum.zip, $dist/Cuprum-setup.exe"
