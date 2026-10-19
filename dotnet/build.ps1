$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root

$csc = Join-Path $env:WINDIR "Microsoft.NET\Framework64\v4.0.30319\csc.exe"
if (-not (Test-Path -LiteralPath $csc)) {
    $csc = Join-Path $env:WINDIR "Microsoft.NET\Framework\v4.0.30319\csc.exe"
}
if (-not (Test-Path -LiteralPath $csc)) {
    throw "The .NET Framework C# compiler was not found."
}

$outputDir = Join-Path $root "dist"
New-Item -ItemType Directory -Path $outputDir -Force | Out-Null

$sources = Get-ChildItem -LiteralPath (Join-Path $root "src") -Filter "*.cs" |
    Sort-Object Name |
    ForEach-Object { $_.FullName }

$arguments = @(
    "/nologo",
    "/target:winexe",
    "/optimize+",
    "/out:$outputDir\QuickFTP.exe",
    "/win32icon:$root\assets\QuickFTP.ico",
    "/win32manifest:$root\app.manifest",
    "/reference:System.dll",
    "/reference:System.Core.dll",
    "/reference:System.Windows.Forms.dll",
    "/reference:System.Drawing.dll",
    "/reference:System.Security.dll"
) + $sources

& $csc $arguments
if ($LASTEXITCODE -ne 0) {
    throw "Compilation failed with exit code $LASTEXITCODE."
}

if (Test-Path -LiteralPath (Join-Path $root "README.md")) {
    Copy-Item -LiteralPath (Join-Path $root "README.md") -Destination $outputDir -Force
}

Write-Host "Built: $outputDir\QuickFTP.exe"
