//! 便携配置读写。
//!
//! 默认把 quickftp-rust.ini 放在 exe 同目录，密码用 Windows DPAPI(CurrentUser)
//! 加密后以十六进制保存。配置文件损坏时回退默认值，不阻塞程序启动。
use std::env;
use std::fs;
use std::path::PathBuf;
use std::ptr;

use crate::win32::{CryptProtectData, CryptUnprotectData, LocalFree, DATA_BLOB};

#[derive(Clone)]
pub struct Settings {
    // root/port 是服务器基础配置；两个 access 字段对应界面下拉框索引。
    pub root: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub user_access: usize,
    pub anonymous_access: usize,
}

impl Settings {
    pub fn defaults() -> Settings {
        // 默认共享当前用户目录，端口 2121，用户读写、匿名关闭。
        let root = env::var("USERPROFILE")
            .or_else(|_| env::current_dir().map(|p| p.display().to_string()))
            .unwrap_or_else(|_| "C:\\".to_string());
        Settings {
            root,
            port: 2121,
            user: "user".to_string(),
            password: String::new(),
            user_access: 2,
            anonymous_access: 0,
        }
    }
}

pub fn data_dir() -> PathBuf {
    // QUICKFTP_DATA_DIR 允许把配置放到任意便携目录。
    if let Ok(value) = env::var("QUICKFTP_DATA_DIR") {
        if !value.is_empty() {
            return PathBuf::from(value);
        }
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            return parent.to_path_buf();
        }
    }
    PathBuf::from(".")
}

pub fn settings_path() -> PathBuf {
    data_dir().join("quickftp-rust.ini")
}

pub fn load() -> Settings {
    // 使用简单的 key=value 文本格式；未知字段忽略，便于向后兼容。
    let mut settings = Settings::defaults();
    let path = settings_path();
    let text = match fs::read_to_string(&path) {
        Ok(value) => value,
        Err(_) => return settings,
    };

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, '=');
        let key = parts.next().unwrap_or("").trim();
        let value = parts.next().unwrap_or("").trim();
        match key {
            "root" => {
                let decoded = unescape(value);
                if !decoded.is_empty() {
                    settings.root = decoded;
                }
            }
            "port" => {
                if let Ok(port) = value.parse::<u16>() {
                    if port > 0 {
                        settings.port = port;
                    }
                }
            }
            "username" => settings.user = unescape(value),
            "password" => settings.password = unprotect(value),
            "userAccess" => {
                if let Ok(index) = value.parse::<usize>() {
                    if index <= 2 {
                        settings.user_access = index;
                    }
                }
            }
            "anonymousAccess" => {
                if let Ok(index) = value.parse::<usize>() {
                    if index <= 2 {
                        settings.anonymous_access = index;
                    }
                }
            }
            _ => {}
        }
    }
    settings
}

pub fn save(settings: &Settings) -> Result<(), String> {
    // 先构造完整文本再写入，减少配置只写了一半的概率。
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Create settings folder failed: {}", e))?;
    }

    let mut text = String::new();
    text.push_str("# QuickFTP Rust portable settings\n");
    text.push_str("root=");
    text.push_str(&escape(&settings.root));
    text.push('\n');
    text.push_str("port=");
    text.push_str(&settings.port.to_string());
    text.push('\n');
    text.push_str("username=");
    text.push_str(&escape(&settings.user));
    text.push('\n');
    text.push_str("password=");
    text.push_str(&protect(&settings.password));
    text.push('\n');
    text.push_str("userAccess=");
    text.push_str(&settings.user_access.to_string());
    text.push('\n');
    text.push_str("anonymousAccess=");
    text.push_str(&settings.anonymous_access.to_string());
    text.push('\n');

    fs::write(&path, text).map_err(|e| format!("Save settings failed: {}", e))
}

fn escape(value: &str) -> String {
    // INI 不支持多行值，因此把反斜杠/换行转义成可见字符序列。
    let mut result = String::new();
    for ch in value.chars() {
        match ch {
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            _ => result.push(ch),
        }
    }
    result
}

fn unescape(value: &str) -> String {
    let mut result = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('\\') => result.push('\\'),
                Some(other) => result.push(other),
                None => result.push('\\'),
            }
        } else {
            result.push(ch);
        }
    }
    result
}

fn protect(value: &str) -> String {
    // 空密码不写入密文；非空密码用 DPAPI 绑定当前 Windows 用户。
    if value.is_empty() {
        return String::new();
    }
    let bytes = value.as_bytes();
    let mut input = bytes.to_vec();
    let in_blob = DATA_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_mut_ptr(),
    };
    let mut out_blob = DATA_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    let ok = unsafe {
        // DATA_BLOB 只借用 Rust Vec 的内存，调用返回后立即复制并 LocalFree。
        CryptProtectData(
            &in_blob,
            ptr::null(),
            ptr::null(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            &mut out_blob,
        )
    };
    if ok == 0 || out_blob.pbData.is_null() {
        return String::new();
    }
    let encrypted = unsafe {
        let slice = std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize);
        slice.to_vec()
    };
    unsafe {
        LocalFree(out_blob.pbData as *mut _);
    }
    hex_encode(&encrypted)
}

fn unprotect(value: &str) -> String {
    // 当前配置只保存 DPAPI 十六进制密文；无法解密时返回空密码。
    if value.is_empty() {
        return String::new();
    }
    let encrypted = match hex_decode(value) {
        Some(bytes) => bytes,
        None => return String::new(),
    };
    let mut input = encrypted;
    let in_blob = DATA_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_mut_ptr(),
    };
    let mut out_blob = DATA_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &in_blob,
            ptr::null_mut(),
            ptr::null(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            &mut out_blob,
        )
    };
    if ok == 0 || out_blob.pbData.is_null() {
        return String::new();
    }
    let clear = unsafe {
        let slice = std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize);
        slice.to_vec()
    };
    unsafe {
        LocalFree(out_blob.pbData as *mut _);
    }
    String::from_utf8_lossy(&clear).to_string()
}

fn hex_encode(bytes: &[u8]) -> String {
    // 自己实现十六进制编解码，避免为了一个小功能引入 crate。
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        result.push(DIGITS[(byte >> 4) as usize] as char);
        result.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    result
}

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    let bytes = value.as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    let mut result = Vec::with_capacity(bytes.len() / 2);
    let mut index = 0;
    while index < bytes.len() {
        let high = hex_value(bytes[index])?;
        let low = hex_value(bytes[index + 1])?;
        result.push((high << 4) | low);
        index += 2;
    }
    Some(result)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

