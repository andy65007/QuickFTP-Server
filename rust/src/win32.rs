//! 最小化的 Win32 FFI 绑定。
//!
//! 这里只声明 QuickFTP 实际使用的 user32/gdi32/kernel32/shell32/ole32/crypt32
//! 函数，不引入任何 crate。所有 `unsafe` 调用的参数都来自本程序创建的窗口、
//! UTF-16 字符串或系统分配的内存，调用约定统一为 Windows 的 `system` ABI。
#![allow(non_snake_case)]

use std::os::raw::{c_int, c_void};
use std::ptr;

pub type BOOL = c_int;
// 基础 Win32 句柄在 Rust 中通常用裸指针表示；具体含义由 API 决定。
pub type DWORD = u32;
pub type UINT = u32;
pub type WPARAM = usize;
pub type LPARAM = isize;
pub type LRESULT = isize;
pub type HWND = *mut c_void;
pub type HINSTANCE = *mut c_void;
pub type HMENU = *mut c_void;
pub type HICON = *mut c_void;
pub type HCURSOR = *mut c_void;
pub type HBRUSH = *mut c_void;
pub type HFONT = *mut c_void;
pub type HANDLE = *mut c_void;
pub type WNDPROC = extern "system" fn(HWND, UINT, WPARAM, LPARAM) -> LRESULT;

pub const WM_CREATE: UINT = 0x0001;
// WM_*、WS_*、ES_* 等常量直接采用 Windows SDK 的数值，避免运行时依赖。
pub const WM_DESTROY: UINT = 0x0002;
pub const WM_CLOSE: UINT = 0x0010;
pub const WM_SETFONT: UINT = 0x0030;
pub const WM_COMMAND: UINT = 0x0111;
pub const WM_TIMER: UINT = 0x0113;

pub const WS_CHILD: DWORD = 0x40000000;
pub const WS_VISIBLE: DWORD = 0x10000000;
pub const WS_TABSTOP: DWORD = 0x00010000;
pub const WS_VSCROLL: DWORD = 0x00200000;
pub const WS_CAPTION: DWORD = 0x00C00000;
pub const WS_SYSMENU: DWORD = 0x00080000;
pub const WS_MINIMIZEBOX: DWORD = 0x00020000;
pub const WS_OVERLAPPED_FIXED: DWORD = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
pub const WS_EX_CLIENTEDGE: DWORD = 0x00000200;
pub const ES_AUTOHSCROLL: DWORD = 0x0080;
pub const ES_PASSWORD: DWORD = 0x0020;
pub const ES_NUMBER: DWORD = 0x2000;
pub const BS_GROUPBOX: DWORD = 0x00000007;
pub const CBS_DROPDOWNLIST: DWORD = 0x0003;
pub const CBS_HASSTRINGS: DWORD = 0x0200;
pub const SS_LEFT: DWORD = 0x00000000;

pub const SW_SHOW: c_int = 5;
pub const COLOR_WINDOW: c_int = 5;
pub const DEFAULT_GUI_FONT: c_int = 17;

pub const BN_CLICKED: u16 = 0;
pub const CB_ADDSTRING: UINT = 0x0143;
pub const CB_GETCURSEL: UINT = 0x0147;
pub const CB_SETCURSEL: UINT = 0x014E;

pub const MB_OK: UINT = 0x00000000;
pub const MB_YESNO: UINT = 0x00000004;
pub const MB_ICONERROR: UINT = 0x00000010;
pub const MB_ICONWARNING: UINT = 0x00000030;
pub const IDYES: c_int = 6;

pub const CF_UNICODETEXT: UINT = 13;
pub const GMEM_MOVEABLE: UINT = 0x0002;
pub const BIF_RETURNONLYFSDIRS: u32 = 0x0001;

pub const IDI_APPLICATION: *const u16 = 32512 as *const u16;
pub const IDC_ARROW: *const u16 = 32512 as *const u16;

