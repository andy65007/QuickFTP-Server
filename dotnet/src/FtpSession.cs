// 单个 FTP 控制连接的完整协议状态机。
// 控制连接负责命令/响应，数据连接负责 LIST/RETR/STOR 等实际传输。
// 每个会话持有自己的当前目录、登录状态、权限和 PASV/PORT 数据端点。
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Threading;

namespace QuickFTP
{
    /// <summary>
    /// 一个客户端控制连接的会话。实例不跨线程共享；所有命令都在该连接自己的线程中串行执行。
    /// </summary>
    internal class FtpSession
    {
        private const int MaxCommandLength = 8192;

        private readonly FtpServer _server;
        private readonly TcpClient _control;
        private readonly NetworkStream _controlStream;
        private readonly FtpConfig _config;
        private readonly string _root;
        private readonly string _rootPrefix;
        private readonly object _dataSync = new object();

        // FTP 逻辑当前目录，使用逐级目录名保存，避免直接保存可能逃逸根目录的本机路径。
        private string[] _cwd = new string[0];
        private bool _loggedIn;
        private string _pendingUser;
        private AccessMode _access;
        private bool _ascii;
        // REST 命令设置的断点；RETR/STOR 完成后清空。
        private long _restartOffset;
        private string _renameFrom;
        // PASV 使用临时监听器，PORT 使用主动连接地址，两者同一时间只保留一个。
        private TcpListener _passiveListener;
        private IPEndPoint _activeEndpoint;
        private TcpClient _dataClient;
        private volatile bool _closed;

        public FtpSession(FtpServer server, TcpClient control, FtpConfig config)
        {
            _server = server;
            _control = control;
            _config = config;
            string fullRoot = Path.GetFullPath(config.RootPath);
            // 统一去掉末尾分隔符，并单独保存 rootPrefix，后续用前缀比较阻止目录逃逸。
            _root = fullRoot.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
            if (_root.Length == 0)
            {
                _root = fullRoot;
            }
            else if (_root.EndsWith(":", StringComparison.Ordinal))
            {
                _root += Path.DirectorySeparatorChar;
            }
            _rootPrefix = _root.EndsWith(Path.DirectorySeparatorChar.ToString(), StringComparison.Ordinal)
                ? _root
                : _root + Path.DirectorySeparatorChar;
            _control.ReceiveTimeout = Math.Max(30, config.IdleTimeoutSeconds) * 1000;
            _control.SendTimeout = 60000;
            _controlStream = control.GetStream();
        }

        public void Run()
        {
            try
            {
                // 220 是 FTP 连接建立后的标准欢迎响应，客户端收到后才会发送 USER/PASS。
                Send("220 Browser Ftp Server.");

                while (!_closed)
                {
                    string line = ReadLine();
                    if (line == null)
                    {
                        break;
                    }

                    if (line.Length == 0)
                    {
                        continue;
                    }

                    HandleCommand(line);
                }
            }
            catch (IOException ex)
            {
                SocketException socketError = ex.InnerException as SocketException;
                if (socketError != null && socketError.SocketErrorCode == SocketError.TimedOut)
                {
                    TrySend("421 Timeout: closing control connection.");
                }
            }
            catch (SocketException)
            {
            }
            catch (ObjectDisposedException)
            {
            }
            catch
            {
                TrySend("421 Internal error.");
            }
            finally
            {
                Close();
                _server.RemoveSession(this);
            }
        }

        public void Close()
        {
            _closed = true;
            ClosePassiveListener();
            lock (_dataSync)
            {
                if (_dataClient != null)
                {
                    try
                    {
                        _dataClient.Close();
                    }
                    catch
                    {
                    }
                    _dataClient = null;
                }
            }

            try
            {
                _control.Close();
            }
            catch
            {
            }
        }

