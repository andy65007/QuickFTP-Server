// FTP 服务端的生命周期管理：
// 这里只负责监听控制端口、接收连接和统一停止；具体协议处理在 FtpSession 中。
using System;
using System.Collections.Generic;
using System.Net;
using System.Net.NetworkInformation;
using System.Net.Sockets;
using System.Threading;

namespace QuickFTP
{
    /// <summary>
    /// 启动服务器所需的不可变配置快照。Start 会复制该对象，避免界面继续修改时影响运行中的服务。
    /// </summary>
    public class FtpConfig
    {
        public string RootPath;
        public int Port;
        public string UserName;
        public string Password;
        public AccessMode UserAccess;
        public AccessMode AnonymousAccess;
        public int IdleTimeoutSeconds;

        public FtpConfig()
        {
            IdleTimeoutSeconds = 300;
        }
    }

    /// <summary>
    /// 简易 FTP 服务端。监听 0.0.0.0:Port，每个控制连接使用一个后台线程。
    /// 服务器不保存全局登录状态，认证和权限都属于各自的 FtpSession。
    /// </summary>
    public class FtpServer
    {
        private readonly object _sync = new object();
        private readonly List<FtpSession> _sessions = new List<FtpSession>();
        private TcpListener _listener;
        private Thread _acceptThread;
        private FtpConfig _config;
        private volatile bool _running;

        public bool IsRunning
        {
            get { return _running; }
        }

        public int Port
        {
            get { return _config == null ? 0 : _config.Port; }
        }

        public string RootPath
        {
            get { return _config == null ? "" : _config.RootPath; }
        }

        public void Start(FtpConfig config)
        {
            if (_running)
            {
                throw new InvalidOperationException("The server is already running.");
            }

            if (config == null)
            {
                throw new ArgumentNullException("config");
            }

            if (string.IsNullOrEmpty(config.RootPath) || !System.IO.Directory.Exists(config.RootPath))
            {
                throw new System.IO.DirectoryNotFoundException("The selected root path does not exist.");
            }

            // 复制配置，防止启动后界面上的文本框变化影响正在运行的服务器。
            FtpConfig copy = new FtpConfig();
            copy.RootPath = System.IO.Path.GetFullPath(config.RootPath);
            copy.Port = config.Port;
            copy.UserName = config.UserName ?? "";
            copy.Password = config.Password ?? "";
            copy.UserAccess = config.UserAccess;
            copy.AnonymousAccess = config.AnonymousAccess;
            copy.IdleTimeoutSeconds = config.IdleTimeoutSeconds > 0 ? config.IdleTimeoutSeconds : 300;

            TcpListener listener = new TcpListener(IPAddress.Any, copy.Port);
            listener.Start();

            _config = copy;
            _listener = listener;
            _running = true;
            _acceptThread = new Thread(AcceptLoop);
            _acceptThread.IsBackground = true;
            _acceptThread.Name = "QuickFTP accept loop";
            _acceptThread.Start();
        }

        public void Stop()
        {
            _running = false;

            // 先关闭监听器和已登记的控制连接，让阻塞中的 Accept/Read 尽快返回。
            TcpListener listener = _listener;
            _listener = null;
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

            FtpSession[] sessions;
            lock (_sync)
            {
                sessions = _sessions.ToArray();
                _sessions.Clear();
            }

            foreach (FtpSession session in sessions)
            {
                try
                {
                    session.Close();
                }
                catch
                {
                }
            }

            Thread acceptThread = _acceptThread;
            _acceptThread = null;
            if (acceptThread != null && acceptThread != Thread.CurrentThread)
            {
                try
                {
                    acceptThread.Join(1500);
                }
                catch
                {
                }
            }
        }

        internal void RemoveSession(FtpSession session)
        {
            lock (_sync)
            {
                _sessions.Remove(session);
            }
        }

        private void AcceptLoop()
        {
            while (_running)
            {
                TcpClient client;
                try
                {
                    client = _listener.AcceptTcpClient();
                }
                catch
                {
                    break;
                }

                try
                {
                    // 每个 TCP 连接对应一个独立会话；会话线程退出时自行从列表移除。
                    client.NoDelay = true;
                    FtpSession session = new FtpSession(this, client, _config);
                    lock (_sync)
                    {
                        _sessions.Add(session);
                    }

                    Thread thread = new Thread(session.Run);
                    thread.IsBackground = true;
                    thread.Name = "QuickFTP session";
                    thread.Start();
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
                }
            }
        }

        public static string[] GetLocalIPv4Addresses()
        {
            // 状态栏展示的是本机可用的 IPv4 地址，不把回环和 APIPA(169.254.x.x) 地址混进去。
            // 如果没有任何可用网卡，则退回 127.0.0.1，至少方便本机测试。
            List<string> addresses = new List<string>();
            try
            {
                foreach (NetworkInterface adapter in NetworkInterface.GetAllNetworkInterfaces())
                {
                    if (adapter.OperationalStatus != OperationalStatus.Up)
                    {
                        continue;
                    }

                    if (adapter.NetworkInterfaceType == NetworkInterfaceType.Loopback)
                    {
                        continue;
                    }

                    foreach (UnicastIPAddressInformation unicast in adapter.GetIPProperties().UnicastAddresses)
                    {
                        if (unicast.Address.AddressFamily != AddressFamily.InterNetwork)
                        {
                            continue;
                        }

                        string value = unicast.Address.ToString();
                        if (value.StartsWith("169.254.", StringComparison.Ordinal) ||
                            addresses.Contains(value))
                        {
                            continue;
                        }

                        addresses.Add(value);
                    }
                }
            }
            catch
            {
            }

            if (addresses.Count == 0)
            {
                addresses.Add("127.0.0.1");
            }

            addresses.Sort(StringComparer.Ordinal);
            return addresses.ToArray();
        }
    }
}
