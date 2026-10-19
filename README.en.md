# QuickFTP Server for Windows

[中文说明](README.md)

This repository contains two independent Windows reimplementations of the macOS app **QuickFTP Server 1.0.8**:

| Edition | Stack | Directory | Highlights |
| --- | --- | --- | --- |
| .NET edition | C# + WinForms + .NET Framework 4.8 | [`dotnet/`](dotnet/) | Uses the .NET Framework included with Windows; one executable |
| Rust edition | Rust + raw Win32 FFI | [`rust/`](rust/) | No third-party crates; x64/x86 portable builds |

Both editions reproduce the core workflow of the original app: choose a shared folder, set a port, configure a login account or anonymous access, and start the FTP server. Like the original, this is plain FTP without FTPS/SFTP.

## Features

- Root folder, port, status, start/stop
- Username/password login and anonymous access
- Separate Disabled / Read Only / Read & Write permissions for the user and anonymous accounts
- PASV/EPSV passive mode and PORT/EPRT active mode
- `LIST`, `NLST`, `MLSD`, `RETR`, `STOR`, `APPE`, `REST`, `SIZE`, `MDTM`, rename, delete, and directory operations
- UTF-8 filenames, resumable transfers, ASCII/binary modes
- Server-side enforcement of read-only permissions
- Path sandbox: blocks `..`, drive/ADS paths, Windows device names, and junction/symlink escapes
- Passwords protected with Windows DPAPI
- The .NET edition stores settings in `%APPDATA%\QuickFTP\settings.ini`; the Rust edition stores them next to the executable for portability

## Quick start

Download a package from GitHub **Releases**, or use a local build:

```text
dotnet\dist\QuickFTP.exe
rust\dist\QuickFTP-Rust-x64.exe
rust\dist\QuickFTP-Rust-x86.exe
```

Keep each Rust executable's matching `.manifest` file next to it to enable Windows visual styles.

1. Select the folder to share. The default port is `2121`.
2. Configure a username/password and access mode, or enable anonymous access only.
3. Click **Start Server**.
4. Allow Windows Firewall access for private networks on first launch.
5. Connect with `ftp://LAN-IP:port` or FileZilla.

## Compatibility

| System | .NET edition | Rust edition |
| --- | --- | --- |
| Windows 11 x64 | Supported | x64 supported |
| Windows 10 x64 / 32-bit | Supported | x64 / x86 supported |
| Windows 8.1 / 8 | Supported | x64 / x86 supported |
| Windows 7 SP1 | Supported | x64 / x86 supported |
| Windows XP | Not verified, not recommended | Not supported |

Rust 1.77's standard library imports synchronization and file APIs introduced after XP. The Rust edition therefore targets Windows 7–11. True XP support would require a separate `no_std` or custom-runtime implementation, not merely a 32-bit build.

## Build from source

### .NET edition

Windows 11 includes the .NET Framework 4.8 C# compiler:

```powershell
cd dotnet
.\build.ps1
```

Output: `dotnet\dist\QuickFTP.exe`.

### Rust edition

The build toolchain is kept inside the project and is not installed system-wide:

```powershell
cd rust
.\fetch-toolchain.ps1   # first run downloads Rust 1.77.2 and i686 MinGW
.\build.ps1
```

Outputs: `rust\dist\QuickFTP-Rust-x64.exe` and `rust\dist\QuickFTP-Rust-x86.exe`. The Rust edition uses only the standard library and Windows system DLLs, with no third-party crates.

## GitHub Actions

[`.github/workflows/release.yml`](.github/workflows/release.yml) builds both editions on a Windows runner:

- Pushing a `v*` tag builds the .NET edition and Rust x64/x86 binaries, uploads artifacts, and creates a GitHub Release.
- The workflow can also be started manually; its output is saved as a workflow artifact.

Recommended release flow:

```powershell
git tag v1.0.8
git push origin v1.0.8
```

## Security

- This is plain FTP. Use it only on a trusted LAN and do not transfer sensitive files over the public Internet.
- Read-only permissions are enforced by the server, not by hiding client buttons.
- The FTP root is not an operating-system sandbox; the Windows account running the server still has its normal file permissions.
- Do not commit settings containing passwords. DPAPI protects the stored password, but configuration directories should still not be published.

## Attribution

The original **QuickFTP Server** was created by Leszek Szary (Copyright © 2019 LS). This repository is an independent Windows reimplementation based on observed behavior; it does not include the original macOS app bundle or its source code.