        private void HandleCommand(string line)
        {
            // FTP 命令格式为 "COMMAND [argument]"，命令名大小写不敏感。
            line = line.TrimStart();
            int space = line.IndexOf(' ');
            string command = (space < 0 ? line : line.Substring(0, space)).Trim().ToUpperInvariant();
            string argument = space < 0 ? "" : line.Substring(space + 1);

            if (!_loggedIn && !IsPreLoginCommand(command))
            {
                // 未登录时只允许认证、退出和少量能力协商命令，防止匿名探测文件系统。
                Send("530 Please login with USER and PASS.");
                return;
            }

            switch (command)
            {
                case "USER":
                    HandleUser(argument);
                    break;
                case "PASS":
                    HandlePass(argument);
                    break;
                case "ACCT":
                    Send(_loggedIn ? "230 User Logged In." : "530 Please login with USER and PASS.");
                    break;
                case "QUIT":
                    Send("221 Goodbye.");
                    _closed = true;
                    break;
                case "NOOP":
                    Send("200 NOOP Command Successful.");
                    break;
                case "SYST":
                    Send("215 UNIX Type: L8");
                    break;
                case "FEAT":
                    SendFeatures();
                    break;
                case "HELP":
                    Send("500 No Help Available.");
                    break;
                case "CLNT":
                    Send("200 Ok.");
                    break;
                case "OPTS":
                    Send("200 Ok.");
                    break;
                case "TYPE":
                    HandleType(argument);
                    break;
                case "MODE":
                    HandleMode(argument);
                    break;
                case "STRU":
                    HandleStructure(argument);
                    break;
                case "PWD":
                case "XPWD":
                    Send("257 \"" + EscapePath(CurrentDirectory) + "\" is current directory.");
                    break;
                case "CWD":
                case "XCWD":
                    HandleChangeDirectory(argument);
                    break;
                case "CDUP":
                case "XCUP":
                    HandleChangeToParent();
                    break;
                case "PASV":
                    EnterPassiveMode(false);
                    break;
                case "EPSV":
                    EnterPassiveMode(true);
                    break;
                case "PORT":
                    HandlePort(argument);
                    break;
                case "EPRT":
                    HandleExtendedPort(argument);
                    break;
                case "LIST":
                    HandleList(argument, false, false);
                    break;
                case "NLST":
                    HandleList(argument, true, false);
                    break;
                case "MLSD":
                    HandleList(argument, false, true);
                    break;
                case "MLST":
                    HandleMachineList(argument);
                    break;
                case "RETR":
                    HandleRetrieve(argument);
                    break;
                case "STOR":
                    HandleStore(argument, false);
                    break;
                case "APPE":
                    HandleStore(argument, true);
                    break;
                case "SIZE":
                    HandleSize(argument);
                    break;
                case "MDTM":
                    HandleModificationTime(argument);
                    break;
                case "DELE":
                    HandleDelete(argument);
                    break;
                case "RNFR":
                    HandleRenameFrom(argument);
                    break;
                case "RNTO":
                    HandleRenameTo(argument);
                    break;
                case "MKD":
                case "XMKD":
                    HandleMakeDirectory(argument);
                    break;
                case "RMD":
                case "XRMD":
                    HandleRemoveDirectory(argument);
                    break;
                case "REST":
                    HandleRestart(argument);
                    break;
                case "STAT":
                    HandleStatus(argument);
                    break;
                case "ABOR":
                    ClosePassiveListener();
                    Send("226 ABOR command successful.");
                    break;
                case "REIN":
                    _loggedIn = false;
                    _pendingUser = null;
                    _access = AccessMode.Disabled;
                    _cwd = new string[0];
                    Send("220 Browser Ftp Server.");
                    break;
                default:
                    Send("500 Command not understood.");
                    break;
            }
        }

        private static bool IsPreLoginCommand(string command)
        {
            return command == "USER" || command == "PASS" || command == "QUIT" ||
                   command == "NOOP" || command == "FEAT" || command == "SYST" ||
                   command == "HELP" || command == "CLNT" || command == "OPTS" ||
                   command == "REIN";
        }

        private void HandleUser(string argument)
        {
            if (argument.Length == 0)
            {
                Send("501 Invalid number of arguments.");
                return;
            }

            _loggedIn = false;
            _access = AccessMode.Disabled;
            _cwd = new string[0];
            _pendingUser = argument;
            // 统一返回 331，不暴露用户名是否存在；真正的校验在 PASS 中完成。
            Send("331 Password required for this user.");
        }

        private void HandlePass(string argument)
        {
            if (_pendingUser == null)
            {
                Send("503 Bad sequence of commands.");
                return;
            }

            bool userLogin = _config.UserAccess != AccessMode.Disabled &&
                             string.Equals(_pendingUser, _config.UserName, StringComparison.Ordinal) &&
                             string.Equals(argument, _config.Password, StringComparison.Ordinal);

            bool anonymousName = string.Equals(_pendingUser, "anonymous", StringComparison.OrdinalIgnoreCase) ||
                                 string.Equals(_pendingUser, "ftp", StringComparison.OrdinalIgnoreCase);
            // 匿名账号的密码按 FTP 惯例忽略，但仍然要求先发送 PASS。
            bool anonymousLogin = _config.AnonymousAccess != AccessMode.Disabled && anonymousName;

            if (userLogin)
            {
                _loggedIn = true;
                _access = _config.UserAccess;
                _pendingUser = null;
                Send("230 User Logged In.");
                return;
            }

            if (anonymousLogin)
            {
                _loggedIn = true;
                _access = _config.AnonymousAccess;
                _pendingUser = null;
                Send("230 User Logged In.");
                return;
            }

            _pendingUser = null;
            Send("530 Login or password incorrect!");
        }

        private void SendFeatures()
        {
            Send("211-Features:");
            SendRaw(" CLNT");
            SendRaw(" MDTM");
            SendRaw(" MLST type*;size*;modify*;");
            SendRaw(" REST STREAM");
            SendRaw(" SIZE");
            SendRaw(" UTF8");
            SendRaw("211 End");
        }

