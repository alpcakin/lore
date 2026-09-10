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
$url = "https://github.com/$repo/releases/download/$version/$name.zip"

$work = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null

try {
    Write-Host "lore: downloading $version for $target"
    $archive = Join-Path $work "$name.zip"
    Invoke-WebRequest -Uri $url -OutFile $archive

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
