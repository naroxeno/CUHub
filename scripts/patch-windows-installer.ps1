# Updates the generated Inno Setup file when packaging through Niubash on Windows.
param([Parameter(Mandatory)][string]$Path)

$text = [IO.File]::ReadAllText($Path)
$match = [regex]::Match($text, '(?m)^AppVersion=(.+)$')
if (-not $match.Success) {
    throw 'AppVersion missing from Cuhub.iss'
}

$version = $match.Groups[1].Value.Trim()
$parts = @($version.Split('.'))
while ($parts.Count -lt 4) {
    $parts += '0'
}
$versionInfo = ($parts[0..3] -join '.')
$metadata = @"
AppPublisher=Naro-Xeno
AppPublisherURL=https://github.com/naroxeno/CUHub
AppSupportURL=https://github.com/naroxeno/CUHub/issues
AppUpdatesURL=https://github.com/naroxeno/CUHub/releases
AppCopyright=Copyright (C) 2026 Naro-Xeno
VersionInfoCompany=Naro-Xeno
VersionInfoDescription=Cuhub installer
VersionInfoProductName=Cuhub
VersionInfoProductVersion=$version
VersionInfoVersion=$versionInfo
LicenseFile=LICENSE
"@

if (-not $text.Contains('AppPublisher=')) {
    $text = [regex]::Replace(
        $text,
        '(?m)^AppVerName=.*\r?\n',
        { param($found) $found.Value + $metadata + "`r`n" },
        1
    )
}
if (-not $text.Contains('LicenseFile=')) {
    throw 'LicenseFile missing from Cuhub.iss'
}

$installDelete = @"
[InstallDelete]
Type: filesandordirs; Name: "{app}\nsh"
Type: files; Name: "{app}\LICENSE"
Type: files; Name: "{app}\README.md"
Type: files; Name: "{app}\README.zh-CN.md"
Type: filesandordirs; Name: "{app}\docs"
"@
$text = [regex]::Replace(
    $text,
    '(?ms)^\[InstallDelete\]\r?\n.*?(?=\r?\n\[)',
    { $installDelete },
    1
)

$uninstallDelete = @"
[UninstallDelete]
Type: filesandordirs; Name: "{app}\nsh"
Type: files; Name: "{app}\LICENSE"
Type: files; Name: "{app}\README.md"
Type: files; Name: "{app}\README.zh-CN.md"
Type: filesandordirs; Name: "{app}\docs"
Type: dirifempty; Name: "{app}"
"@
if (-not $text.Contains('[UninstallDelete]')) {
    $text = [regex]::Replace(
        $text,
        '(?m)^\[Run\]$',
        { $uninstallDelete + "`r`n`r`n[Run]" },
        1
    )
}
if (-not $text.Contains('[UninstallDelete]')) {
    throw '[Run] section missing from Cuhub.iss'
}

[IO.File]::WriteAllText($Path, $text, [Text.UTF8Encoding]::new($false))
