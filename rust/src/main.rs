//! QuickFTP Rust 版的 Win32 前端。
//!
//! 程序只有一个主窗口。窗口句柄统一放在 `STATE` 中，WndProc 负责把按钮、
//! 定时器和关闭消息分发给对应处理函数；FTP 服务端由 `ftp` 模块在后台线程运行。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(non_snake_case)]

mod ftp;
mod settings;
mod win32;

use std::fs;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::Ordering;

use win32::*;

const ID_START: usize = 1001;
const ID_STOP: usize = 1002;
const ID_BROWSE: usize = 1003;
const ID_COPY: usize = 1004;
const TIMER_ID: usize = 1;

struct AppState {
    // 保存控件句柄，避免每次消息处理都重新查找子窗口。
    root_hwnd: HWND,
    port_hwnd: HWND,
    status_hwnd: HWND,
    address_hwnd: HWND,
    user_hwnd: HWND,
    password_hwnd: HWND,
    user_access_hwnd: HWND,
    anonymous_access_hwnd: HWND,
    start_hwnd: HWND,
    stop_hwnd: HWND,
    browse_hwnd: HWND,
    copy_hwnd: HWND,
    settings: settings::Settings,
    server: Option<ftp::Server>,
    last_status: String,
    last_address: String,
}

static mut STATE: Option<Box<AppState>> = None;
// 说明：Win32 消息循环和控件回调都在主线程执行，STATE 只在主线程读写；
// FTP 后台线程只访问 ftp::Server 内部的 Arc/原子状态，不访问该全局。

fn main() {
    unsafe {
        // 注册窗口类并进入标准 GetMessage/DispatchMessage 消息循环。
        let hinstance = GetModuleHandleW(ptr::null());
        let class_name = wide("QuickFTPRustWindow");
        let window_class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: LoadIconW(ptr::null_mut(), IDI_APPLICATION),
            hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_WINDOW + 1) as usize as HBRUSH,
            lpszMenuName: ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&window_class);

        let title = wide("QuickFTP Server - Rust portable");
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPED_FIXED,
            100,
            100,
            560,
            540,
            ptr::null_mut(),
            ptr::null_mut(),
            hinstance,
            ptr::null_mut(),
        );
        if hwnd.is_null() {
            message_box(
                ptr::null_mut(),
                "Cannot create the main window.",
                "QuickFTP Server",
                MB_OK | MB_ICONERROR,
            );
            return;
        }

        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);

        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