        private void HandleType(string argument)
        {
            string type = (argument ?? "").Trim().ToUpperInvariant();
            if (type == "A" || type == "A N")
            {
                _ascii = true;
                Send("200 ASCII transfer mode active.");
            }
            else if (type == "I" || type == "L 8" || type == "L8")
            {
                _ascii = false;
                Send("200 Binary transfer mode active.");
            }
            else
            {
                Send("504 \"" + type + "\": Unsupported transfer MODE.");
            }
        }

        private void HandleMode(string argument)
        {
            string mode = (argument ?? "").Trim().ToUpperInvariant();
            if (mode == "S")
            {
                Send("200 MODE set to S.");
            }
            else if (mode == "Z")
            {
                Send("502 MODE Z non-implemented.");
            }
            else
            {
                Send("504 \"" + mode + "\": Unsupported transfer MODE.");
            }
        }

        private void HandleStructure(string argument)
        {
            string structure = (argument ?? "").Trim().ToUpperInvariant();
            if (structure == "F")
            {
                Send("200 STRU Command Successful.");
            }
            else
            {
                Send("504 STRU failled. Parametre not implemented");
            }
        }

        private void HandleChangeDirectory(string argument)
        {
            if (argument.Length == 0)
            {
                Send("501 Invalid number of arguments.");
                return;
            }

            string[] parts;
            string path;
            if (!TryResolve(argument, out path, out parts) || !Directory.Exists(path))
            {
                Send("550 No such file or directory.");
                return;
            }

            _cwd = parts;
            Send("250 CWD command successful.");
        }

        private void HandleChangeToParent()
        {
            if (_cwd.Length > 0)
            {
                string[] parent = new string[_cwd.Length - 1];
                Array.Copy(_cwd, parent, parent.Length);
                _cwd = parent;
            }

            Send("250 CDUP command successful.");
        }

        private void EnterPassiveMode(bool extended)
        {
            // PASV/EPSV 每次命令都重新开一个临时端口；旧监听器先关闭，避免数据端口串线。
            ClosePassiveListener();
            CloseActiveMode();

            IPAddress localAddress = ((IPEndPoint)_control.Client.LocalEndPoint).Address;
            if (localAddress.AddressFamily != AddressFamily.InterNetwork ||
                localAddress.Equals(IPAddress.Any))
            {
                string[] addresses = FtpServer.GetLocalIPv4Addresses();
                IPAddress parsed;
                if (addresses.Length > 0 && IPAddress.TryParse(addresses[0], out parsed))
                {
                    localAddress = parsed;
                }
                else
                {
                    localAddress = IPAddress.Loopback;
                }
            }

            TcpListener listener = new TcpListener(IPAddress.Any, 0);
            listener.Start();
            _passiveListener = listener;
            int port = ((IPEndPoint)listener.LocalEndpoint).Port;

            if (extended)
            {
                Send("229 Entering Extended Passive Mode (|||" + port.ToString(CultureInfo.InvariantCulture) + "|)");
            }
            else
            {
                // 227 响应必须使用客户端可回连的本机地址，而不是固定的 0.0.0.0。
                byte[] bytes = localAddress.GetAddressBytes();
                Send(string.Format(
                    CultureInfo.InvariantCulture,
                    "227 Entering Passive Mode ({0},{1},{2},{3},{4},{5})",
                    bytes[0], bytes[1], bytes[2], bytes[3], port / 256, port % 256));
            }
        }

        private void HandlePort(string argument)
        {
            // PORT 的 6 个数字分别是 IP 的 4 段和端口高/低字节。
            string[] fields = (argument ?? "").Split(',');
            if (fields.Length != 6)
            {
                Send("501 Syntax error in arguments.");
                return;
            }

            byte[] bytes = new byte[4];
            int[] numbers = new int[6];
            for (int i = 0; i < 6; i++)
            {
                if (!int.TryParse(fields[i].Trim(), NumberStyles.Integer, CultureInfo.InvariantCulture, out numbers[i]) ||
                    numbers[i] < 0 || numbers[i] > 255)
                {
                    Send("501 Syntax error in arguments.");
                    return;
                }

                if (i < 4)
                {
                    bytes[i] = (byte)numbers[i];
                }
            }

            IPAddress requested = new IPAddress(bytes);
            IPAddress remote = ((IPEndPoint)_control.Client.RemoteEndPoint).Address;
            if (remote.IsIPv4MappedToIPv6)
            {
                remote = remote.MapToIPv4();
            }

            if (!requested.Equals(remote))
            {
                // 拒绝 FTP bounce：客户端不能让服务器连接任意第三方地址。
                Send("501 PORT address does not match originator.");
                return;
            }

            ClosePassiveListener();
            _activeEndpoint = new IPEndPoint(requested, numbers[4] * 256 + numbers[5]);
            Send("200 PORT command successful.");
        }

