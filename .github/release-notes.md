## QuickFTP Server for Windows

This release contains both independent Windows editions:

- **QuickFTP-DotNET-win.zip** — C#/WinForms edition for .NET Framework 4.8
- **QuickFTP-Rust-portable.zip** — Rust edition with x64 and x86 executables, no third-party crates

### Notes

- Plain FTP only (no FTPS/SFTP), matching the original application.
- Keep the Rust `.manifest` files next to their executables to enable Windows visual styles.
- Windows Firewall may ask for private-network access on first launch.
- The Rust edition targets Windows 7–11; Windows XP is not supported by its standard-library runtime.

### 中文说明

- **QuickFTP-DotNET-win.zip**：C#/WinForms 版，使用 Windows 自带的 .NET Framework 4.8。
- **QuickFTP-Rust-portable.zip**：Rust 便携版，包含 x64/x86 两个 exe，不依赖第三方 crate。
- 仅提供明文 FTP，不包含 FTPS/SFTP。
- Rust 版建议保留同名 `.manifest` 文件；首次启动请允许 Windows 防火墙的专用网络访问。
- Rust 版面向 Windows 7–11，不支持 Windows XP。
