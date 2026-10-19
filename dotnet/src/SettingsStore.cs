// 本文件负责 Windows 版 QuickFTP 的配置持久化。
// 配置写入 INI 文本，密码只以 DPAPI 密文形式保存，避免明文落盘。
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Security.Cryptography;
using System.Text;

namespace QuickFTP
{
    /// <summary>
    /// 账号权限级别。数值会直接写入配置文件，因此不要随意调整顺序。
    /// </summary>
    public enum AccessMode
    {
        Disabled = 0,
        ReadOnly = 1,
        ReadWrite = 2
    }

    /// <summary>
    /// 界面与服务器共用的配置模型。默认共享当前用户目录，端口使用 2121，
    /// 登录账号默认为读写权限，匿名访问默认关闭。
    /// </summary>
    public class AppSettings
    {
        public string RootPath;
        public int Port;
        public string UserName;
        public string Password;
        public AccessMode UserAccess;
        public AccessMode AnonymousAccess;

        public AppSettings()
        {
            RootPath = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
            Port = 2121;
            UserName = "user";
            Password = "";
            UserAccess = AccessMode.ReadWrite;
            AnonymousAccess = AccessMode.Disabled;
        }
    }

    /// <summary>
    /// 负责 settings.ini 的读写。文件损坏或权限不足时回退到默认配置，
    /// 不应因为配置问题阻止程序启动。
    /// </summary>
    public static class SettingsStore
    {
        private const string PasswordPrefixDpapi = "dpapi:";
        private const string PasswordPrefixRaw = "raw:";

        private static string SettingsDirectory
        {
            get
            {
                // 便携模式：显式指定 QUICKFTP_DATA_DIR 时不再使用 %APPDATA%。
                string overrideDirectory = Environment.GetEnvironmentVariable("QUICKFTP_DATA_DIR");
                if (!string.IsNullOrEmpty(overrideDirectory))
                {
                    return overrideDirectory;
                }

                return Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
                    "QuickFTP");
            }
        }

        private static string SettingsFile
        {
            get { return Path.Combine(SettingsDirectory, "settings.ini"); }
        }

        public static AppSettings Load()
        {
            AppSettings settings = new AppSettings();
            try
            {
                if (!File.Exists(SettingsFile))
                {
                    return settings;
                }

                Dictionary<string, string> values = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
                // 使用简单的 key=value 格式，保持配置文件可读、可手工修复。
                foreach (string rawLine in File.ReadAllLines(SettingsFile, Encoding.UTF8))
                {
                    string line = rawLine.Trim();
                    if (line.Length == 0 || line.StartsWith("#", StringComparison.Ordinal))
                    {
                        continue;
                    }

                    int equals = line.IndexOf('=');
                    if (equals <= 0)
                    {
                        continue;
                    }

                    values[line.Substring(0, equals).Trim()] = line.Substring(equals + 1).Trim();
                }

                string value;
                if (values.TryGetValue("root", out value))
                {
                    string decoded = DecodeString(value);
                    if (!string.IsNullOrEmpty(decoded))
                    {
                        settings.RootPath = decoded;
                    }
                }

                if (values.TryGetValue("port", out value))
                {
                    int port;
                    if (int.TryParse(value, NumberStyles.Integer, CultureInfo.InvariantCulture, out port) &&
                        port >= 1 && port <= 65535)
                    {
                        settings.Port = port;
                    }
                }

                if (values.TryGetValue("username", out value))
                {
                    settings.UserName = DecodeString(value);
                }

                if (values.TryGetValue("password", out value))
                {
                    settings.Password = Unprotect(value);
                }

                if (values.TryGetValue("userAccess", out value))
                {
                    settings.UserAccess = ParseAccess(value, settings.UserAccess);
                }

                if (values.TryGetValue("anonymousAccess", out value))
                {
                    settings.AnonymousAccess = ParseAccess(value, settings.AnonymousAccess);
                }
            }
            catch
            {
                // Settings are a convenience. A damaged file must not prevent the app from starting.
            }

            return settings;
        }

        public static void Save(AppSettings settings)
        {
            if (settings == null)
            {
                return;
            }

            try
            {
                // 先写字符串再一次性落盘，避免多个字段写入过程中出现半截配置。
                Directory.CreateDirectory(SettingsDirectory);
                StringBuilder text = new StringBuilder();
                text.AppendLine("# QuickFTP Server for Windows");
                text.AppendLine("root=" + EncodeString(settings.RootPath));
                text.AppendLine("port=" + settings.Port.ToString(CultureInfo.InvariantCulture));
                text.AppendLine("username=" + EncodeString(settings.UserName));
                text.AppendLine("password=" + Protect(settings.Password));
                text.AppendLine("userAccess=" + ((int)settings.UserAccess).ToString(CultureInfo.InvariantCulture));
                text.AppendLine("anonymousAccess=" + ((int)settings.AnonymousAccess).ToString(CultureInfo.InvariantCulture));
                File.WriteAllText(SettingsFile, text.ToString(), new UTF8Encoding(false));
            }
            catch
            {
                // Saving is best effort; the server can still run for this session.
            }
        }

        private static AccessMode ParseAccess(string value, AccessMode fallback)
        {
            int parsed;
            if (int.TryParse(value, NumberStyles.Integer, CultureInfo.InvariantCulture, out parsed) &&
                parsed >= 0 && parsed <= 2)
            {
                return (AccessMode)parsed;
            }

            return fallback;
        }

        private static string EncodeString(string value)
        {
            return Convert.ToBase64String(Encoding.UTF8.GetBytes(value ?? ""));
        }

        private static string DecodeString(string value)
        {
            try
            {
                return Encoding.UTF8.GetString(Convert.FromBase64String(value ?? ""));
            }
            catch
            {
                return "";
            }
        }

        private static string Protect(string value)
        {
            byte[] clear = Encoding.UTF8.GetBytes(value ?? "");
            try
            {
                // DPAPI + CurrentUser：只有当前 Windows 用户能解密该密码。
                byte[] encrypted = ProtectedData.Protect(clear, null, DataProtectionScope.CurrentUser);
                return PasswordPrefixDpapi + Convert.ToBase64String(encrypted);
            }
            catch
            {
                // 加密失败时不回退为明文，宁可让用户下次重新输入密码。
                return "";
            }
        }

        private static string Unprotect(string value)
        {
            if (string.IsNullOrEmpty(value))
            {
                return "";
            }

            try
            {
                if (value.StartsWith(PasswordPrefixDpapi, StringComparison.Ordinal))
                {
                    byte[] encrypted = Convert.FromBase64String(value.Substring(PasswordPrefixDpapi.Length));
                    byte[] clear = ProtectedData.Unprotect(encrypted, null, DataProtectionScope.CurrentUser);
                    return Encoding.UTF8.GetString(clear);
                }

                if (value.StartsWith(PasswordPrefixRaw, StringComparison.Ordinal))
                {
                    return Encoding.UTF8.GetString(Convert.FromBase64String(value.Substring(PasswordPrefixRaw.Length)));
                }
            }
            catch
            {
                return "";
            }

            return "";
        }
    }
}
