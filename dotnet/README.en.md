# QuickFTP Server - .NET edition

[中文说明](README.md) · [Project home](../README.en.md)

This is the C#/.NET Framework reimplementation of QuickFTP Server. It uses WinForms for a native Windows UI and builds to a single `QuickFTP.exe`.

## Highlights

- C# + WinForms targeting the .NET Framework 4.8 included with Windows
- Builds with the system `csc.exe`; no .NET SDK or Visual Studio required
- Root folder, port, user account, anonymous access, and read-only/read-write permissions
- Built-in FTP server with PASV/PORT, LIST/NLST/MLSD, upload/download, and resume support
- Windows DPAPI password protection; settings in `%APPDATA%\QuickFTP\settings.ini`
- Path sandbox, Windows device-name filtering, and reparse-point checks

## Build

On Windows 11:

```powershell
.\build.ps1
```

Output:

```text
dist\QuickFTP.exe
```

Source layout:

```text
src\Program.cs         entry point
src\MainForm.cs        WinForms UI
src\FtpServer.cs       listener and session management
src\FtpSession.cs      FTP protocol, permissions, transfers
src\SettingsStore.cs   settings and DPAPI
```

## Usage

Run `dist\QuickFTP.exe`, choose the shared folder, port, and authentication mode, then click **Start Server**. Allow private-network access in Windows Firewall on first launch.

The default port is `2121`; the default root is the current user profile, and anonymous access is disabled. Read-only accounts cannot upload, delete, rename, or create/remove directories.

## Compatibility

- Windows 7 SP1, Windows 8/8.1, Windows 10, and Windows 11
- IPv4 only
- Plain FTP; no FTPS/SFTP

## Attribution

The original QuickFTP Server was created by Leszek Szary (Copyright © 2019 LS). This directory is an independent Windows behavioral reimplementation and does not contain the original macOS app bundle.