        private void HandleExtendedPort(string argument)
        {
            string[] fields = (argument ?? "").Split('|');
            if (fields.Length < 5)
            {
                Send("501 Syntax error in arguments.");
                return;
            }

            int port;
            IPAddress address;
            if (!IPAddress.TryParse(fields[2], out address) ||
                !int.TryParse(fields[3], NumberStyles.Integer, CultureInfo.InvariantCulture, out port) ||
                port < 1 || port > 65535)
            {
                Send("501 Syntax error in arguments.");
                return;
            }

            IPAddress remote = ((IPEndPoint)_control.Client.RemoteEndPoint).Address;
            if (remote.IsIPv4MappedToIPv6)
            {
                remote = remote.MapToIPv4();
            }

            if (!address.Equals(remote))
            {
                Send("501 PORT address does not match originator.");
                return;
            }

            ClosePassiveListener();
            _activeEndpoint = new IPEndPoint(address, port);
            Send("200 PORT command successful.");
        }

        private void HandleList(string argument, bool namesOnly, bool machine)
        {
            string pathArgument = argument ?? "";
            if (pathArgument.StartsWith("-", StringComparison.Ordinal))
            {
                int firstSpace = pathArgument.IndexOf(' ');
                pathArgument = firstSpace < 0 ? "" : pathArgument.Substring(firstSpace + 1).Trim();
            }

            string[] parts;
            string path;
            if (!TryResolve(pathArgument, out path, out parts) ||
                (!Directory.Exists(path) && !File.Exists(path)))
            {
                Send("550 No such file or directory.");
                return;
            }

            TcpClient data = OpenDataConnection();
            if (data == null)
            {
                Send("425 Can't open data connection.");
                CloseDataConnection(null);
                return;
            }

            Send("150 Opening data channel.");
            // LIST/NLST/MLSD 的文本通过数据连接发送，而不是控制连接。
            try
            {
                string listing;
                if (machine)
                {
                    listing = BuildMachineListing(path);
                }
                else if (namesOnly)
                {
                    listing = BuildNameListing(path);
                }
                else
                {
                    listing = BuildUnixListing(path);
                }

                byte[] payload = Encoding.UTF8.GetBytes(listing);
                NetworkStream stream = data.GetStream();
                stream.Write(payload, 0, payload.Length);
                stream.Flush();
                Send("226 Transfer complete.");
            }
            catch
            {
                Send("426 Previous command has been finished abnormally.");
            }
            finally
            {
                CloseDataConnection(data);
            }
        }

        private void HandleMachineList(string argument)
        {
            string[] parts;
            string path;
            if (!TryResolve(argument ?? "", out path, out parts) ||
                (!Directory.Exists(path) && !File.Exists(path)))
            {
                Send("550 No such file or directory.");
                return;
            }

            bool isDirectory = Directory.Exists(path);
            string name = isDirectory ? new DirectoryInfo(path).Name : new FileInfo(path).Name;
            Send("250-Listing " + EscapePath(name));
            SendRaw(" " + BuildMachineFacts(path, isDirectory) + " " + name);
            Send("250 End");
        }

        private void HandleRetrieve(string argument)
        {
            if (argument.Length == 0)
            {
                Send("501 Invalid number of arguments.");
                return;
            }

            string[] parts;
            string path;
            if (!TryResolve(argument, out path, out parts) || !File.Exists(path))
            {
                Send("550 File not found.");
                return;
            }

            TcpClient data = OpenDataConnection();
            if (data == null)
            {
                Send("425 Can't open data connection.");
                CloseDataConnection(null);
                return;
            }

            long offset = _restartOffset;
            _restartOffset = 0;
            Send("150 FILE: " + Path.GetFileName(path));

            try
            {
                // 支持 REST 断点续传；偏移不能超过文件长度，超出时从文件末尾开始。
                using (FileStream file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite))
                {
                    if (offset > 0)
                    {
                        file.Seek(Math.Min(offset, file.Length), SeekOrigin.Begin);
                    }

                    NetworkStream stream = data.GetStream();
                    if (_ascii)
                    {
                        CopyAsciiOut(file, stream);
                    }
                    else
                    {
                        CopyStream(file, stream);
                    }
                    stream.Flush();
                }

                Send("226 Transfer complete.");
            }
            catch
            {
                Send("426 Previous command has been finished abnormally.");
            }
            finally
            {
                CloseDataConnection(data);
            }
        }

