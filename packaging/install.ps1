# Installs lore into %LOCALAPPDATA%\Programs\lore. No administrator rights,
# nothing outside your profile.
#
#   irm https://raw.githubusercontent.com/alpcakin/lore/main/packaging/install.ps1 | iex

$ErrorActionPreference = 'Stop'

$repo = 'alpcakin/lore'
$target = 'x86_64-pc-windows-msvc'
$binDir = if ($env:LORE_BIN_DIR) { $env:LORE_BIN_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\lore' }

$version = $env:LORE_VERSION
if (-not $version) {
    $version = (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name
}
if (-not $version) { throw 'could not work out the latest version' }

$name = "lore-$version-$target"
$base = "https://github.com/$repo/releases/download/$version"

$work = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null

try {
    Write-Host "lore: downloading $version for $target"
    $archive = Join-Path $work "$name.zip"
    Invoke-WebRequest -Uri "$base/$name.zip" -OutFile $archive

    # The archive travels over https from a host nobody here controls the
    # contents of after the fact. The checksums are published with the release,
    # so verifying costs one more request and turns a swapped asset into a
    # refusal.
    $sums = Join-Path $work 'SHA256SUMS'
    Invoke-WebRequest -Uri "$base/SHA256SUMS" -OutFile $sums

    # GNU sha256sum separates with two spaces and its binary mode with a space
    # and a star, so both are accepted.
    $pattern = "^([0-9a-fA-F]{64})[\s\*]+$([regex]::Escape("$name.zip"))$"
    $line = Get-Content $sums | Where-Object { $_ -match $pattern } | Select-Object -First 1
    if (-not $line) { throw "$name.zip is not listed in SHA256SUMS" }

    $expected = $Matches[1]
    $actual = (Get-FileHash $archive -Algorithm SHA256).Hash
    if ($expected -ne $actual) { throw "checksum mismatch for $name.zip, refusing to install" }

    Expand-Archive -Path $archive -DestinationPath $work
    New-Item -ItemType Directory -Path $binDir -Force | Out-Null
    Copy-Item (Join-Path $work "$name\lore.exe") (Join-Path $binDir 'lore.exe') -Force
} finally {
    Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "lore: installed to $binDir\lore.exe"

# The user's own PATH, so no elevation and no effect on anyone else.
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath -notlike "*$binDir*") {
    [Environment]::SetEnvironmentVariable('Path', "$userPath;$binDir", 'User')
    Write-Host 'lore: added it to your PATH, open a new terminal first'
}

Write-Host 'lore: now run: lore setup'
