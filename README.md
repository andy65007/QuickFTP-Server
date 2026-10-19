# QuickFTP Server for Windows

[English README](README.en.md)

这是 macOS 应用 **QuickFTP Server 1.0.8** 的 Windows 重实现。仓库包含两个相互独立的版本：

| 版本 | 技术栈 | 目录 | 特点 |
| --- | --- | --- | --- |
| .NET 版 | C# + WinForms + .NET Framework 4.8 | [`dotnet/`](dotnet/) | 使用 Windows 自带的 .NET Framework，单个 exe |
| Rust 版 | Rust + 原生 Win32 FFI | [`rust/`](rust/) | 不依赖任何第三方 crate，提供 x64/x86 便携版 |

两个版本都还原了原应用的核心使用方式：选择一个共享目录，设置端口，配置登录账号或匿名访问权限，然后一键启动 FTP 服务。原版是明文 FTP，不包含 FTPS/SFTP；本项目保持同样的协议范围。

## 功能

- 根目录、端口、状态、启动/停止
- 用户名/密码登录和匿名访问
- 用户与匿名账号分别支持：禁用 / 只读 / 读写
- 被动 PASV/EPSV 和主动 PORT/EPRT 数据连接
- `LIST`、`NLST`、`MLSD`、`RETR`、`STOR`、`APPE`、`REST`、`SIZE`、`MDTM`、重命名、删除、目录操作等
- UTF-8 文件名、断点续传、ASCII/二进制模式
- 只读权限在服务端强制执行
- 路径沙箱：阻止 `..`、盘符、NTFS ADS、Windows 设备名和 junction/符号链接逃逸
- 密码使用 Windows DPAPI 加密保存
- .NET 版使用 `%APPDATA%\QuickFTP\settings.ini`；Rust 版使用 exe 同目录的 `quickftp-rust.ini`，便于便携复制

## 快速开始

### 直接使用发布包

在 GitHub **Releases** 页面下载对应压缩包，或在本地构建后使用：

```text
dotnet\dist\QuickFTP.exe
rust\dist\QuickFTP-Rust-x64.exe
rust\dist\QuickFTP-Rust-x86.exe
```

Rust 版建议同时保留同名的 `.manifest` 文件，以启用 Windows 视觉样式。

启动后：

1. 选择共享目录，端口默认 `2121`。
2. 在认证区域设置用户名/密码和权限，或者只开启匿名访问。
3. 点击 **Start Server**。
4. 首次启动时允许 Windows 防火墙的“专用网络”访问。
5. 用 `ftp://局域网IP:端口` 或 FileZilla 连接。

## 兼容性

| 系统 | .NET 版 | Rust 版 |
| --- | --- | --- |
| Windows 11 x64 | 支持 | 支持 x64 |
| Windows 10 x64 / 32 位 | 支持 | 支持 x64 / x86 |
| Windows 8.1 / 8 | 支持 | 支持 x64 / x86 |
| Windows 7 SP1 | 支持 | 支持 x64 / x86 |
| Windows XP | 未验证，不建议 | 不支持 |

Rust 1.77 的标准库会依赖 Vista 之后的同步和文件 API，因此 Rust 版明确以 Windows 7–11 为目标。要真正兼容 XP，需要另行实现 `no_std` 或自定义运行时，不能只靠 32 位编译解决。

## 从源码构建

### .NET 版

Windows 11 自带 .NET Framework 4.8 的 C# 编译器：

```powershell
cd dotnet
.\build.ps1
```

输出 `dotnet\dist\QuickFTP.exe`。

### Rust 版

构建工具链是项目内的便携目录，不安装到系统目录：

```powershell
cd rust
.\fetch-toolchain.ps1   # 首次需要联网下载 Rust 1.77.2 和 i686 MinGW
.\build.ps1
```

输出 `rust\dist\QuickFTP-Rust-x64.exe` 和 `rust\dist\QuickFTP-Rust-x86.exe`。Rust 版只使用标准库和 Windows 系统 DLL，不包含第三方 crate。

## GitHub Actions 自动构建

仓库包含 [`.github/workflows/release.yml`](.github/workflows/release.yml)：

- 推送 `v*` 标签时，在 Windows runner 上同时构建 .NET 版和 Rust x64/x86 版
- 上传构建产物，并自动创建 GitHub Release
- 也可以在 Actions 页面手动触发，构建结果作为 workflow artifact 保存

推荐发布方式：

```powershell
git tag v1.0.8
git push origin v1.0.8
```

如果只想生成 Release 而不立即发布，可先使用 Actions 的 `workflow_dispatch` 手动运行。

## 安全说明

- 这是明文 FTP，请只在可信局域网使用，不要在公网传输敏感文件。
- 只读权限由服务端验证，不依赖客户端限制了哪些按钮。
- FTP 根目录不是操作系统沙箱：运行服务的 Windows 账号本身仍拥有其文件权限。
- 不要把包含密码的配置文件提交到仓库；密码内容由 DPAPI 保护，但仍不应公开分发配置目录。

## 项目来源

原版 **QuickFTP Server** 由 Leszek Szary 开发（Copyright © 2019 LS）。本仓库是根据原应用行为做的独立 Windows 重实现，不包含原 macOS 应用包，也未使用原应用代码。两个 Windows 版本均为独立实现。