        private void HandleStore(string argument, bool append)
        {
            // 写权限在命令入口统一检查，避免每个分支重复判断或遗漏。
            if (!RequireWriteAccess())
            {
                return;
            }

            if (argument.Length == 0)
            {
                Send("501 Invalid number of arguments.");
                return;
            }

            string[] parts;
            string path;
            if (!TryResolve(argument, out path, out parts))
            {
                Send("550 Can't store file.");
                return;
            }

            string parent = Path.GetDirectoryName(path);
            if (string.IsNullOrEmpty(parent) || !Directory.Exists(parent))
            {
                Send("550 Can't store file.");
                return;
            }

            TcpClient data = OpenDataConnection();
            if (data == null)
            {
                Send("425 Can't open data connection.");
                CloseDataConnection(null);
                return;
            }

            long offset = _restartOffset;
            _restartOffset = 0;
            Send("150 Opening data channel.");

            try
            {
                // APPE 追加；带 REST 的 STOR 从指定偏移覆盖；普通 STOR 截断后重写。
                FileMode mode;
                if (append)
                {
                    mode = FileMode.Append;
                }
                else if (offset > 0)
                {
                    mode = FileMode.OpenOrCreate;
                }
                else
                {
                    mode = FileMode.Create;
                }

                using (FileStream file = new FileStream(path, mode, FileAccess.Write, FileShare.Read))
                {
                    if (!append && offset > 0)
                    {
                        file.Seek(Math.Min(offset, file.Length), SeekOrigin.Begin);
                    }

                    NetworkStream stream = data.GetStream();
                    if (_ascii)
                    {
                        CopyAsciiIn(stream, file);
                    }
                    else
                    {
                        CopyStream(stream, file);
                    }
                    file.Flush();
                }

                Send("226 Transfer complete.");
            }
            catch
            {
                Send("426 Previous command has been finished abnormally.");
            }
            finally
            {
                CloseDataConnection(data);
            }
        }

        private void HandleSize(string argument)
        {
            string[] parts;
            string path;
            if (!TryResolve(argument ?? "", out path, out parts) || !File.Exists(path))
            {
                Send("550 No such file.");
                return;
            }

            Send("213 " + new FileInfo(path).Length.ToString(CultureInfo.InvariantCulture));
        }

        private void HandleModificationTime(string argument)
        {
            string[] parts;
            string path;
            if (!TryResolve(argument ?? "", out path, out parts) || !File.Exists(path))
            {
                Send("550 File not found.");
                return;
            }

            DateTime modified = File.GetLastWriteTimeUtc(path);
            Send("213 " + modified.ToString("yyyyMMddHHmmss", CultureInfo.InvariantCulture));
        }

        private void HandleDelete(string argument)
        {
            if (!RequireWriteAccess())
            {
                return;
            }

            string[] parts;
            string path;
            if (!TryResolve(argument ?? "", out path, out parts) || !File.Exists(path))
            {
                Send("550 Can' t Remove or Access Error.");
                return;
            }

            try
            {
                File.Delete(path);
                Send("250 DELE command successful.");
            }
            catch
            {
                Send("550 Can' t Remove or Access Error.");
            }
        }

        private void HandleRenameFrom(string argument)
        {
            if (!RequireWriteAccess())
            {
                return;
            }

            string[] parts;
            string path;
            if (!TryResolve(argument ?? "", out path, out parts) ||
                (!File.Exists(path) && !Directory.Exists(path)))
            {
                Send("550 File not found.");
                return;
            }

            _renameFrom = path;
            Send("350 File or directory exists, ready for destination name.");
        }

        private void HandleRenameTo(string argument)
        {
            if (!RequireWriteAccess())
            {
                return;
            }

            if (_renameFrom == null)
            {
                Send("503 Bad sequence of commands.");
                return;
            }

            string[] parts;
            string destination;
            if (!TryResolve(argument ?? "", out destination, out parts))
            {
                Send("550 Rename failure.");
                _renameFrom = null;
                return;
            }

            if (File.Exists(destination) || Directory.Exists(destination))
            {
                // 与原版保持一致：目标已存在时拒绝覆盖，避免误删已有文件。
                Send("550 File Already Exists.");
                _renameFrom = null;
                return;
            }

            try
            {
                if (Directory.Exists(_renameFrom))
                {
                    Directory.Move(_renameFrom, destination);
                }
                else
                {
                    File.Move(_renameFrom, destination);
                }

                Send("250 Rename successful.");
            }
            catch
            {
                Send("550 Rename failure.");
            }
            finally
            {
                _renameFrom = null;
            }
        }

        private void HandleMakeDirectory(string argument)
        {
            if (!RequireWriteAccess())
            {
                return;
            }

            string[] parts;
            string path;
            if (!TryResolve(argument ?? "", out path, out parts))
            {
                Send("550 MKD Error Creating DIR.");
                return;
            }

            if (Directory.Exists(path) || File.Exists(path))
            {
                Send("550 File Already Exists.");
                return;
            }

            try
            {
                Directory.CreateDirectory(path);
                Send("250 MKD command successful.");
            }
            catch
            {
                Send("550 MKD Error Creating DIR.");
            }
        }

        private void HandleRemoveDirectory(string argument)
        {
            if (!RequireWriteAccess())
            {
                return;
            }

            string[] parts;
            string path;
            if (!TryResolve(argument ?? "", out path, out parts) || !Directory.Exists(path))
            {
                Send("550 Directory not found.");
                return;
            }

            try
            {
                Directory.Delete(path, false);
                Send("250 Directory deleted successfully.");
            }
            catch
            {
                Send("450 Internal error deleting the directory.");
            }
        }