#[repr(C)]
pub struct WNDCLASSW {
    // 必须与 Windows 的 WNDCLASSW 内存布局完全一致，字段顺序不能调整。
    pub style: UINT,
    pub lpfnWndProc: Option<WNDPROC>,
    pub cbClsExtra: c_int,
    pub cbWndExtra: c_int,
    pub hInstance: HINSTANCE,
    pub hIcon: HICON,
    pub hCursor: HCURSOR,
    pub hbrBackground: HBRUSH,
    pub lpszMenuName: *const u16,
    pub lpszClassName: *const u16,
}

#[repr(C)]
pub struct POINT {
    pub x: c_int,
    pub y: c_int,
}

#[repr(C)]
pub struct MSG {
    // 64 位 MSG 末尾多一个 lPrivate 字段；32 位布局由 cfg 自动选择。
    pub hwnd: HWND,
    pub message: UINT,
    pub wParam: WPARAM,
    pub lParam: LPARAM,
    pub time: DWORD,
    pub pt: POINT,
    #[cfg(target_pointer_width = "64")]
    pub lPrivate: DWORD,
}

#[repr(C)]
pub struct BROWSEINFOW {
    pub hwndOwner: HWND,
    pub pidlRoot: *mut c_void,
    pub pszDisplayName: *mut u16,
    pub lpszTitle: *const u16,
    pub ulFlags: u32,
    pub lpfn: *mut c_void,
    pub lParam: LPARAM,
    pub iImage: c_int,
}

#[repr(C)]
pub struct DATA_BLOB {
    pub cbData: DWORD,
    pub pbData: *mut u8,
}

#[link(name = "user32")]
extern "system" {
    // 窗口、控件、消息循环和剪贴板相关 API。
    pub fn RegisterClassW(lpWndClass: *const WNDCLASSW) -> u16;
    pub fn CreateWindowExW(
        dwExStyle: DWORD,
        lpClassName: *const u16,
        lpWindowName: *const u16,
        dwStyle: DWORD,
        X: c_int,
        Y: c_int,
        nWidth: c_int,
        nHeight: c_int,
        hWndParent: HWND,
        hMenu: HMENU,
        hInstance: HINSTANCE,
        lpParam: *mut c_void,
    ) -> HWND;
    pub fn DefWindowProcW(hWnd: HWND, Msg: UINT, wParam: WPARAM, lParam: LPARAM) -> LRESULT;
    pub fn DestroyWindow(hWnd: HWND) -> BOOL;
    pub fn ShowWindow(hWnd: HWND, nCmdShow: c_int) -> BOOL;
    pub fn UpdateWindow(hWnd: HWND) -> BOOL;
    pub fn GetMessageW(lpMsg: *mut MSG, hWnd: HWND, wMsgFilterMin: UINT, wMsgFilterMax: UINT) -> BOOL;
    pub fn TranslateMessage(lpMsg: *const MSG) -> BOOL;
    pub fn DispatchMessageW(lpMsg: *const MSG) -> LRESULT;
    pub fn PostQuitMessage(nExitCode: c_int);
    pub fn SendMessageW(hWnd: HWND, Msg: UINT, wParam: WPARAM, lParam: LPARAM) -> LRESULT;
    pub fn SetWindowTextW(hWnd: HWND, lpString: *const u16) -> BOOL;
    pub fn GetWindowTextW(hWnd: HWND, lpString: *mut u16, nMaxCount: c_int) -> c_int;
    pub fn GetWindowTextLengthW(hWnd: HWND) -> c_int;
    pub fn EnableWindow(hWnd: HWND, bEnable: BOOL) -> BOOL;
    pub fn SetTimer(hWnd: HWND, nIDEvent: usize, uElapse: UINT, lpTimerFunc: *mut c_void) -> usize;
    pub fn MessageBoxW(hWnd: HWND, lpText: *const u16, lpCaption: *const u16, uType: UINT) -> c_int;
    pub fn LoadIconW(hInstance: HINSTANCE, lpIconName: *const u16) -> HICON;
    pub fn LoadCursorW(hInstance: HINSTANCE, lpCursorName: *const u16) -> HCURSOR;
    pub fn GetModuleHandleW(lpModuleName: *const u16) -> HINSTANCE;
    pub fn SetFocus(hWnd: HWND) -> HWND;
    pub fn OpenClipboard(hWndNewOwner: HWND) -> BOOL;
    pub fn EmptyClipboard() -> BOOL;
    pub fn SetClipboardData(uFormat: UINT, hMem: HANDLE) -> HANDLE;
    pub fn CloseClipboard() -> BOOL;
}

