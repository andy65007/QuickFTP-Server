$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$tools = Join-Path $root "tools"
New-Item -ItemType Directory -Path $tools -Force | Out-Null

$env:RUSTUP_HOME = Join-Path $root ".rustup"
$env:CARGO_HOME = Join-Path $root ".cargo"
$env:RUSTUP_DIST_SERVER = "https://rsproxy.cn"
$env:RUSTUP_UPDATE_ROOT = "https://rsproxy.cn/rustup"

$rustupInit = Join-Path $tools "rustup-init.exe"
if (-not (Test-Path -LiteralPath $rustupInit)) {
    Invoke-WebRequest -Uri "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe" -OutFile $rustupInit -UseBasicParsing
}

& $rustupInit -y --no-modify-path --profile minimal --default-toolchain 1.77.2-x86_64-pc-windows-gnu
if ($LASTEXITCODE -ne 0) {
    throw "Rust toolchain installation failed."
}

$env:PATH = (Join-Path $env:CARGO_HOME "bin") + ";" + $env:PATH
& (Join-Path $env:CARGO_HOME "bin\rustup.exe") target add i686-pc-windows-gnu
if ($LASTEXITCODE -ne 0) {
    throw "i686 Rust target installation failed."
}

$gcc = Get-ChildItem -LiteralPath $tools -Recurse -Filter "i686-w64-mingw32-gcc.exe" -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -notmatch "rust-mingw-i686" } |
    Select-Object -First 1
if ($null -eq $gcc) {
    $archive = Join-Path $tools "mingw32.7z"
    $expectedSize = 109409982
    if (Test-Path -LiteralPath $archive) {
        $actualSize = (Get-Item -LiteralPath $archive).Length
        if ($actualSize -ne $expectedSize) {
            Remove-Item -LiteralPath $archive -Force
        }
    }
    if (-not (Test-Path -LiteralPath $archive)) {
        $urls = @(
            "https://ghproxy.net/https://github.com/niXman/mingw-builds-binaries/releases/download/16.2.0-rt_v14-rev1/i686-16.2.0-release-posix-dwarf-msvcrt-rt_v14-rev1.7z",
            "https://github.com/niXman/mingw-builds-binaries/releases/download/16.2.0-rt_v14-rev1/i686-16.2.0-release-posix-dwarf-msvcrt-rt_v14-rev1.7z"
        )
        $downloaded = $false
        foreach ($url in $urls) {
            try {
                Invoke-WebRequest -Uri $url -OutFile $archive -UseBasicParsing
                $downloaded = $true
                break
            }
            catch {
                Remove-Item -LiteralPath $archive -Force -ErrorAction SilentlyContinue
            }
        }
        if (-not $downloaded) {
            throw "MinGW download failed from all mirrors."
        }
    }
    $extract = Join-Path $tools "mingw32"
    New-Item -ItemType Directory -Path $extract -Force | Out-Null
    tar -xf $archive -C $extract
    if ($LASTEXITCODE -ne 0) {
        throw "MinGW archive extraction failed."
    }
}

Write-Host "Toolchain and 32-bit MinGW are ready."