        private void HandleRestart(string argument)
        {
            long offset;
            if (!long.TryParse((argument ?? "").Trim(), NumberStyles.Integer, CultureInfo.InvariantCulture, out offset) ||
                offset < 0)
            {
                Send("501 Syntax error in arguments.");
                return;
            }

            _restartOffset = offset;
            Send("350 REST command successful.");
        }

        private void HandleStatus(string argument)
        {
            if (string.IsNullOrEmpty(argument))
            {
                Send("211-Status:");
                SendRaw(" Connected");
                SendRaw(" Logged in: " + (_loggedIn ? "yes" : "no"));
                SendRaw(" TYPE: " + (_ascii ? "ASCII" : "Binary"));
                SendRaw("211 End");
                return;
            }

            string[] parts;
            string path;
            if (!TryResolve(argument, out path, out parts) ||
                (!Directory.Exists(path) && !File.Exists(path)))
            {
                Send("550 No such file or directory.");
                return;
            }

            Send("213-Status follows:");
            string listing = BuildUnixListing(path);
            string[] lines = listing.Replace("\r\n", "\n").Split('\n');
            foreach (string line in lines)
            {
                if (line.Length > 0)
                {
                    SendRaw(" " + line);
                }
            }
            Send("213 End of status");
        }

        private TcpClient OpenDataConnection()
        {
            TcpListener listener = _passiveListener;
            if (listener != null)
            {
                DateTime deadline = DateTime.UtcNow.AddSeconds(30);
                while (!_closed)
                {
                    try
                    {
                        if (listener.Pending())
                        {
                            TcpClient accepted = listener.AcceptTcpClient();
                            accepted.NoDelay = true;
                            accepted.ReceiveTimeout = 60000;
                            accepted.SendTimeout = 60000;
                            lock (_dataSync)
                            {
                                _dataClient = accepted;
                            }
                            return accepted;
                        }
                    }
                    catch
                    {
                        return null;
                    }

                    if (DateTime.UtcNow > deadline)
                    {
                        return null;
                    }
                    Thread.Sleep(40);
                }
                return null;
            }

            IPEndPoint active = _activeEndpoint;
            if (active != null)
            {
                TcpClient client = new TcpClient(active.AddressFamily);
                try
                {
                    IAsyncResult result = client.BeginConnect(active.Address, active.Port, null, null);
                    if (!result.AsyncWaitHandle.WaitOne(10000))
                    {
                        client.Close();
                        return null;
                    }
                    client.EndConnect(result);
                    client.NoDelay = true;
                    client.ReceiveTimeout = 60000;
                    client.SendTimeout = 60000;
                    lock (_dataSync)
                    {
                        _dataClient = client;
                    }
                    return client;
                }
                catch
                {
                    try
                    {
                        client.Close();
                    }
                    catch
                    {
                    }
                    return null;
                }
            }

            return null;
        }

        private void CloseDataConnection(TcpClient data)
        {
            lock (_dataSync)
            {
                if (data != null)
                {
                    try
                    {
                        data.Close();
                    }
                    catch
                    {
                    }
                }
                if (_dataClient != null)
                {
                    try
                    {
                        _dataClient.Close();
                    }
                    catch
                    {
                    }
                    _dataClient = null;
                }
            }

            ClosePassiveListener();
            _activeEndpoint = null;
        }

        private void ClosePassiveListener()
        {
            TcpListener listener = _passiveListener;
            _passiveListener = null;
            if (listener != null)
            {
                try
                {
                    listener.Stop();
                }
                catch
                {
                }
            }
        }

        private void CloseActiveMode()
        {
            _activeEndpoint = null;
        }

        private bool RequireWriteAccess()
        {
            if (_access == AccessMode.ReadWrite)
            {
                return true;
            }

            Send("550 Permission denied.");
            return false;
        }

        private string CurrentDirectory
        {
            get { return "/" + string.Join("/", _cwd); }
        }

