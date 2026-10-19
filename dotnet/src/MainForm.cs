// WinForms 前端：负责配置编辑、启动校验、状态显示和服务器生命周期。
// 真正的 FTP 协议实现位于 FtpServer/FtpSession，界面层不直接处理网络数据。
using System;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.IO;
using System.Windows.Forms;

namespace QuickFTP
{
    /// <summary>
    /// QuickFTP 主窗口。表单是单线程 STA，服务器在后台线程运行；
    /// 停止服务时通过共享状态和 socket 关闭让后台线程退出。
    /// </summary>
    public class MainForm : Form
    {
        private AppSettings _settings;
        private FtpServer _server;

        private TextBox _rootBox;
        private Button _browseButton;
        private NumericUpDown _portBox;
        private PictureBox _statusImage;
        private Label _statusLabel;
        private Button _startButton;
        private Button _stopButton;
        private Label _addressLabel;
        private LinkLabel _copyLink;

        private TextBox _userNameBox;
        private TextBox _passwordBox;
        private ComboBox _userAccessBox;
        private ComboBox _anonymousAccessBox;
        private LinkLabel _moreAppsLink;

        public MainForm()
        {
            // 配置只在启动时读取一次；用户修改后会在启动/关闭时保存。
            _settings = SettingsStore.Load();

            Text = "QuickFTP Server";
            Font = new Font("Segoe UI", 9F, FontStyle.Regular, GraphicsUnit.Point);
            AutoScaleMode = AutoScaleMode.Dpi;
            FormBorderStyle = FormBorderStyle.FixedSingle;
            MaximizeBox = false;
            StartPosition = FormStartPosition.CenterScreen;
            ClientSize = new Size(500, 365);
            BackColor = Color.FromArgb(245, 245, 247);

            try
            {
                Icon = Icon.ExtractAssociatedIcon(Application.ExecutablePath);
            }
            catch
            {
            }

            BuildInterface();
            ApplySettingsToInterface();
            UpdateRunningInterface(false);
        }

        private void BuildInterface()
        {
            // 三个页面与 macOS 原版对应：General、Authentication、About。
            TabControl tabs = new TabControl();
            tabs.Dock = DockStyle.Fill;
            tabs.Padding = new Point(14, 6);
            tabs.Font = new Font("Segoe UI", 9F, FontStyle.Regular, GraphicsUnit.Point);
            Controls.Add(tabs);

            TabPage general = new TabPage("General");
            general.BackColor = Color.White;
            tabs.TabPages.Add(general);
            BuildGeneralTab(general);

            TabPage authentication = new TabPage("Authentication");
            authentication.BackColor = Color.White;
            tabs.TabPages.Add(authentication);
            BuildAuthenticationTab(authentication);

            TabPage about = new TabPage("About");
            about.BackColor = Color.White;
            tabs.TabPages.Add(about);
            BuildAboutTab(about);
        }