extern "system" fn wnd_proc(hwnd: HWND, message: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match message {
            WM_CREATE => {
                create_interface(hwnd);
                0
            }
            WM_COMMAND => {
                // 控件 ID 在低 16 位，通知码在高 16 位；按钮只处理 BN_CLICKED。
                let id = (wparam & 0xffff) as usize;
                let notification = ((wparam >> 16) & 0xffff) as u16;
                if notification == BN_CLICKED {
                    match id {
                        ID_START => start_button_clicked(hwnd),
                        ID_STOP => stop_button_clicked(hwnd),
                        ID_BROWSE => browse_button_clicked(hwnd),
                        ID_COPY => copy_button_clicked(hwnd),
                        _ => {}
                    }
                }
                0
            }
            WM_TIMER => {
                // 用 500ms 定时器轮询后台服务器状态，避免从工作线程直接操作 UI。
                update_runtime_interface();
                0
            }
            WM_CLOSE => {
                shutdown_application();
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                shutdown_application();
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

fn create_interface(hwnd: HWND) {
    // 采用固定坐标布局，保证旧版 Windows 和不同 DPI 下仍能显示完整控件。
    let loaded = settings::load();
    let font = stock_font();

    let title = create_control(
        hwnd,
        "STATIC",
        "QuickFTP Server",
        SS_LEFT,
        0,
        16,
        12,
        500,
        24,
        0,
    );
    let subtitle = create_control(
        hwnd,
        "STATIC",
        "Portable Rust edition - plain FTP for Windows 7 / 8 / 10 / 11",
        SS_LEFT,
        0,
        16,
        36,
        500,
        18,
        0,
    );

    let server_group = create_control(
        hwnd,
        "BUTTON",
        "Server",
        BS_GROUPBOX,
        0,
        12,
        58,
        520,
        190,
        0,
    );
    let auth_group = create_control(
        hwnd,
        "BUTTON",
        "Authentication",
        BS_GROUPBOX,
        0,
        12,
        258,
        520,
        170,
        0,
    );

    let root_label = create_control(
        hwnd,
        "STATIC",
        "Root path:",
        SS_LEFT,
        0,
        24,
        86,
        80,
        20,
        0,
    );
    let root_edit = create_control(
        hwnd,
        "EDIT",
        "",
        ES_AUTOHSCROLL | WS_TABSTOP,
        WS_EX_CLIENTEDGE,
        110,
        82,
        320,
        24,
        0,
    );
    let browse = create_control(
        hwnd,
        "BUTTON",
        "Browse...",
        WS_TABSTOP,
        0,
        440,
        82,
        80,
        24,
        ID_BROWSE,
    );

    let port_label = create_control(
        hwnd,
        "STATIC",
        "Port:",
        SS_LEFT,
        0,
        24,
        118,
        80,
        20,
        0,
    );
    let port_edit = create_control(
        hwnd,
        "EDIT",
        "",
        ES_AUTOHSCROLL | ES_NUMBER | WS_TABSTOP,
        WS_EX_CLIENTEDGE,
        110,
        114,
        90,
        24,
        0,
    );

    let status_label = create_control(
        hwnd,
        "STATIC",
        "Status:",
        SS_LEFT,
        0,
        24,
        150,
        80,
        20,
        0,
    );
    let status = create_control(
        hwnd,
        "STATIC",
        "FTP Server is OFF",
        SS_LEFT,
        0,
        110,
        150,
        410,
        20,
        0,
    );

    let start = create_control(
        hwnd,
        "BUTTON",
        "Start Server",
        WS_TABSTOP,
        0,
        110,
        176,
        120,
        30,
        ID_START,
    );
    let stop = create_control(
        hwnd,
        "BUTTON",
        "Stop Server",
        WS_TABSTOP,
        0,
        240,
        176,
        120,
        30,
        ID_STOP,
    );
    let copy = create_control(
        hwnd,
        "BUTTON",
        "Copy FTP link",
        WS_TABSTOP,
        0,
        370,
        176,
        120,
        30,
        ID_COPY,
    );
    let address = create_control(
        hwnd,
        "STATIC",
        "",
        SS_LEFT,
        0,
        110,
        212,
        410,
        20,
        0,
    );

    let user_label = create_control(
        hwnd,
        "STATIC",
        "Username:",
        SS_LEFT,
        0,
        24,
        286,
        80,
        20,
        0,
    );
    let user_edit = create_control(
        hwnd,
        "EDIT",
        "",
        ES_AUTOHSCROLL | WS_TABSTOP,
        WS_EX_CLIENTEDGE,
        110,
        282,
        240,
        24,
        0,
    );
    let password_label = create_control(
        hwnd,
        "STATIC",
        "Password:",
        SS_LEFT,
        0,
        24,
        318,
        80,
        20,
        0,
    );
    let password_edit = create_control(
        hwnd,
        "EDIT",
        "",
        ES_AUTOHSCROLL | ES_PASSWORD | WS_TABSTOP,
        WS_EX_CLIENTEDGE,
        110,
        314,
        240,
        24,
        0,
    );
    let user_access_label = create_control(
        hwnd,
        "STATIC",
        "Access:",
        SS_LEFT,
        0,
        24,
        350,
        80,
        20,
        0,
    );
    let user_access = create_control(
        hwnd,
        "COMBOBOX",
        "",
        CBS_DROPDOWNLIST | CBS_HASSTRINGS | WS_VSCROLL | WS_TABSTOP,
        WS_EX_CLIENTEDGE,
        110,
        346,
        190,
        200,
        0,
    );
    let anonymous_access_label = create_control(
        hwnd,
        "STATIC",
        "Anonymous:",
        SS_LEFT,
        0,
        24,
        382,
        110,
        20,
        0,
    );
    let anonymous_access = create_control(
        hwnd,
        "COMBOBOX",
        "",
        CBS_DROPDOWNLIST | CBS_HASSTRINGS | WS_VSCROLL | WS_TABSTOP,
        WS_EX_CLIENTEDGE,
        140,
        378,
        190,
        200,
        0,
    );

    let about = create_control(
        hwnd,
        "STATIC",
        "Based on QuickFTP Server for macOS by Leszek Szary. Plain FTP only; no TLS/FTPS.",
        SS_LEFT,
        0,
        16,
        440,
        520,
        40,
        0,
    );

    for &control in [
        title,
        subtitle,
        server_group,
        auth_group,
        root_label,
        root_edit,
        browse,
        port_label,
        port_edit,
        status_label,
        status,
        start,
        stop,
        copy,
        address,
        user_label,
        user_edit,
        password_label,
        password_edit,
        user_access_label,
        user_access,
        anonymous_access_label,
        anonymous_access,
        about,
    ].iter() {
        // 统一使用系统默认 GUI 字体，避免额外字体资源和 DPI 缩放依赖。
        set_font(control, font);
    }

    set_combo_items(user_access, &["Disabled", "Read Only", "Read & Write"], loaded.user_access.min(2));
    set_combo_items(
        anonymous_access,
        &["Disabled", "Read Only", "Read & Write"],
        loaded.anonymous_access.min(2),
    );

    set_text(root_edit, &loaded.root);
    set_text(port_edit, &loaded.port.to_string());
    set_text(user_edit, &loaded.user);
    set_text(password_edit, &loaded.password);
    set_text(status, "FTP Server is OFF");

    unsafe {
        STATE = Some(Box::new(AppState {
            root_hwnd: root_edit,
            port_hwnd: port_edit,
            status_hwnd: status,
            address_hwnd: address,
            user_hwnd: user_edit,
            password_hwnd: password_edit,
            user_access_hwnd: user_access,
            anonymous_access_hwnd: anonymous_access,
            start_hwnd: start,
            stop_hwnd: stop,
            browse_hwnd: browse,
            copy_hwnd: copy,
            settings: loaded,
            server: None,
            last_status: String::new(),
            last_address: String::new(),
        }));

        SetTimer(hwnd, TIMER_ID, 500, ptr::null_mut());
        SetFocus(root_edit);
    }
    update_runtime_interface();
}

fn read_settings_from_interface() -> settings::Settings {
    let state = match unsafe { STATE.as_ref() } {
        Some(value) => value,
        None => return settings::Settings::defaults(),
    };

    settings::Settings {
        root: get_text(state.root_hwnd).trim().to_string(),
        port: get_text(state.port_hwnd).trim().parse::<u16>().unwrap_or(0),
        user: get_text(state.user_hwnd).trim().to_string(),
        password: get_text(state.password_hwnd),
        user_access: combo_index(state.user_access_hwnd),
        anonymous_access: combo_index(state.anonymous_access_hwnd),
    }
}

fn start_button_clicked(hwnd: HWND) {
    // 界面层先做完整校验，再创建不可变配置快照交给后台线程。
    let settings = read_settings_from_interface();
    if settings.root.is_empty() || !Path::new(&settings.root).is_dir() {
        message_box(
            hwnd,
            "Please select a valid root path.",
            "QuickFTP Server",
            MB_OK | MB_ICONWARNING,
        );
        return;
    }
    if settings.port == 0 {
        message_box(
            hwnd,
            "Please enter a valid port number.",
            "QuickFTP Server",
            MB_OK | MB_ICONWARNING,
        );
        return;
    }
    if settings.user_access == 0 && settings.anonymous_access == 0 {
        message_box(
            hwnd,
            "Please enable user access or anonymous access.",
            "QuickFTP Server",
            MB_OK | MB_ICONWARNING,
        );
        return;
    }
    if settings.user_access != 0 && (settings.user.is_empty() || settings.password.is_empty()) {
        message_box(
            hwnd,
            "Please enter user name and password first.",
            "QuickFTP Server",
            MB_OK | MB_ICONWARNING,
        );
        return;
    }
    if settings.port < 1024 {
        // Windows 不强制要求管理员绑定低端口，但防火墙/权限可能不同，先让用户确认。
        let answer = message_box(
            hwnd,
            "Ports below 1024 may require administrator privileges.\r\n\r\nContinue?",
            "QuickFTP Server",
            MB_YESNO | MB_ICONWARNING,
        );
        if answer != IDYES {
            return;
        }
    }

    let root_canon = match fs::canonicalize(&settings.root) {
        Ok(value) => value,
        Err(_) => {
            message_box(
                hwnd,
                "Please select a valid root path.",
                "QuickFTP Server",
                MB_OK | MB_ICONWARNING,
            );
            return;
        }
    };
    if settings::save(&settings).is_err() {
        message_box(
            hwnd,
            "Settings could not be saved, but the server will still start.",
            "QuickFTP Server",
            MB_OK | MB_ICONWARNING,
        );
    }

    if let Some(state) = unsafe { STATE.as_mut() } {
        if let Some(mut old_server) = state.server.take() {
            old_server.stop();
        }
        let config = ftp::Config {
            root: PathBuf::from(&settings.root),
            root_canon,
            user: settings.user.clone(),
            password: settings.password.clone(),
            user_access: ftp::Access::from_index(settings.user_access),
            anonymous_access: ftp::Access::from_index(settings.anonymous_access),
        };
        match ftp::Server::start(config, settings.port) {
            Ok(server) => {
                state.settings = settings;
                state.server = Some(server);
                update_runtime_interface();
            }
            Err(error) => {
                message_box(
                    hwnd,
                    &format!("Error while starting server:\r\n{}", error),
                    "QuickFTP Server",
                    MB_OK | MB_ICONERROR,
                );
            }
        }
    }
}

fn stop_button_clicked(_hwnd: HWND) {
    stop_server();
    update_runtime_interface();
}

fn stop_server() {
    // 先从界面状态中取出 Server，再停止并等待监听线程退出。
    if let Some(state) = unsafe { STATE.as_mut() } {
        if let Some(mut server) = state.server.take() {
            server.stop();
        }
    }
}

fn browse_button_clicked(hwnd: HWND) {
    if let Some(state) = unsafe { STATE.as_mut() } {
        if let Some(path) = browse_for_folder(hwnd) {
            set_text(state.root_hwnd, &path);
        }
    }
}

fn copy_button_clicked(hwnd: HWND) {
    if let Some(state) = unsafe { STATE.as_ref() } {
        let address = get_text(state.address_hwnd);
        if !address.is_empty() {
            let _ = copy_text_to_clipboard(hwnd, &address);
        }
    }
}

fn update_runtime_interface() {
    // 运行期间锁定配置控件，避免出现“界面已改但服务端仍用旧配置”的错觉。
    let state = match unsafe { STATE.as_mut() } {
        Some(value) => value,
        None => return,
    };

    let running = state
        .server
        .as_ref()
        .map(|server| server.shared.running.load(Ordering::SeqCst))
        .unwrap_or(false);

    enable(state.start_hwnd, !running);
    enable(state.stop_hwnd, running);
    enable(state.root_hwnd, !running);
    enable(state.browse_hwnd, !running);
    enable(state.port_hwnd, !running);
    enable(state.user_hwnd, !running);
    enable(state.password_hwnd, !running);
    enable(state.user_access_hwnd, !running);
    enable(state.anonymous_access_hwnd, !running);
    enable(state.copy_hwnd, running);

    let (status, address) = if running {
        let addresses = state
            .server
            .as_ref()
            .and_then(|server| server.shared.addresses.lock().ok())
            .map(|values| values.join(", "))
            .unwrap_or_else(|| ftp::primary_ipv4());
        let port = state
            .server
            .as_ref()
            .map(|server| server.shared.port)
            .unwrap_or(0);
        let first = addresses.split(',').next().unwrap_or("127.0.0.1").trim();
        // 状态栏展示所有网卡地址；复制按钮使用第一个可用 IPv4。
        (
            format!("FTP Server is ON ({})", addresses),
            format!("ftp://{}:{}", first, port),
        )
    } else {
        ("FTP Server is OFF".to_string(), String::new())
    };

    if status != state.last_status {
        set_text(state.status_hwnd, &status);
        state.last_status = status;
    }
    if address != state.last_address {
        set_text(state.address_hwnd, &address);
        state.last_address = address;
    }
}

fn shutdown_application() {
    // 窗口关闭时先停止服务器，再把当前控件值写回配置。
    unsafe {
        if let Some(mut state) = STATE.take() {
            if let Some(mut server) = state.server.take() {
                server.stop();
            }
            let current = read_settings_from_state(&state);
            let _ = settings::save(&current);
        }
    }
}

fn read_settings_from_state(state: &AppState) -> settings::Settings {
    settings::Settings {
        root: get_text(state.root_hwnd).trim().to_string(),
        port: get_text(state.port_hwnd).trim().parse::<u16>().unwrap_or(0),
        user: get_text(state.user_hwnd).trim().to_string(),
        password: get_text(state.password_hwnd),
        user_access: combo_index(state.user_access_hwnd),
        anonymous_access: combo_index(state.anonymous_access_hwnd),
    }
}
