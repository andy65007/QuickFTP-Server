$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$cargoHome = Join-Path $root ".cargo"
$rustupHome = Join-Path $root ".rustup"
$toolchain = Join-Path $rustupHome "toolchains\1.77.2-x86_64-pc-windows-gnu"
$localCargo = Join-Path $toolchain "bin\cargo.exe"
$useLocalToolchain = Test-Path -LiteralPath $localCargo
if ($useLocalToolchain) {
    $cargo = $localCargo
} else {
    $cargoCommand = Get-Command cargo.exe -ErrorAction SilentlyContinue
    if ($null -eq $cargoCommand) {
        throw "Rust toolchain not found. Run .\fetch-toolchain.ps1 or install Rust 1.77.2."
    }
    $cargo = $cargoCommand.Source
}

$gcc = Get-ChildItem -LiteralPath (Join-Path $root "tools") -Recurse -Filter "i686-w64-mingw32-gcc.exe" -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -notmatch "rust-mingw-i686" } |
    Select-Object -First 1
if ($null -eq $gcc) {
    $gccCommand = Get-Command i686-w64-mingw32-gcc.exe -ErrorAction SilentlyContinue
    if ($null -eq $gccCommand) {
        throw "32-bit MinGW linker not found. Run .\fetch-toolchain.ps1 or install MinGW-w64."
    }
    $gccDirectory = Split-Path -Parent $gccCommand.Source
} else {
    $gccDirectory = $gcc.Directory.FullName
}

$dist = Join-Path $root "dist"
New-Item -ItemType Directory -Path $dist -Force | Out-Null

$oldPath = $env:PATH
$oldRustFlags = $env:RUSTFLAGS
$oldRustupHome = $env:RUSTUP_HOME
$oldCargoHome = $env:CARGO_HOME
$oldRustToolchain = $env:RUSTUP_TOOLCHAIN
try {
    if ($useLocalToolchain) {
        $env:RUSTUP_HOME = $rustupHome
        $env:CARGO_HOME = $cargoHome
        $env:PATH = (Join-Path $toolchain "bin") + ";" + $env:PATH
    }
    $env:RUSTUP_TOOLCHAIN = "1.77.2-x86_64-pc-windows-gnu"
    $env:PATH = $gccDirectory + ";" + $env:PATH
    Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue

    & $cargo build --release --manifest-path (Join-Path $root "Cargo.toml") --target x86_64-pc-windows-gnu
    if ($LASTEXITCODE -ne 0) {
        throw "x64 build failed."
    }

    & $cargo build --release --manifest-path (Join-Path $root "Cargo.toml") --target i686-pc-windows-gnu
    if ($LASTEXITCODE -ne 0) {
        throw "x86 build failed."
    }
}
finally {
    $env:PATH = $oldPath
    $env:RUSTUP_HOME = $oldRustupHome
    $env:CARGO_HOME = $oldCargoHome
    $env:RUSTUP_TOOLCHAIN = $oldRustToolchain
    if ($null -eq $oldRustFlags) {
        Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue
    } else {
        $env:RUSTFLAGS = $oldRustFlags
    }
}

$x64 = Join-Path $dist "QuickFTP-Rust-x64.exe"
$x86 = Join-Path $dist "QuickFTP-Rust-x86.exe"
Copy-Item -LiteralPath (Join-Path $root "target\x86_64-pc-windows-gnu\release\QuickFTP-Rust.exe") -Destination $x64 -Force
Copy-Item -LiteralPath (Join-Path $root "target\i686-pc-windows-gnu\release\QuickFTP-Rust.exe") -Destination $x86 -Force
Copy-Item -LiteralPath (Join-Path $root "QuickFTP-Rust.exe.manifest") -Destination ($x64 + ".manifest") -Force
Copy-Item -LiteralPath (Join-Path $root "QuickFTP-Rust.exe.manifest") -Destination ($x86 + ".manifest") -Force
Copy-Item -LiteralPath (Join-Path $root "README.md") -Destination $dist -Force

Compress-Archive -LiteralPath @(
    $x64,
    ($x64 + ".manifest"),
    $x86,
    ($x86 + ".manifest"),
    (Join-Path $dist "README.md")
) -DestinationPath (Join-Path $dist "QuickFTP-Rust-portable.zip") -Force

Get-ChildItem -LiteralPath $dist | Select-Object Name, Length | Format-Table -AutoSize
Write-Host "Built Rust x64 and x86 portable editions in $dist"
