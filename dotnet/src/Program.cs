using System;
using System.Windows.Forms;

namespace QuickFTP
{
    /// <summary>
    /// 程序入口。WinForms/剪贴板要求 STA 线程，因此必须保留 [STAThread]。
    /// </summary>
    internal static class Program
    {
        [STAThread]
        private static void Main()
        {
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            Application.Run(new MainForm());
        }
    }
}