        private bool TryResolve(string argument, out string fullPath, out string[] logicalParts)
        {
            fullPath = null;
            logicalParts = null;

            try
            {
                string value = (argument ?? "").Replace('\\', '/');
                List<string> parts = new List<string>(_cwd);
                if (value.StartsWith("/", StringComparison.Ordinal))
                {
                    parts.Clear();
                    value = value.TrimStart('/');
                }

                string[] segments = value.Split('/');
                foreach (string segment in segments)
                {
                    if (segment.Length == 0 || segment == ".")
                    {
                        continue;
                    }

                    if (segment == "..")
                    {
                        // 在根目录继续向上不会被允许，逻辑上停留在根目录。
                        if (parts.Count > 0)
                        {
                            parts.RemoveAt(parts.Count - 1);
                        }
                        continue;
                    }

                    if (segment.IndexOf('\0') >= 0 || segment.IndexOf(':') >= 0 || IsReservedWindowsName(segment))
                    {
                        // 冒号可表示 NTFS ADS/盘符，设备名可能绕过普通文件目录。
                        return false;
                    }

                    parts.Add(segment);
                }

                string path = _root;
                string checkPath = _root;
                foreach (string part in parts)
                {
                    path = Path.Combine(path, part);
                    checkPath = Path.Combine(checkPath, part);
                    try
                    {
                        if (File.Exists(checkPath) || Directory.Exists(checkPath))
                        {
                            FileAttributes attributes = File.GetAttributes(checkPath);
                            if ((attributes & FileAttributes.ReparsePoint) != 0)
                            {
                                return false;
                            }
                        }
                    }
                    catch
                    {
                        return false;
                    }
                }

                path = Path.GetFullPath(path);
                // 最终再做一次前缀校验；构造路径时逐级检查重解析点，阻止 junction/符号链接逃逸。
                if (!path.Equals(_root, StringComparison.OrdinalIgnoreCase) &&
                    !path.StartsWith(_rootPrefix, StringComparison.OrdinalIgnoreCase))
                {
                    return false;
                }

                fullPath = path;
                logicalParts = parts.ToArray();
                return true;
            }
            catch
            {
                return false;
            }
        }

        private static bool IsReservedWindowsName(string segment)
        {
            string name = (segment ?? "").TrimEnd(' ', '.');
            int dot = name.IndexOf('.');
            if (dot >= 0)
            {
                name = name.Substring(0, dot);
            }

            name = name.ToUpperInvariant();
            if (name == "CON" || name == "PRN" || name == "AUX" || name == "NUL")
            {
                return true;
            }

            if (name.Length == 4 &&
                (name.StartsWith("COM", StringComparison.Ordinal) || name.StartsWith("LPT", StringComparison.Ordinal)) &&
                name[3] >= '1' && name[3] <= '9')
            {
                return true;
            }

            return false;
        }

        private string BuildUnixListing(string path)
        {
            // 输出接近 Unix 的 LIST 格式，兼容 FileZilla、Explorer 等常见客户端。
            StringBuilder builder = new StringBuilder();
            if (File.Exists(path))
            {
                AppendUnixEntry(builder, new FileInfo(path), false);
                return builder.ToString();
            }

            List<FileSystemInfo> entries = new List<FileSystemInfo>();
            DirectoryInfo directory = new DirectoryInfo(path);
            entries.AddRange(directory.GetDirectories());
            entries.AddRange(directory.GetFiles());
            entries.Sort(delegate(FileSystemInfo left, FileSystemInfo right)
            {
                return string.Compare(left.Name, right.Name, StringComparison.OrdinalIgnoreCase);
            });

            foreach (FileSystemInfo entry in entries)
            {
                AppendUnixEntry(builder, entry, entry is DirectoryInfo);
            }

            return builder.ToString();
        }

        private string BuildNameListing(string path)
        {
            StringBuilder builder = new StringBuilder();
            if (File.Exists(path))
            {
                builder.Append(Path.GetFileName(path));
                builder.Append("\r\n");
                return builder.ToString();
            }

            List<FileSystemInfo> entries = new List<FileSystemInfo>();
            DirectoryInfo directory = new DirectoryInfo(path);
            entries.AddRange(directory.GetDirectories());
            entries.AddRange(directory.GetFiles());
            entries.Sort(delegate(FileSystemInfo left, FileSystemInfo right)
            {
                return string.Compare(left.Name, right.Name, StringComparison.OrdinalIgnoreCase);
            });

            foreach (FileSystemInfo entry in entries)
            {
                builder.Append(entry.Name);
                builder.Append("\r\n");
            }

            return builder.ToString();
        }

        private string BuildMachineListing(string path)
        {
            StringBuilder builder = new StringBuilder();
            if (File.Exists(path))
            {
                FileInfo file = new FileInfo(path);
                builder.Append(BuildMachineFacts(path, false));
                builder.Append(" ");
                builder.Append(file.Name);
                builder.Append("\r\n");
                return builder.ToString();
            }

            List<FileSystemInfo> entries = new List<FileSystemInfo>();
            DirectoryInfo directory = new DirectoryInfo(path);
            entries.AddRange(directory.GetDirectories());
            entries.AddRange(directory.GetFiles());
            entries.Sort(delegate(FileSystemInfo left, FileSystemInfo right)
            {
                return string.Compare(left.Name, right.Name, StringComparison.OrdinalIgnoreCase);
            });

            foreach (FileSystemInfo entry in entries)
            {
                bool isDirectory = entry is DirectoryInfo;
                builder.Append(BuildMachineFacts(entry.FullName, isDirectory));
                builder.Append(" ");
                builder.Append(entry.Name);
                builder.Append("\r\n");
            }

            return builder.ToString();
        }

