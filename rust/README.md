# QuickFTP Server - Rust portable edition

[English README](README.en.md) · [项目主页](../README.md)

这是 QuickFTP Server 的 Rust 重实现，和第二版 C# 版本相互独立。

- 不依赖任何第三方 crate：`Cargo.toml` 没有 `dependencies`
- 只使用 Rust 标准库、Win32 API 和 Windows 系统 DLL
- 单个 exe 运行，不携带 DLL
- 提供 64 位和 32 位两个便携版本

## 运行

运行以下任一文件：

```text
dist\QuickFTP-Rust-x64.exe
dist\QuickFTP-Rust-x86.exe
```

同目录下的 `.manifest` 文件用于启用 Windows 视觉样式，建议和 exe 一起复制。`QuickFTP-Rust-portable.zip` 已把 exe、manifest 和说明打包在一起。

首次使用：

1. 选择共享目录，端口默认 `2121`。
2. 设置用户名/密码，选择用户权限；或者只开启匿名访问。
3. 点击 **Start Server**。
4. 首次启动时允许 Windows 防火墙的“专用网络”访问。
5. 用 `ftp://局域网IP:端口` 或 FileZilla 连接。

配置默认保存在 exe 同目录的 `quickftp-rust.ini`，因此整个目录可以直接复制到其他电脑。也可以设置 `QUICKFTP_DATA_DIR` 环境变量，把配置放到指定目录。

密码通过 Windows DPAPI（当前用户范围）加密后以十六进制保存，不写入明文。

## 兼容性

| 系统 | 支持情况 |
| --- | --- |
| Windows 11 x64 | 支持，使用 x64 版本 |
| Windows 10 x64 / 32 位 | 支持，分别使用 x64 / x86 版本 |
| Windows 8.1 / 8 | 建议使用 x86 或 x64 对应版本 |
| Windows 7 SP1 | 支持，使用 x64 或 x86 版本 |
| Windows XP | **不支持** |

这里需要说明：Rust 1.77 的标准库已经依赖 Vista 之后才有的同步和文件 API（例如 SRWLock、InitOnce、GetFileInformationByHandleEx）。因此即使生成 32 位 exe，也不能可靠运行在 Windows XP 上。要真正兼容 XP，必须改写标准库运行时或使用 `no_std`，这超出了“无第三方库的普通便携程序”的范围，所以本版明确以 Win7-Win11 为目标。

32 位版本使用 Rust 官方 i686 GNU 目标和 MinGW-w64 链接；生成的 exe 只导入以下系统 DLL：

```text
CRYPT32.dll GDI32.dll KERNEL32.dll msvcrt.dll
NTDLL.dll ole32.dll SHELL32.dll USER32.dll WS2_32.dll
```

不需要额外安装运行库。

## GitHub Actions

仓库根目录的 `.github/workflows/release.yml` 会在 Windows runner 上同时构建 .NET 版、Rust x64 版和 Rust x86 版。推送 `v*` 标签会自动创建 Release；也可以在 Actions 页面手动运行，下载 workflow artifact。

## 功能

- 根目录、端口、启动/停止、状态和局域网地址显示
- 用户名/密码登录、匿名访问
- 用户和匿名访问分别支持：禁用 / 只读 / 读写
- FTP 命令：`USER`、`PASS`、`QUIT`、`NOOP`、`SYST`、`FEAT`、`HELP`、`TYPE`、`MODE`、`STRU`、`PWD`、`CWD`、`CDUP`、`PASV`、`EPSV`、`PORT`、`EPRT`、`LIST`、`NLST`、`MLSD`、`MLST`、`RETR`、`STOR`、`APPE`、`REST`、`SIZE`、`MDTM`、`DELE`、`RNFR`、`RNTO`、`MKD`、`RMD`、`STAT`、`ABOR`、`CLNT`、`OPTS`
- 被动/主动数据连接、断点续传、ASCII/二进制模式
- UTF-8 文件名
- 只读权限在服务端强制执行
- 路径沙箱：拒绝 `..` 逃逸、盘符/ADS、Windows 设备名，并通过 canonicalize 阻止 junction/符号链接逃逸
- 密码 DPAPI 加密、便携配置

## 从源码构建

构建脚本使用项目目录内的便携 Rust 工具链，不会把 Rust 安装到系统目录。

```powershell
cd rust
.\fetch-toolchain.ps1   # 首次需要，联网下载 Rust 1.77.2 和 i686 MinGW
.\build.ps1
```

输出目录：

```text
rust\dist\
  QuickFTP-Rust-x64.exe
  QuickFTP-Rust-x64.exe.manifest
  QuickFTP-Rust-x86.exe
  QuickFTP-Rust-x86.exe.manifest
  QuickFTP-Rust-portable.zip
```

源码结构：

```text
src\main.rs        Win32 界面、消息循环、启动校验
src\win32.rs       user32/gdi32/shell32/ole32/crypt32 的原始 FFI
src\ftp.rs         FTP 协议、权限、路径沙箱、数据连接
src\settings.rs    INI 配置和 DPAPI 密码保护
```

## 限制

- 和原版一样是明文 FTP，不含 TLS/FTPS。
- 仅监听 IPv4。
- 单一共享根目录、一个登录账号和一个匿名账号。
- 未做代码签名，SmartScreen 可能提示未知发布者。
- Windows XP 不支持，原因见上文。

## 来源

原版 QuickFTP Server 由 Leszek Szary 开发（Copyright © 2019 LS）。Rust 版是根据原应用行为做的独立 Windows 重实现，与原作者无隶属关系。