#[link(name = "gdi32")]
extern "system" {
    pub fn GetStockObject(i: c_int) -> *mut c_void;
}

#[link(name = "kernel32")]
extern "system" {
    pub fn GlobalAlloc(uFlags: UINT, dwBytes: usize) -> HANDLE;
    pub fn GlobalLock(hMem: HANDLE) -> *mut c_void;
    pub fn GlobalUnlock(hMem: HANDLE) -> BOOL;
    pub fn GlobalFree(hMem: HANDLE) -> HANDLE;
    pub fn LocalFree(hMem: *mut c_void) -> *mut c_void;
}

#[link(name = "shell32")]
extern "system" {
    // SHBrowseForFolderW 是 XP 起可用的传统目录选择对话框。
    pub fn SHBrowseForFolderW(lpbi: *mut BROWSEINFOW) -> *mut c_void;
    pub fn SHGetPathFromIDListW(pidl: *const c_void, pszPath: *mut u16) -> BOOL;
}

#[link(name = "ole32")]
extern "system" {
    pub fn CoInitializeEx(pvReserved: *mut c_void, dwCoInit: DWORD) -> i32;
    pub fn CoUninitialize();
    pub fn CoTaskMemFree(pv: *mut c_void);
}

#[link(name = "crypt32")]
extern "system" {
    // DPAPI：用于加密 settings.ini 中的密码。
    pub fn CryptProtectData(
        pDataIn: *const DATA_BLOB,
        szDataDescr: *const u16,
        pOptionalEntropy: *const DATA_BLOB,
        pvReserved: *mut c_void,
        pPromptStruct: *mut c_void,
        dwFlags: DWORD,
        pDataOut: *mut DATA_BLOB,
    ) -> BOOL;
    pub fn CryptUnprotectData(
        pDataIn: *const DATA_BLOB,
        ppszDataDescr: *mut *mut u16,
        pOptionalEntropy: *const DATA_BLOB,
        pvReserved: *mut c_void,
        pPromptStruct: *mut c_void,
        dwFlags: DWORD,
        pDataOut: *mut DATA_BLOB,
    ) -> BOOL;
}

pub fn wide(value: &str) -> Vec<u16> {
    // Win32 的 W 版本 API 使用 NUL 结尾的 UTF-16 字符串。
    value.encode_utf16().chain(Some(0)).collect()
}

pub fn get_text(hwnd: HWND) -> String {
    // 先查询长度再分配缓冲区，避免截断控件文本。
    if hwnd.is_null() {
        return String::new();
    }
    unsafe {
        let length = GetWindowTextLengthW(hwnd);
        if length <= 0 {
            return String::new();
        }
        let mut buffer = vec![0u16; length as usize + 1];
        let copied = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as c_int);
        if copied <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buffer[..copied as usize])
    }
}

pub fn set_text(hwnd: HWND, value: &str) {
    if hwnd.is_null() {
        return;
    }
    let safe = value.replace('\0', " ");
    let text = wide(&safe);
    unsafe {
        SetWindowTextW(hwnd, text.as_ptr());
    }
}

pub fn enable(hwnd: HWND, enabled: bool) {
    if !hwnd.is_null() {
        unsafe {
            EnableWindow(hwnd, if enabled { 1 } else { 0 });
        }
    }
}

pub fn set_font(hwnd: HWND, font: HFONT) {
    if !hwnd.is_null() {
        unsafe {
            SendMessageW(hwnd, WM_SETFONT, font as WPARAM, 1);
        }
    }
}

pub fn stock_font() -> HFONT {
    unsafe { GetStockObject(DEFAULT_GUI_FONT) as HFONT }
}