        private string BuildMachineFacts(string path, bool isDirectory)
        {
            if (isDirectory)
            {
                return "type=dir;modify=" + Directory.GetLastWriteTimeUtc(path).ToString("yyyyMMddHHmmss", CultureInfo.InvariantCulture) + ";";
            }

            FileInfo file = new FileInfo(path);
            return "type=file;size=" + file.Length.ToString(CultureInfo.InvariantCulture) +
                   ";modify=" + file.LastWriteTimeUtc.ToString("yyyyMMddHHmmss", CultureInfo.InvariantCulture) + ";";
        }

        private void AppendUnixEntry(StringBuilder builder, FileSystemInfo entry, bool isDirectory)
        {
            // Windows 没有 Unix 权限位：只读账号统一显示不可写权限，便于客户端识别。
            string permissions;
            if (isDirectory)
            {
                permissions = _access == AccessMode.ReadWrite ? "drwxrwxrwx" : "dr-xr-xr-x";
            }
            else
            {
                permissions = _access == AccessMode.ReadWrite ? "-rw-rw-rw-" : "-r--r--r--";
            }

            long size = 0;
            DateTime modified = entry.LastWriteTime;
            FileInfo file = entry as FileInfo;
            if (file != null)
            {
                size = file.Length;
            }

            string date;
            if (DateTime.Now - modified > TimeSpan.FromDays(180) || modified > DateTime.Now.AddDays(1))
            {
                date = modified.ToString("MMM dd  yyyy", CultureInfo.InvariantCulture);
            }
            else
            {
                date = modified.ToString("MMM dd HH:mm", CultureInfo.InvariantCulture);
            }

            builder.AppendFormat(
                CultureInfo.InvariantCulture,
                "{0} 1 user group {1,14} {2} {3}\r\n",
                permissions,
                size,
                date,
                entry.Name);
        }

        private static void CopyStream(Stream source, Stream destination)
        {
            byte[] buffer = new byte[65536];
            while (true)
            {
                int read = source.Read(buffer, 0, buffer.Length);
                if (read <= 0)
                {
                    break;
                }
                destination.Write(buffer, 0, read);
            }
        }

        private static void CopyAsciiOut(Stream source, Stream destination)
        {
            // ASCII 模式：把本地 LF 转成网络标准 CRLF，同时避免把已有 CRLF 变成 CRCRLF。
            byte[] buffer = new byte[65536];
            bool lastWasCr = false;
            while (true)
            {
                int read = source.Read(buffer, 0, buffer.Length);
                if (read <= 0)
                {
                    break;
                }

                for (int i = 0; i < read; i++)
                {
                    byte value = buffer[i];
                    if (value == (byte)'\n' && !lastWasCr)
                    {
                        destination.WriteByte((byte)'\r');
                    }
                    destination.WriteByte(value);
                    lastWasCr = value == (byte)'\r';
                }
            }
        }

        private static void CopyAsciiIn(Stream source, Stream destination)
        {
            // ASCII 上传方向：把网络 CRLF 还原为本地 LF；跨缓冲区保留 pendingCr 状态。
            byte[] buffer = new byte[65536];
            bool pendingCr = false;
            while (true)
            {
                int read = source.Read(buffer, 0, buffer.Length);
                if (read <= 0)
                {
                    break;
                }

                for (int i = 0; i < read; i++)
                {
                    byte value = buffer[i];
                    if (pendingCr)
                    {
                        if (value == (byte)'\n')
                        {
                            destination.WriteByte((byte)'\n');
                            pendingCr = false;
                            continue;
                        }

                        destination.WriteByte((byte)'\r');
                        pendingCr = false;
                    }

                    if (value == (byte)'\r')
                    {
                        pendingCr = true;
                    }
                    else
                    {
                        destination.WriteByte(value);
                    }
                }
            }

            if (pendingCr)
            {
                destination.WriteByte((byte)'\r');
            }
        }

        private string ReadLine()
        {
            // 限制单条命令长度，避免客户端用超长行消耗内存。
            MemoryStream buffer = new MemoryStream();
            bool tooLong = false;
            while (true)
            {
                int value = _controlStream.ReadByte();
                if (value < 0)
                {
                    return buffer.Length == 0 ? null : Encoding.UTF8.GetString(buffer.ToArray());
                }

                if (value == '\n')
                {
                    break;
                }

                if (value == '\r')
                {
                    continue;
                }

                if (buffer.Length < MaxCommandLength)
                {
                    buffer.WriteByte((byte)value);
                }
                else
                {
                    tooLong = true;
                }
            }

            if (tooLong)
            {
                Send("500 Command line is too long !");
                return "";
            }

            return Encoding.UTF8.GetString(buffer.ToArray());
        }

        private void Send(string line)
        {
            SendRaw(line);
        }

        private void SendRaw(string line)
        {
            byte[] bytes = Encoding.UTF8.GetBytes(line + "\r\n");
            _controlStream.Write(bytes, 0, bytes.Length);
            _controlStream.Flush();
        }

        private void TrySend(string line)
        {
            try
            {
                SendRaw(line);
            }
            catch
            {
            }
        }

        private static string EscapePath(string value)
        {
            return (value ?? "").Replace("\"", "\"\"");
        }
    }
}