        private void BuildGeneralTab(TabPage page)
        {
            TableLayoutPanel layout = CreateLayout(2, 7);
            page.Controls.Add(layout);
            layout.RowStyles.Clear();
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 34F));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 30F));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 30F));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 42F));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 24F));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 48F));
            layout.RowStyles.Add(new RowStyle(SizeType.Percent, 100F));

            TableLayoutPanel pathPanel = new TableLayoutPanel();
            pathPanel.Dock = DockStyle.Fill;
            pathPanel.ColumnCount = 2;
            pathPanel.RowCount = 1;
            pathPanel.Margin = new Padding(0);
            pathPanel.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100F));
            pathPanel.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 86F));

            _rootBox = new TextBox();
            _rootBox.Dock = DockStyle.Fill;
            _rootBox.Margin = new Padding(0, 4, 6, 4);
            pathPanel.Controls.Add(_rootBox, 0, 0);

            _browseButton = new Button();
            _browseButton.Text = "Browse...";
            _browseButton.Dock = DockStyle.Fill;
            _browseButton.Margin = new Padding(0, 2, 0, 2);
            _browseButton.Click += BrowseButtonClick;
            pathPanel.Controls.Add(_browseButton, 1, 0);

            AddRow(layout, 0, "Root path:", pathPanel);

            _portBox = new NumericUpDown();
            _portBox.Minimum = 1;
            _portBox.Maximum = 65535;
            _portBox.Value = 2121;
            _portBox.Width = 110;
            _portBox.Margin = new Padding(0, 4, 0, 4);
            AddRow(layout, 1, "Port:", _portBox);

            TableLayoutPanel statusPanel = new TableLayoutPanel();
            statusPanel.Dock = DockStyle.Fill;
            statusPanel.ColumnCount = 2;
            statusPanel.RowCount = 1;
            statusPanel.Margin = new Padding(0);
            statusPanel.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 22F));
            statusPanel.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100F));

            _statusImage = new PictureBox();
            _statusImage.Size = new Size(16, 16);
            _statusImage.SizeMode = PictureBoxSizeMode.CenterImage;
            _statusImage.Margin = new Padding(2, 5, 4, 4);
            statusPanel.Controls.Add(_statusImage, 0, 0);

            _statusLabel = new Label();
            _statusLabel.Text = "FTP Server is OFF";
            _statusLabel.Dock = DockStyle.Fill;
            _statusLabel.TextAlign = ContentAlignment.MiddleLeft;
            _statusLabel.AutoEllipsis = true;
            _statusLabel.Margin = new Padding(0, 3, 0, 3);
            statusPanel.Controls.Add(_statusLabel, 1, 0);
            AddRow(layout, 2, "Status:", statusPanel);

            FlowLayoutPanel buttons = new FlowLayoutPanel();
            buttons.Dock = DockStyle.Fill;
            buttons.FlowDirection = FlowDirection.LeftToRight;
            buttons.WrapContents = false;
            buttons.Margin = new Padding(0, 6, 0, 2);

            _startButton = new Button();
            _startButton.Text = "Start Server";
            _startButton.Width = 120;
            _startButton.Height = 30;
            _startButton.Margin = new Padding(0, 0, 8, 0);
            _startButton.Click += StartButtonClick;
            buttons.Controls.Add(_startButton);

            _stopButton = new Button();
            _stopButton.Text = "Stop Server";
            _stopButton.Width = 120;
            _stopButton.Height = 30;
            _stopButton.Enabled = false;
            _stopButton.Click += StopButtonClick;
            buttons.Controls.Add(_stopButton);
            AddRow(layout, 3, "", buttons);

            FlowLayoutPanel addressPanel = new FlowLayoutPanel();
            addressPanel.Dock = DockStyle.Fill;
            addressPanel.FlowDirection = FlowDirection.LeftToRight;
            addressPanel.WrapContents = false;
            addressPanel.Margin = new Padding(0, 2, 0, 0);

            _addressLabel = new Label();
            _addressLabel.AutoSize = true;
            _addressLabel.ForeColor = Color.FromArgb(90, 90, 95);
            _addressLabel.Margin = new Padding(0, 4, 8, 0);
            addressPanel.Controls.Add(_addressLabel);

            _copyLink = new LinkLabel();
            _copyLink.Text = "Copy FTP link";
            _copyLink.AutoSize = true;
            _copyLink.Margin = new Padding(0, 3, 0, 0);
            _copyLink.LinkClicked += CopyLinkClicked;
            addressPanel.Controls.Add(_copyLink);
            AddRow(layout, 4, "", addressPanel);

            Label tip = new Label();
            tip.Text = "Tip: on first start, allow QuickFTP through Windows Firewall for private networks so other devices can connect.";
            tip.ForeColor = Color.FromArgb(110, 110, 115);
            tip.Dock = DockStyle.Fill;
            tip.AutoSize = false;
            tip.Height = 42;
            tip.Margin = new Padding(0, 8, 0, 0);
            layout.Controls.Add(tip, 0, 5);
            layout.SetColumnSpan(tip, 2);

        }

        private void BuildAuthenticationTab(TabPage page)
        {
            TableLayoutPanel layout = CreateLayout(2, 6);
            page.Controls.Add(layout);
            layout.RowStyles.Clear();
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 30F));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 30F));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 30F));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 30F));
            layout.RowStyles.Add(new RowStyle(SizeType.Percent, 100F));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 0F));

            _userNameBox = new TextBox();
            _userNameBox.Dock = DockStyle.Fill;
            _userNameBox.Margin = new Padding(0, 4, 0, 4);
            AddRow(layout, 0, "Username:", _userNameBox);

            _passwordBox = new TextBox();
            _passwordBox.Dock = DockStyle.Fill;
            _passwordBox.UseSystemPasswordChar = true;
            _passwordBox.Margin = new Padding(0, 4, 0, 4);
            AddRow(layout, 1, "Password:", _passwordBox);

            _userAccessBox = CreateAccessComboBox();
            AddRow(layout, 2, "Access:", _userAccessBox);

            _anonymousAccessBox = CreateAccessComboBox();
            AddRow(layout, 3, "Anonymous users access:", _anonymousAccessBox);

            Label note = new Label();
            note.Text = "Access controls are enforced for every FTP command. Read Only blocks uploads, deletion, renaming and folder changes.\r\n\r\nCredentials are stored locally with Windows DPAPI.";
            note.ForeColor = Color.FromArgb(110, 110, 115);
            note.Dock = DockStyle.Fill;
            note.AutoSize = false;
            note.Height = 90;
            note.Margin = new Padding(0, 14, 0, 0);
            layout.Controls.Add(note, 0, 4);
            layout.SetColumnSpan(note, 2);

        }

        private void BuildAboutTab(TabPage page)
        {
            TableLayoutPanel layout = new TableLayoutPanel();
            layout.Dock = DockStyle.Fill;
            layout.Padding = new Padding(18, 16, 18, 14);
            layout.ColumnCount = 1;
            layout.RowCount = 7;
            layout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100F));
            page.Controls.Add(layout);

            Label title = new Label();
            title.Text = "QuickFTP Server";
            title.Font = new Font("Segoe UI Semibold", 15F, FontStyle.Bold, GraphicsUnit.Point);
            title.AutoSize = true;
            title.Margin = new Padding(0, 0, 0, 2);
            layout.Controls.Add(title, 0, 0);

            Label version = new Label();
            version.Text = "v1.0.8 - Windows edition";
            version.ForeColor = Color.FromArgb(100, 100, 105);
            version.AutoSize = false;
            version.Dock = DockStyle.Fill;
            version.TextAlign = ContentAlignment.MiddleLeft;
            version.Margin = new Padding(0, 0, 0, 14);
            layout.Controls.Add(version, 0, 1);

            Label description = new Label();
            description.Text = "A small FTP server for sharing a folder with devices on your local network.\r\n\r\nThis is a Windows reimplementation of QuickFTP Server for macOS by Leszek Szary. It is not affiliated with the original developer.";
            description.AutoSize = false;
            description.Dock = DockStyle.Fill;
            description.Height = 90;
            description.Margin = new Padding(0, 0, 0, 10);
            layout.Controls.Add(description, 0, 2);

            Label attribution = new Label();
            attribution.Text = "Original concept and interface: QuickFTP Server (macOS), Copyright (c) 2019 LS.";
            attribution.ForeColor = Color.FromArgb(110, 110, 115);
            attribution.AutoSize = false;
            attribution.Dock = DockStyle.Fill;
            attribution.Height = 36;
            layout.Controls.Add(attribution, 0, 3);

            _moreAppsLink = new LinkLabel();
            _moreAppsLink.Text = "More Apps by the original developer";
            _moreAppsLink.AutoSize = true;
            _moreAppsLink.Margin = new Padding(0, 6, 0, 0);
            _moreAppsLink.LinkClicked += MoreAppsClicked;
            layout.Controls.Add(_moreAppsLink, 0, 4);

            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 32F));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 22F));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 90F));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 36F));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 24F));
            layout.RowStyles.Add(new RowStyle(SizeType.Percent, 100F));
        }

        private static TableLayoutPanel CreateLayout(int columns, int rows)
        {
            TableLayoutPanel layout = new TableLayoutPanel();
            layout.Dock = DockStyle.Fill;
            layout.Padding = new Padding(16, 14, 16, 12);
            layout.ColumnCount = columns;
            layout.RowCount = rows;
            layout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 128F));
            layout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100F));
            return layout;
        }

        private static void AddRow(TableLayoutPanel layout, int row, string labelText, Control control)
        {
            Label label = new Label();
            label.Text = labelText;
            label.Dock = DockStyle.Fill;
            label.TextAlign = ContentAlignment.MiddleLeft;
            label.Margin = new Padding(0, 4, 8, 4);
            layout.Controls.Add(label, 0, row);

            control.Dock = DockStyle.Fill;
            layout.Controls.Add(control, 1, row);
        }

        private static ComboBox CreateAccessComboBox()
        {
            ComboBox box = new ComboBox();
            box.DropDownStyle = ComboBoxStyle.DropDownList;
            box.Items.Add("Disabled");
            box.Items.Add("Read Only");
            box.Items.Add("Read & Write");
            box.SelectedIndex = 0;
            box.Margin = new Padding(0, 4, 0, 4);
            return box;
        }

        private void ApplySettingsToInterface()
        {
            _rootBox.Text = string.IsNullOrEmpty(_settings.RootPath)
                ? Environment.GetFolderPath(Environment.SpecialFolder.UserProfile)
                : _settings.RootPath;

            int port = _settings.Port;
            if (port < 1 || port > 65535)
            {
                port = 2121;
            }
            _portBox.Value = port;
            _userNameBox.Text = _settings.UserName ?? "";
            _passwordBox.Text = _settings.Password ?? "";
            _userAccessBox.SelectedIndex = (int)_settings.UserAccess;
            _anonymousAccessBox.SelectedIndex = (int)_settings.AnonymousAccess;
        }

        private AppSettings ReadSettingsFromInterface()
        {
            // 界面值是唯一输入源；组合框索引直接映射到 AccessMode 枚举。
            AppSettings settings = new AppSettings();
            settings.RootPath = _rootBox.Text.Trim();
            settings.Port = (int)_portBox.Value;
            settings.UserName = _userNameBox.Text.Trim();
            settings.Password = _passwordBox.Text;
            settings.UserAccess = (AccessMode)_userAccessBox.SelectedIndex;
            settings.AnonymousAccess = (AccessMode)_anonymousAccessBox.SelectedIndex;
            return settings;
        }

        private void BrowseButtonClick(object sender, EventArgs e)
        {
            FolderBrowserDialog dialog = new FolderBrowserDialog();
            dialog.Description = "Select the folder to share over FTP";
            dialog.ShowNewFolderButton = true;
            if (Directory.Exists(_rootBox.Text))
            {
                dialog.SelectedPath = _rootBox.Text;
            }

            if (dialog.ShowDialog(this) == DialogResult.OK)
            {
                _rootBox.Text = dialog.SelectedPath;
            }
        }

        private void StartButtonClick(object sender, EventArgs e)
        {
            if (_server != null && _server.IsRunning)
            {
                return;
            }

            AppSettings settings = ReadSettingsFromInterface();
            if (!Directory.Exists(settings.RootPath))
            {
                // 校验顺序与原版保持一致，先提示最基础、最容易修复的问题。
                ShowValidationMessage("Please select a valid root path.");
                return;
            }

            if (settings.Port < 1 || settings.Port > 65535)
            {
                ShowValidationMessage("Please enter a valid port number.");
                return;
            }

            if (settings.UserAccess == AccessMode.Disabled && settings.AnonymousAccess == AccessMode.Disabled)
            {
                ShowValidationMessage("Please enable user access or anonymous access.");
                return;
            }

            if (settings.UserAccess != AccessMode.Disabled &&
                (settings.UserName.Length == 0 || settings.Password.Length == 0))
            {
                ShowValidationMessage("Please enter user name and password first.");
                return;
            }

            if (settings.Port < 1024)
            {
                DialogResult answer = MessageBox.Show(
                    this,
                    "Ports below 1024 may require administrator privileges.\r\n\r\nContinue with port " +
                        settings.Port.ToString() + "?",
                    "QuickFTP Server",
                    MessageBoxButtons.YesNo,
                    MessageBoxIcon.Question);
                if (answer != DialogResult.Yes)
                {
                    return;
                }
            }

            _settings = settings;
            // 先持久化再启动，避免服务器成功运行但下次启动丢失配置。
            SettingsStore.Save(settings);

            FtpConfig config = new FtpConfig();
            config.RootPath = settings.RootPath;
            config.Port = settings.Port;
            config.UserName = settings.UserName;
            config.Password = settings.Password;
            config.UserAccess = settings.UserAccess;
            config.AnonymousAccess = settings.AnonymousAccess;

            try
            {
                _server = new FtpServer();
                _server.Start(config);
                // 启动成功后锁定配置控件，防止运行期间改端口/账号却没有生效。
                UpdateRunningInterface(true);
            }
            catch (Exception ex)
            {
                if (_server != null)
                {
                    try
                    {
                        _server.Stop();
                    }
                    catch
                    {
                    }
                    _server = null;
                }

                UpdateRunningInterface(false);
                MessageBox.Show(
                    this,
                    "Error while starting server:\r\n" + ex.Message,
                    "QuickFTP Server",
                    MessageBoxButtons.OK,
                    MessageBoxIcon.Error);
            }
        }

        private void StopButtonClick(object sender, EventArgs e)
        {
            StopServer();
        }

        private void StopServer()
        {
            if (_server == null)
            {
                return;
            }

            try
            {
                _server.Stop();
            }
            catch
            {
            }

            _server = null;
            UpdateRunningInterface(false);
        }

        private void UpdateRunningInterface(bool running)
        {
            // 运行期间只保留 Stop/复制地址可用，其余配置控件禁用。
            _startButton.Enabled = !running;
            _stopButton.Enabled = running;
            _rootBox.Enabled = !running;
            _browseButton.Enabled = !running;
            _portBox.Enabled = !running;
            _userNameBox.Enabled = !running;
            _passwordBox.Enabled = !running;
            _userAccessBox.Enabled = !running;
            _anonymousAccessBox.Enabled = !running;

            if (running && _server != null)
            {
                string[] addresses = FtpServer.GetLocalIPv4Addresses();
                string joined = string.Join(", ", addresses);
                // 状态行展示所有可用局域网 IPv4；复制链接使用第一个地址。
                _statusLabel.Text = "FTP Server is ON (" + joined + ")";
                _statusLabel.ForeColor = Color.FromArgb(30, 120, 60);
                _addressLabel.Text = "ftp://" + addresses[0] + ":" + _server.Port.ToString();
                _addressLabel.Visible = true;
                _copyLink.Visible = true;
                SetStatusImage(true);
            }
            else
            {
                _statusLabel.Text = "FTP Server is OFF";
                _statusLabel.ForeColor = Color.FromArgb(70, 70, 75);
                _addressLabel.Text = "";
                _addressLabel.Visible = false;
                _copyLink.Visible = false;
                SetStatusImage(false);
            }
        }

        private void SetStatusImage(bool on)
        {
            Image old = _statusImage.Image;
            _statusImage.Image = CreateStatusDot(on);
            if (old != null)
            {
                old.Dispose();
            }
        }

        private static Bitmap CreateStatusDot(bool on)
        {
            // 用简单圆点显示 ON/OFF，避免依赖额外的图标资源。
            Bitmap bitmap = new Bitmap(16, 16);
            using (Graphics graphics = Graphics.FromImage(bitmap))
            {
                graphics.SmoothingMode = SmoothingMode.AntiAlias;
                Color fill = on ? Color.FromArgb(40, 170, 80) : Color.FromArgb(215, 65, 65);
                Color border = on ? Color.FromArgb(25, 120, 55) : Color.FromArgb(170, 40, 40);
                using (SolidBrush brush = new SolidBrush(fill))
                using (Pen pen = new Pen(border))
                {
                    graphics.FillEllipse(brush, 2, 2, 11, 11);
                    graphics.DrawEllipse(pen, 2, 2, 11, 11);
                }
            }
            return bitmap;
        }

        private void CopyLinkClicked(object sender, LinkLabelLinkClickedEventArgs e)
        {
            try
            {
                if (!string.IsNullOrEmpty(_addressLabel.Text))
                {
                    Clipboard.SetText(_addressLabel.Text);
                }
            }
            catch
            {
            }
        }

        private void MoreAppsClicked(object sender, LinkLabelLinkClickedEventArgs e)
        {
            try
            {
                Process.Start(new ProcessStartInfo("https://apps.apple.com/developer/leszek-szary/id464282200"));
            }
            catch
            {
            }
        }

        private void ShowValidationMessage(string message)
        {
            MessageBox.Show(this, message, "QuickFTP Server", MessageBoxButtons.OK, MessageBoxIcon.Warning);
        }

        protected override void OnFormClosing(FormClosingEventArgs e)
        {
            // 关闭窗口即退出程序：先停止监听和会话，再保存当前界面配置。
            if (_server != null)
            {
                try
                {
                    _server.Stop();
                }
                catch
                {
                }
                _server = null;
            }

            try
            {
                SettingsStore.Save(ReadSettingsFromInterface());
            }
            catch
            {
            }

            base.OnFormClosing(e);
        }
    }
}