pub fn create_control(
    parent: HWND,
    class_name: &str,
    text: &str,
    style: DWORD,
    ex_style: DWORD,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    id: usize,
) -> HWND {
    // 所有子控件都使用系统预定义窗口类（STATIC/EDIT/BUTTON/COMBOBOX）。
    let class_wide = wide(class_name);
    let text_wide = wide(text);
    unsafe {
        CreateWindowExW(
            ex_style,
            class_wide.as_ptr(),
            text_wide.as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            x,
            y,
            width,
            height,
            parent,
            id as HMENU,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    }
}

pub fn message_box(owner: HWND, text: &str, caption: &str, flags: UINT) -> c_int {
    let text_wide = wide(text);
    let caption_wide = wide(caption);
    unsafe { MessageBoxW(owner, text_wide.as_ptr(), caption_wide.as_ptr(), flags) }
}

pub fn set_combo_items(hwnd: HWND, items: &[&str], selected: usize) {
    if hwnd.is_null() {
        return;
    }
    unsafe {
        for item in items {
            let text = wide(item);
            SendMessageW(hwnd, CB_ADDSTRING, 0, text.as_ptr() as LPARAM);
        }
        SendMessageW(hwnd, CB_SETCURSEL, selected as WPARAM, 0);
    }
}

pub fn combo_index(hwnd: HWND) -> usize {
    if hwnd.is_null() {
        return 0;
    }
    let value = unsafe { SendMessageW(hwnd, CB_GETCURSEL, 0, 0) };
    if value < 0 {
        0
    } else {
        value as usize
    }
}

pub fn browse_for_folder(owner: HWND) -> Option<String> {
    // SHBrowseForFolderW 需要 COM 环境；S_FALSE 也算成功初始化，需要配对 CoUninitialize。
    unsafe {
        let coinit = CoInitializeEx(ptr::null_mut(), 0x00000002);
        let mut display = vec![0u16; 260];
        let title = wide("Select the folder to share over FTP");
        let mut browse = BROWSEINFOW {
            hwndOwner: owner,
            pidlRoot: ptr::null_mut(),
            pszDisplayName: display.as_mut_ptr(),
            lpszTitle: title.as_ptr(),
            ulFlags: BIF_RETURNONLYFSDIRS,
            lpfn: ptr::null_mut(),
            lParam: 0,
            iImage: 0,
        };

        let pidl = SHBrowseForFolderW(&mut browse);
        let mut result = None;
        if !pidl.is_null() {
            let mut path = vec![0u16; 32768];
            if SHGetPathFromIDListW(pidl, path.as_mut_ptr()) != 0 {
                let length = path.iter().position(|&c| c == 0).unwrap_or(path.len());
                let value = String::from_utf16_lossy(&path[..length]);
                if !value.is_empty() {
                    result = Some(value);
                }
            }
            CoTaskMemFree(pidl);
        }

        if coinit >= 0 {
            CoUninitialize();
        }
        result
    }
}

pub fn copy_text_to_clipboard(owner: HWND, value: &str) -> bool {
    // 剪贴板使用 GMEM_MOVEABLE 内存；SetClipboardData 成功后所有权归系统。
    unsafe {
        if OpenClipboard(owner) == 0 {
            return false;
        }
        let _ = EmptyClipboard();
        let mut utf16: Vec<u16> = value.encode_utf16().collect();
        utf16.push(0);
        let bytes = utf16.len() * 2;
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if handle.is_null() {
            let _ = CloseClipboard();
            return false;
        }
        let target = GlobalLock(handle) as *mut u16;
        if target.is_null() {
            let _ = GlobalFree(handle);
            let _ = CloseClipboard();
            return false;
        }
        ptr::copy_nonoverlapping(utf16.as_ptr(), target, utf16.len());
        let _ = GlobalUnlock(handle);
        if SetClipboardData(CF_UNICODETEXT, handle).is_null() {
            let _ = GlobalFree(handle);
            let _ = CloseClipboard();
            return false;
        }
        let _ = CloseClipboard();
        true
    }
}
