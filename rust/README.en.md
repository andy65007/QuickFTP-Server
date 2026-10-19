# QuickFTP Server - Rust portable edition

[中文说明](README.md) · [Project home](../README.en.md)

This is the Rust reimplementation of QuickFTP Server, independent from the .NET edition.

- No third-party crates: `Cargo.toml` has no `dependencies`
- Uses only the Rust standard library, Win32 APIs, and Windows system DLLs
- Runs from a single executable with no bundled DLLs
- Provides 64-bit and 32-bit portable builds

## Run

Run either executable:

```text
dist\QuickFTP-Rust-x64.exe
dist\QuickFTP-Rust-x86.exe
```

Keep the matching `.manifest` file next to each executable to enable Windows visual styles. `QuickFTP-Rust-portable.zip` contains both executables, manifests, and this README.

First use:

1. Choose the folder to share. The default port is `2121`.
2. Set a username/password and access mode, or enable anonymous access only.
3. Click **Start Server**.
4. Allow Windows Firewall access for private networks on first launch.
5. Connect with `ftp://LAN-IP:port` or FileZilla.

Settings are stored next to the executable in `quickftp-rust.ini`, so the whole directory can be copied to another computer. Set the `QUICKFTP_DATA_DIR` environment variable to use a different configuration directory.

Passwords are encrypted with Windows DPAPI (current-user scope) and stored as hexadecimal ciphertext; plaintext passwords are never written.

## Compatibility

| System | Support |
| --- | --- |
| Windows 11 x64 | Supported with the x64 build |
| Windows 10 x64 / 32-bit | Supported with x64 / x86 |
| Windows 8.1 / 8 | Use the matching x86 or x64 build |
| Windows 7 SP1 | Supported with x64 or x86 |
| Windows XP | **Not supported** |

Rust 1.77's standard library uses synchronization and file APIs introduced after XP, such as SRWLock, InitOnce, and GetFileInformationByHandleEx. A true XP build would require a `no_std` or custom runtime, so this edition targets Windows 7–11.

The 32-bit build uses the official Rust i686 GNU target and MinGW-w64. The resulting executables import only these system DLLs:

```text
CRYPT32.dll GDI32.dll KERNEL32.dll msvcrt.dll
NTDLL.dll ole32.dll SHELL32.dll USER32.dll WS2_32.dll
```

No additional runtime installation is required.

## Features

- Root folder, port, start/stop, status, and LAN address display
- Username/password login and anonymous access
- Disabled / Read Only / Read & Write permissions for each account type
- FTP commands: `USER`, `PASS`, `QUIT`, `NOOP`, `SYST`, `FEAT`, `HELP`, `TYPE`, `MODE`, `STRU`, `PWD`, `CWD`, `CDUP`, `PASV`, `EPSV`, `PORT`, `EPRT`, `LIST`, `NLST`, `MLSD`, `MLST`, `RETR`, `STOR`, `APPE`, `REST`, `SIZE`, `MDTM`, `DELE`, `RNFR`, `RNTO`, `MKD`, `RMD`, `STAT`, `ABOR`, `CLNT`, `OPTS`
- Passive/active data connections, resume, and ASCII/binary modes
- UTF-8 filenames
- Server-side enforcement of read-only permissions
- Path sandbox: rejects `..` escapes, drive/ADS paths, Windows device names, and junction/symlink escapes
- DPAPI password protection and portable settings

## GitHub Actions

The repository's `.github/workflows/release.yml` builds the .NET edition and Rust x64/x86 editions on a Windows runner. Pushing a `v*` tag creates a GitHub Release; the workflow can also be run manually and its output downloaded as a workflow artifact.

## Build from source

The build toolchain is kept inside the project and is not installed system-wide:

```powershell
cd rust
.\fetch-toolchain.ps1   # first run downloads Rust 1.77.2 and i686 MinGW
.\build.ps1
```

Output:

```text
rust\dist\
  QuickFTP-Rust-x64.exe
  QuickFTP-Rust-x64.exe.manifest
  QuickFTP-Rust-x86.exe
  QuickFTP-Rust-x86.exe.manifest
  QuickFTP-Rust-portable.zip
```

Source layout:

```text
src\main.rs        Win32 UI, message loop, startup validation
src\win32.rs       raw FFI for user32/gdi32/shell32/ole32/crypt32
src\ftp.rs         FTP protocol, permissions, path sandbox, data connections
src\settings.rs    INI settings and DPAPI password protection
```

## Limitations

- Plain FTP, like the original; no TLS/FTPS.
- IPv4 only.
- One shared root, one login account, and one anonymous account.
- The executables are unsigned; SmartScreen may show an unknown publisher warning.
- Windows XP is not supported; see the compatibility section.

## Attribution

The original QuickFTP Server was created by Leszek Szary (Copyright © 2019 LS). This Rust edition is an independent Windows reimplementation and is not affiliated with the original author.
