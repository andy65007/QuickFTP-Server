# QuickFTP Server - .NET edition

[English README](README.en.md) · [项目主页](../README.md)

这是 QuickFTP Server 的 C#/.NET Framework 重实现，使用 WinForms 构建原生 Windows 界面，编译结果是单个 `QuickFTP.exe`。

## 特点

- C# + WinForms，目标框架为 Windows 11 自带的 .NET Framework 4.8
- 无需安装 .NET SDK 或 Visual Studio，使用系统自带的 `csc.exe` 编译
- 根目录、端口、账号、匿名访问、只读/读写权限
- 内置独立 FTP 服务端，支持 PASV/PORT、LIST/NLST/MLSD、上传下载、断点续传等
- 密码使用 Windows DPAPI 加密，配置保存在 `%APPDATA%\QuickFTP\settings.ini`
- 路径沙箱、设备名过滤和重解析点检查

## 构建

在 Windows 11 上运行：

```powershell
.\build.ps1
```

输出：

```text
dist\QuickFTP.exe
```

如果只需要源码，可查看：

```text
src\Program.cs         程序入口
src\MainForm.cs        WinForms 界面
src\FtpServer.cs       监听和会话管理
src\FtpSession.cs      FTP 协议、权限、文件传输
src\SettingsStore.cs   配置和 DPAPI
```

## 使用

运行 `dist\QuickFTP.exe`，选择共享目录、端口和认证方式，然后点击 **Start Server**。首次启动时允许 Windows 防火墙的专用网络访问。

默认端口是 `2121`，默认共享当前用户目录，匿名访问默认关闭。只读账号不能上传、删除、重命名或创建/删除目录。

## 兼容性

- Windows 7 SP1、Windows 8/8.1、Windows 10、Windows 11
- 仅支持 IPv4
- 明文 FTP，不包含 FTPS/SFTP

## 来源

原版 QuickFTP Server 由 Leszek Szary 开发（Copyright © 2019 LS）。本目录是独立的 Windows 行为复刻，不包含原 macOS 应用包。
