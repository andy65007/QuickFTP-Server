//! 纯标准库实现的 FTP 服务端。
//!
//! 一个控制连接对应一个 `Session`，命令在该会话线程内串行处理；数据连接
//! 按 PASV/EPSV 或 PORT/EPRT 临时建立。所有文件访问都必须经过 `resolve`，
//! 由它完成 FTP 路径归一化和根目录沙箱校验。
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Access {
    // 账号在 Authentication 页面选择的权限级别。
    Disabled,
    ReadOnly,
    ReadWrite,
}

impl Access {
    pub fn from_index(index: usize) -> Access {
        match index {
            1 => Access::ReadOnly,
            2 => Access::ReadWrite,
            _ => Access::Disabled,
        }
    }

    pub fn can_write(self) -> bool {
        // 只有 ReadWrite 允许 STOR/DELE/RNFR/RNTO/MKD/RMD 等修改操作。
        self == Access::ReadWrite
    }
}

#[derive(Clone)]
pub struct Config {
    // root 用于展示/拼接路径，root_canon 是 canonicalize 后的安全根目录。
    pub root: PathBuf,
    pub root_canon: PathBuf,
    pub user: String,
    pub password: String,
    pub user_access: Access,
    pub anonymous_access: Access,
}

pub struct Shared {
    // UI 通过 running/addresses 读取状态；stop 用于通知所有后台线程退出。
    pub running: AtomicBool,
    pub stop: AtomicBool,
    pub port: u16,
    pub addresses: Mutex<Vec<String>>,
    pub sessions: Mutex<Vec<(u32, TcpStream)>>,
}

pub struct Server {
    // Server 只持有监听线程句柄；具体连接由 accept_loop 派生会话线程。
    pub shared: Arc<Shared>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Server {
    pub fn start(config: Config, port: u16) -> Result<Server, String> {
        if !config.root_canon.is_dir() {
            return Err("The selected root path is not a directory.".to_string());
        }

        // 先绑定端口再启动线程，这样端口占用等错误能同步返回给 UI。
        let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port)))
            .map_err(|e| format!("Cannot open port {}: {}", port, e))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| format!("Cannot configure listener: {}", e))?;

        let addresses = local_addresses();
        let shared = Arc::new(Shared {
            running: AtomicBool::new(true),
            stop: AtomicBool::new(false),
            port,
            addresses: Mutex::new(addresses),
            sessions: Mutex::new(Vec::new()),
        });
        let worker_shared = Arc::clone(&shared);
        let thread = thread::spawn(move || {
            accept_loop(listener, config, worker_shared);
        });

        Ok(Server {
            shared,
            thread: Some(thread),
        })
    }

    pub fn stop(&mut self) {
        // 标记停止后关闭所有控制连接，唤醒可能在 read 上阻塞的会话线程。
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Ok(sessions) = self.shared.sessions.lock() {
            for &(_, ref stream) in sessions.iter() {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.shared.running.store(false, Ordering::SeqCst);
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

static NEXT_SESSION: AtomicU32 = AtomicU32::new(1);

fn accept_loop(listener: TcpListener, config: Config, shared: Arc<Shared>) {
    while !shared.stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                // 监听器是非阻塞的；accept 得到的 socket 必须显式恢复阻塞模式。
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_nodelay(true);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(300)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(60)));
                let id = NEXT_SESSION.fetch_add(1, Ordering::SeqCst);
                if let Ok(clone) = stream.try_clone() {
                    if let Ok(mut sessions) = shared.sessions.lock() {
                        sessions.push((id, clone));
                    }
                }
                let session_config = config.clone();
                let session_shared = Arc::clone(&shared);
                let _ = thread::spawn(move || {
                    let _ = run_session(stream, session_config, Arc::clone(&session_shared));
                    if let Ok(mut sessions) = session_shared.sessions.lock() {
                        sessions.retain(|&(session_id, _)| session_id != id);
                    }
                });
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => break,
        }
    }
    shared.running.store(false, Ordering::SeqCst);
}

fn run_session(stream: TcpStream, config: Config, shared: Arc<Shared>) -> io::Result<()> {
    let reader_stream = stream.try_clone()?;
    let mut session = Session {
        stream,
        reader: BufReader::new(reader_stream),
        config,
        shared,
        cwd: Vec::new(),
        logged_in: false,
        pending_user: None,
        access: Access::Disabled,
        ascii: false,
        restart: 0,
        rename_from: None,
        passive: None,
        active: None,
        closed: false,
    };
    let result = session.run();
    #[cfg(debug_assertions)]
    if let Err(ref error) = result {
        eprintln!("ftp session error: {}", error);
    }
    result
}

struct Session {
    // reader 是控制 socket 的克隆，仅用于读取；stream 用于写响应。
    stream: TcpStream,
    reader: BufReader<TcpStream>,
    config: Config,
    shared: Arc<Shared>,
    cwd: Vec<String>,
    logged_in: bool,
    pending_user: Option<String>,
    access: Access,
    ascii: bool,
    restart: u64,
    rename_from: Option<PathBuf>,
    passive: Option<TcpListener>,
    active: Option<SocketAddr>,
    closed: bool,
}

impl Session {
    fn run(&mut self) -> io::Result<()> {
        // 控制连接采用 FTP 的逐行命令/响应模型，所有响应以 CRLF 结尾。
        self.send("220 Browser Ftp Server.")?;
        loop {
            if self.closed || self.shared.stop.load(Ordering::SeqCst) {
                break;
            }
            let line = match self.read_line()? {
                Some(value) => value,
                None => break,
            };
            if line.is_empty() {
                continue;
            }
            self.handle(&line)?;
        }
        Ok(())
    }

    fn read_line(&mut self) -> io::Result<Option<String>> {
        let mut buffer = Vec::new();
        let read = self.reader.read_until(b'\n', &mut buffer)?;
        if read == 0 {
            return Ok(None);
        }
        if buffer.len() > 8192 {
            self.send("500 Command line is too long !")?;
            return Ok(Some(String::new()));
        }
        let value = String::from_utf8_lossy(&buffer)
            .trim_end_matches(|c| c == '\r' || c == '\n')
            .to_string();
        Ok(Some(value))
    }

    fn send(&mut self, line: &str) -> io::Result<()> {
        self.stream.write_all(line.as_bytes())?;
        self.stream.write_all(b"\r\n")?;
        self.stream.flush()
    }

    fn handle(&mut self, line: &str) -> io::Result<()> {
        // 命令名大小写不敏感；参数保留原始大小写，路径由 resolve 负责处理。
        let line = line.trim_start();
        let (command, argument) = match line.find(' ') {
            Some(index) => (&line[..index], line[index + 1..].trim_start()),
            None => (line, ""),
        };
        let command = command.to_ascii_uppercase();

        if !self.logged_in && !is_pre_login_command(&command) {
            // 未登录时只允许 USER/PASS/QUIT/能力协商等安全命令。
            return self.send("530 Please login with USER and PASS.");
        }

        match command.as_str() {
            "USER" => {
                if argument.is_empty() {
                    return self.send("501 Invalid number of arguments.");
                }
                self.logged_in = false;
                self.access = Access::Disabled;
                self.cwd.clear();
                self.pending_user = Some(argument.to_string());
                self.send("331 Password required for this user.")
            }
            "PASS" => self.handle_pass(argument),
            "ACCT" => {
                if self.logged_in {
                    self.send("230 User Logged In.")
                } else {
                    self.send("530 Please login with USER and PASS.")
                }
            }
            "QUIT" => {
                self.send("221 Goodbye.")?;
                self.closed = true;
                Ok(())
            }
            "NOOP" => self.send("200 NOOP Command Successful."),
            "SYST" => self.send("215 UNIX Type: L8"),
            "FEAT" => self.send_features(),
            "HELP" => self.send("500 No Help Available."),
            "CLNT" => self.send("200 Ok."),
            "OPTS" => self.send("200 Ok."),
            "ALLO" => self.send("200 Ok."),
            "TYPE" => self.handle_type(argument),
            "MODE" => self.handle_mode(argument),
            "STRU" => self.handle_structure(argument),
            "PWD" | "XPWD" => {
                let current = self.current_path();
                self.send(&format!("257 \"{}\" is current directory.", escape_ftp_path(&current)))
            }
            "CWD" | "XCWD" => self.handle_cwd(argument),
            "CDUP" | "XCUP" => {
                self.cwd.pop();
                self.send("250 CDUP command successful.")
            }
            "PASV" => self.enter_passive(false),
            "EPSV" => self.enter_passive(true),
            "PORT" => self.handle_port(argument),
            "EPRT" => self.handle_eprt(argument),
            "LIST" => self.handle_listing(argument, false, false),
            "NLST" => self.handle_listing(argument, true, false),
            "MLSD" => self.handle_listing(argument, false, true),
            "MLST" => self.handle_mlst(argument),
            "RETR" => self.handle_retr(argument),
            "STOR" => self.handle_stor(argument, false),
            "APPE" => self.handle_stor(argument, true),
            "SIZE" => self.handle_size(argument),
            "MDTM" => self.handle_mdtm(argument),
            "DELE" => self.handle_dele(argument),
            "RNFR" => self.handle_rnfr(argument),
            "RNTO" => self.handle_rnto(argument),
            "MKD" | "XMKD" => self.handle_mkd(argument),
            "RMD" | "XRMD" => self.handle_rmd(argument),
            "REST" => self.handle_rest(argument),
            "STAT" => self.handle_stat(argument),
            "ABOR" => {
                self.passive = None;
                self.active = None;
                self.send("226 ABOR command successful.")
            }
            "REIN" => {
                self.logged_in = false;
                self.pending_user = None;
                self.access = Access::Disabled;
                self.cwd.clear();
                self.send("220 Browser Ftp Server.")
            }
            _ => self.send("500 Command not understood."),
        }
    }

    fn handle_pass(&mut self, argument: &str) -> io::Result<()> {
        // 用户名先记在 pending_user，PASS 再统一校验，避免提前暴露账号是否存在。
        let pending = match self.pending_user.take() {
            Some(value) => value,
            None => return self.send("503 Bad sequence of commands."),
        };

        let is_user = self.config.user_access != Access::Disabled
            && pending == self.config.user
            && argument == self.config.password;
        let anonymous_name =
            pending.eq_ignore_ascii_case("anonymous") || pending.eq_ignore_ascii_case("ftp");
        let is_anonymous =
            self.config.anonymous_access != Access::Disabled && anonymous_name;

        if is_user {
            self.logged_in = true;
            self.access = self.config.user_access;
            self.send("230 User Logged In.")
        } else if is_anonymous {
            self.logged_in = true;
            self.access = self.config.anonymous_access;
            self.send("230 User Logged In.")
        } else {
            self.send("530 Login or password incorrect!")
        }
    }

    fn send_features(&mut self) -> io::Result<()> {
        self.send("211-Features:")?;
        self.send(" CLNT")?;
        self.send(" MDTM")?;
        self.send(" MLST type*;size*;modify*;")?;
        self.send(" REST STREAM")?;
        self.send(" SIZE")?;
        self.send(" UTF8")?;
        self.send("211 End")
    }

    fn handle_type(&mut self, argument: &str) -> io::Result<()> {
        let value = argument.trim().to_ascii_uppercase();
        if value == "A" || value == "A N" {
            self.ascii = true;
            self.send("200 ASCII transfer mode active.")
        } else if value == "I" || value == "L 8" || value == "L8" {
            self.ascii = false;
            self.send("200 Binary transfer mode active.")
        } else {
            self.send(&format!("504 \"{}\": Unsupported transfer MODE.", value))
        }
    }

    fn handle_mode(&mut self, argument: &str) -> io::Result<()> {
        let value = argument.trim().to_ascii_uppercase();
        if value == "S" {
            self.send("200 MODE set to S.")
        } else if value == "Z" {
            self.send("502 MODE Z non-implemented.")
        } else {
            self.send(&format!("504 \"{}\": Unsupported transfer MODE.", value))
        }
    }

    fn handle_structure(&mut self, argument: &str) -> io::Result<()> {
        if argument.trim().eq_ignore_ascii_case("F") {
            self.send("200 STRU Command Successful.")
        } else {
            self.send("504 STRU failled. Parametre not implemented")
        }
    }

    fn handle_cwd(&mut self, argument: &str) -> io::Result<()> {
        if argument.is_empty() {
            return self.send("501 Invalid number of arguments.");
        }
        match self.resolve(argument) {
            Ok((path, parts)) if path.is_dir() => {
                self.cwd = parts;
                self.send("250 CWD command successful.")
            }
            _ => self.send("550 No such file or directory."),
        }
    }

    fn enter_passive(&mut self, extended: bool) -> io::Result<()> {
        // 每次 PASV/EPSV 都新建临时监听端口；旧的数据端点必须先释放。
        self.passive = None;
        self.active = None;
        let listener = match TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 0))) {
            Ok(value) => value,
            Err(_) => return self.send("425 Can't open data connection."),
        };
        if listener.set_nonblocking(true).is_err() {
            return self.send("425 Can't open data connection.");
        }
        let port = match listener.local_addr() {
            Ok(value) => value.port(),
            Err(_) => return self.send("425 Can't open data connection."),
        };
        self.passive = Some(listener);

        let mut local = self
            .stream
            .local_addr()
            .ok()
            .map(|value| value.ip())
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        if let IpAddr::V6(_) = local {
            local = IpAddr::V4(Ipv4Addr::LOCALHOST);
        }
        let ip = match local {
            IpAddr::V4(value) => value,
            IpAddr::V6(_) => Ipv4Addr::LOCALHOST,
        };

        if extended {
            self.send(&format!(
                "229 Entering Extended Passive Mode (|||{}|)",
                port
            ))
        } else {
            let octets = ip.octets();
            self.send(&format!(
                "227 Entering Passive Mode ({},{},{},{},{},{})",
                octets[0],
                octets[1],
                octets[2],
                octets[3],
                port / 256,
                port % 256
            ))
        }
    }

    fn handle_port(&mut self, argument: &str) -> io::Result<()> {
        // PORT 用 6 个 0..255 的数字表示 IPv4 地址和端口。
        let fields: Vec<&str> = argument.split(',').collect();
        if fields.len() != 6 {
            return self.send("501 Syntax error in arguments.");
        }
        let mut values = [0u16; 6];
        for (index, field) in fields.iter().enumerate() {
            let value = match field.trim().parse::<u16>() {
                Ok(value) if value <= 255 => value,
                _ => return self.send("501 Syntax error in arguments."),
            };
            values[index] = value;
        }
        let ip = Ipv4Addr::new(
            values[0] as u8,
            values[1] as u8,
            values[2] as u8,
            values[3] as u8,
        );
        let remote = match self.stream.peer_addr() {
            Ok(value) => value.ip(),
            Err(_) => return self.send("501 Syntax error in arguments."),
        };
        if let IpAddr::V4(remote_v4) = remote {
            if remote_v4 != ip {
                // 防止 FTP bounce：服务器只能回连控制连接的对端地址。
                return self.send("501 PORT address does not match originator.");
            }
        } else {
            return self.send("501 PORT address does not match originator.");
        }

        self.passive = None;
        self.active = Some(SocketAddr::from((
            ip,
            values[4] * 256 + values[5],
        )));
        self.send("200 PORT command successful.")
    }

    fn handle_eprt(&mut self, argument: &str) -> io::Result<()> {
        let fields: Vec<&str> = argument.split('|').collect();
        if fields.len() < 5 {
            return self.send("501 Syntax error in arguments.");
        }
        let ip = match fields[2].parse::<IpAddr>() {
            Ok(value) => value,
            Err(_) => return self.send("501 Syntax error in arguments."),
        };
        let port = match fields[3].parse::<u16>() {
            Ok(value) if value > 0 => value,
            _ => return self.send("501 Syntax error in arguments."),
        };
        let remote = match self.stream.peer_addr() {
            Ok(value) => value.ip(),
            Err(_) => return self.send("501 Syntax error in arguments."),
        };
        if remote != ip {
            return self.send("501 PORT address does not match originator.");
        }

        self.passive = None;
        self.active = Some(SocketAddr::new(ip, port));
        self.send("200 PORT command successful.")
    }

    fn handle_listing(
        &mut self,
        argument: &str,
        names_only: bool,
        machine: bool,
    ) -> io::Result<()> {
        let mut path_argument = argument.trim();
        if path_argument.starts_with('-') {
            path_argument = match path_argument.find(' ') {
                Some(index) => path_argument[index + 1..].trim(),
                None => "",
            };
        }
        let path = match self.resolve(path_argument) {
            Ok((value, _)) if value.exists() => value,
            _ => return self.send("550 No such file or directory."),
        };

        let payload = match self.build_listing(&path, names_only, machine) {
            Ok(value) => value,
            Err(_) => return self.send("550 No such file or directory."),
        };

        let mut data = match self.open_data() {
            Some(value) => value,
            None => return self.send("425 Can't open data connection."),
        };
        self.send("150 Opening data channel.")?;
        // 目录/文件列表走数据连接，控制连接只负责 150/226 状态。
        let result = data.write_all(payload.as_bytes()).and_then(|_| data.flush());
        if result.is_ok() {
            self.send("226 Transfer complete.")
        } else {
            self.send("426 Previous command has been finished abnormally.")
        }
    }

    fn handle_mlst(&mut self, argument: &str) -> io::Result<()> {
        let path = match self.resolve(argument) {
            Ok((value, _)) if value.exists() => value,
            _ => return self.send("550 No such file or directory."),
        };
        let metadata = match fs::metadata(&path) {
            Ok(value) => value,
            Err(_) => return self.send("550 No such file or directory."),
        };
        let name = path
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());
        let facts = machine_facts(&metadata, metadata.is_dir());
        self.send(&format!("250-Listing {}", escape_ftp_path(&name)))?;
        self.send(&format!(" {} {}", facts, name))?;
        self.send("250 End")
    }

    fn handle_retr(&mut self, argument: &str) -> io::Result<()> {
        let path = match self.resolve(argument) {
            Ok((value, _)) if value.is_file() => value,
            _ => return self.send("550 File not found."),
        };
        let mut file = match File::open(&path) {
            Ok(value) => value,
            Err(_) => return self.send("550 File not found."),
        };
        if self.restart > 0 {
            // REST 断点不能超过文件长度；超出时从文件末尾开始。
            let length = file.metadata().map(|m| m.len()).unwrap_or(0);
            let _ = file.seek(SeekFrom::Start(self.restart.min(length)));
        }
        self.restart = 0;

        let mut data = match self.open_data() {
            Some(value) => value,
            None => return self.send("425 Can't open data connection."),
        };
        let name = path
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default();
        self.send(&format!("150 FILE: {}", name))?;

        let result = if self.ascii {
            copy_ascii_out(&mut file, &mut data)
        } else {
            io::copy(&mut file, &mut data).map(|_| ())
        };
        match result {
            Ok(_) => self.send("226 Transfer complete."),
            Err(_) => self.send("426 Previous command has been finished abnormally."),
        }
    }

    fn handle_stor(&mut self, argument: &str, append: bool) -> io::Result<()> {
        // 所有写命令先经过统一权限检查。
        if !self.require_write()? {
            return Ok(());
        }
        let path = match self.resolve(argument) {
            Ok((value, _)) => value,
            Err(_) => return self.send("550 Can't store file."),
        };
        let parent = match path.parent() {
            Some(value) if value.is_dir() => value,
            _ => return self.send("550 Can't store file."),
        };
        if !parent.exists() {
            return self.send("550 Can't store file.");
        }

        let mut data = match self.open_data() {
            Some(value) => value,
            None => return self.send("425 Can't open data connection."),
        };
        self.send("150 Opening data channel.")?;

        let mut options = OpenOptions::new();
        // APPE 追加；REST + STOR 覆盖指定位置；普通 STOR 截断后重写。
        options.create(true);
        if append {
            options.append(true);
        } else if self.restart > 0 {
            options.write(true).truncate(false);
        } else {
            options.write(true).truncate(true);
        }
        let mut file = match options.open(&path) {
            Ok(value) => value,
            Err(_) => return self.send("550 Can't store file."),
        };
        if !append && self.restart > 0 {
            let length = file.metadata().map(|m| m.len()).unwrap_or(0);
            let _ = file.seek(SeekFrom::Start(self.restart.min(length)));
        }
        self.restart = 0;

        let result = if self.ascii {
            copy_ascii_in(&mut data, &mut file)
        } else {
            io::copy(&mut data, &mut file).map(|_| ())
        };
        let flush_result = file.flush();
        match result.and(flush_result) {
            Ok(_) => self.send("226 Transfer complete."),
            Err(_) => self.send("426 Previous command has been finished abnormally."),
        }
    }

    fn handle_size(&mut self, argument: &str) -> io::Result<()> {
        match self.resolve(argument) {
            Ok((path, _)) if path.is_file() => {
                let length = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                self.send(&format!("213 {}", length))
            }
            _ => self.send("550 No such file."),
        }
    }

    fn handle_mdtm(&mut self, argument: &str) -> io::Result<()> {
        match self.resolve(argument) {
            Ok((path, _)) if path.is_file() => {
                let modified = fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .unwrap_or(UNIX_EPOCH);
                self.send(&format!("213 {}", format_timestamp(modified)))
            }
            _ => self.send("550 File not found."),
        }
    }

    fn handle_dele(&mut self, argument: &str) -> io::Result<()> {
        if !self.require_write()? {
            return Ok(());
        }
        match self.resolve(argument) {
            Ok((path, _)) if path.is_file() => match fs::remove_file(path) {
                Ok(_) => self.send("250 DELE command successful."),
                Err(_) => self.send("550 Can' t Remove or Access Error."),
            },
            _ => self.send("550 Can' t Remove or Access Error."),
        }
    }

    fn handle_rnfr(&mut self, argument: &str) -> io::Result<()> {
        if !self.require_write()? {
            return Ok(());
        }
        match self.resolve(argument) {
            Ok((path, _)) if path.exists() => {
                self.rename_from = Some(path);
                self.send("350 File or directory exists, ready for destination name.")
            }
            _ => self.send("550 File not found."),
        }
    }

    fn handle_rnto(&mut self, argument: &str) -> io::Result<()> {
        if !self.require_write()? {
            return Ok(());
        }
        let source = match self.rename_from.take() {
            Some(value) => value,
            None => return self.send("503 Bad sequence of commands."),
        };
        let destination = match self.resolve(argument) {
            Ok((value, _)) => value,
            Err(_) => return self.send("550 Rename failure."),
        };
        if destination.exists() {
            // 目标已存在时拒绝覆盖，避免远程客户端误删同名文件。
            return self.send("550 File Already Exists.");
        }
        let result = if source.is_dir() {
            fs::rename(&source, &destination)
        } else {
            fs::rename(&source, &destination)
        };
        match result {
            Ok(_) => self.send("250 Rename successful."),
            Err(_) => self.send("550 Rename failure."),
        }
    }

    fn handle_mkd(&mut self, argument: &str) -> io::Result<()> {
        if !self.require_write()? {
            return Ok(());
        }
        let path = match self.resolve(argument) {
            Ok((value, _)) => value,
            Err(_) => return self.send("550 MKD Error Creating DIR."),
        };
        if path.exists() {
            return self.send("550 File Already Exists.");
        }
        match fs::create_dir(path) {
            Ok(_) => self.send("250 MKD command successful."),
            Err(_) => self.send("550 MKD Error Creating DIR."),
        }
    }

    fn handle_rmd(&mut self, argument: &str) -> io::Result<()> {
        if !self.require_write()? {
            return Ok(());
        }
        match self.resolve(argument) {
            Ok((path, _)) if path.is_dir() => match fs::remove_dir(path) {
                Ok(_) => self.send("250 Directory deleted successfully."),
                Err(_) => self.send("450 Internal error deleting the directory."),
            },
            _ => self.send("550 Directory not found."),
        }
    }

    fn handle_rest(&mut self, argument: &str) -> io::Result<()> {
        match argument.trim().parse::<u64>() {
            Ok(value) => {
                self.restart = value;
                self.send("350 REST command successful.")
            }
            Err(_) => self.send("501 Syntax error in arguments."),
        }
    }

    fn handle_stat(&mut self, argument: &str) -> io::Result<()> {
        if argument.trim().is_empty() {
            self.send("211-Status:")?;
            self.send(" Connected")?;
            self.send(&format!(" Logged in: {}", if self.logged_in { "yes" } else { "no" }))?;
            self.send(&format!(" TYPE: {}", if self.ascii { "ASCII" } else { "Binary" }))?;
            return self.send("211 End");
        }

        let path = match self.resolve(argument) {
            Ok((value, _)) if value.exists() => value,
            _ => return self.send("550 No such file or directory."),
        };
        let listing = match self.build_listing(&path, false, false) {
            Ok(value) => value,
            Err(_) => return self.send("550 No such file or directory."),
        };
        self.send("213-Status follows:")?;
        for line in listing.lines() {
            self.send(&format!(" {}", line))?;
        }
        self.send("213 End of status")
    }

    fn require_write(&mut self) -> io::Result<bool> {
        if self.access.can_write() {
            Ok(true)
        } else {
            self.send("550 Permission denied.")?;
            Ok(false)
        }
    }

    fn current_path(&self) -> String {
        if self.cwd.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", self.cwd.join("/"))
        }
    }

    fn resolve(&self, argument: &str) -> Result<(PathBuf, Vec<String>), ()> {
        // 将 FTP 的 '/' 路径归一化；".." 在根目录处不会继续向上。
        let value = argument.replace('\\', "/");
        let mut parts = self.cwd.clone();
        let trimmed = value.trim_start_matches('/');
        if value.starts_with('/') {
            parts.clear();
        }
        for segment in trimmed.split('/') {
            if segment.is_empty() || segment == "." {
                continue;
            }
            if segment == ".." {
                parts.pop();
                continue;
            }
            if segment.contains('\0') || segment.contains(':') || is_reserved_name(segment) {
                // 冒号代表盘符/NTFS ADS，设备名可能绕过普通文件系统语义。
                return Err(());
            }
            parts.push(segment.to_string());
        }

        let mut path = self.config.root.clone();
        for part in &parts {
            path.push(part);
        }

        let candidate = if path.exists() {
            // 已存在路径交给 canonicalize 解析 junction/符号链接。
            fs::canonicalize(&path).map_err(|_| ())?
        } else {
            // 新文件校验父目录，确保上传目标仍位于共享根目录内。
            let parent = path.parent().ok_or(())?;
            let parent_canon = fs::canonicalize(parent).map_err(|_| ())?;
            let name = path.file_name().ok_or(())?;
            parent_canon.join(name)
        };

        if !candidate.starts_with(&self.config.root_canon) {
            // 最终前缀校验是防止目录逃逸的最后一道防线。
            return Err(());
        }
        Ok((candidate, parts))
    }

    fn open_data(&mut self) -> Option<TcpStream> {
        // 被动模式轮询 accept，主动模式使用 connect_timeout；两者都有超时。
        if let Some(listener) = self.passive.take() {
            let started = Instant::now();
            loop {
                if self.shared.stop.load(Ordering::SeqCst) {
                    return None;
                }
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_nodelay(true);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(60)));
                        return Some(stream);
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        if started.elapsed() > Duration::from_secs(30) {
                            return None;
                        }
                        thread::sleep(Duration::from_millis(40));
                    }
                    Err(_) => return None,
                }
            }
        }

        if let Some(address) = self.active.take() {
            if let Ok(stream) = TcpStream::connect_timeout(&address, Duration::from_secs(10)) {
                let _ = stream.set_nodelay(true);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(60)));
                return Some(stream);
            }
            return None;
        }
        None
    }

    fn build_listing(
        &self,
        path: &Path,
        names_only: bool,
        machine: bool,
    ) -> io::Result<String> {
        // 统一生成 LIST/NLST/MLSD 文本，目录项按名称排序便于客户端浏览。
        let metadata = fs::metadata(path)?;
        let mut result = String::new();
        if metadata.is_file() {
            let name = path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_default();
            append_entry(&mut result, &name, &metadata, false, names_only, machine, self.access);
            return Ok(result);
        }

        let mut entries = Vec::new();
        for item in fs::read_dir(path)? {
            let item = match item {
                Ok(value) => value,
                Err(_) => continue,
            };
            let name = item.file_name().to_string_lossy().to_string();
            let meta = match item.metadata() {
                Ok(value) => value,
                Err(_) => continue,
            };
            entries.push((name, meta));
        }
        entries.sort_by(|left, right| {
            left.0
                .to_ascii_lowercase()
                .cmp(&right.0.to_ascii_lowercase())
        });
        for (name, meta) in entries {
            append_entry(
                &mut result,
                &name,
                &meta,
                meta.is_dir(),
                names_only,
                machine,
                self.access,
            );
        }
        Ok(result)
    }
}

fn is_pre_login_command(command: &str) -> bool {
    command == "USER"
        || command == "PASS"
        || command == "QUIT"
        || command == "NOOP"
        || command == "FEAT"
        || command == "SYST"
        || command == "HELP"
        || command == "CLNT"
        || command == "OPTS"
        || command == "REIN"
}

fn is_reserved_name(segment: &str) -> bool {
    // 拒绝 CON/NUL/COM1 等 Windows 设备名，避免通过 FTP 访问设备对象。
    let trimmed = segment.trim_end_matches(|c| c == ' ' || c == '.');
    let base = trimmed.split('.').next().unwrap_or("").to_ascii_uppercase();
    if base == "CON"
        || base == "PRN"
        || base == "AUX"
        || base == "NUL"
        || base == "CONIN$"
        || base == "CONOUT$"
    {
        return true;
    }
    if base.len() == 4
        && (base.starts_with("COM") || base.starts_with("LPT"))
        && base.as_bytes()[3] >= b'1'
        && base.as_bytes()[3] <= b'9'
    {
        return true;
    }
    false
}

fn escape_ftp_path(value: &str) -> String {
    value.replace('"', "\"\"")
}

fn append_entry(
    output: &mut String,
    name: &str,
    metadata: &fs::Metadata,
    is_dir: bool,
    names_only: bool,
    machine: bool,
    access: Access,
) {
    // Windows 没有 Unix 权限位；只读账号统一显示不可写权限。
    if names_only {
        output.push_str(name);
        output.push_str("\r\n");
        return;
    }
    if machine {
        output.push_str(&machine_facts(metadata, is_dir));
        output.push(' ');
        output.push_str(name);
        output.push_str("\r\n");
        return;
    }

    let permissions = if is_dir {
        if access.can_write() {
            "drwxrwxrwx"
        } else {
            "dr-xr-xr-x"
        }
    } else if access.can_write() {
        "-rw-rw-rw-"
    } else {
        "-r--r--r--"
    };
    let size = if is_dir { 0 } else { metadata.len() };
    let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
    output.push_str(&format!(
        "{} 1 user group {:>14} {} {}\r\n",
        permissions,
        size,
        format_list_time(modified),
        name
    ));
}

fn machine_facts(metadata: &fs::Metadata, is_dir: bool) -> String {
    let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
    if is_dir {
        format!("type=dir;modify={};", format_timestamp(modified))
    } else {
        format!(
            "type=file;size={};modify={};",
            metadata.len(),
            format_timestamp(modified)
        )
    }
}

fn format_timestamp(time: SystemTime) -> String {
    let (year, month, day, hour, minute, second) = time_parts(time);
    format!(
        "{:04}{:02}{:02}{:02}{:02}{:02}",
        year, month, day, hour, minute, second
    )
}

fn format_list_time(time: SystemTime) -> String {
    // LIST 使用接近 Unix ls 的时间格式；超过半年显示年份，否则显示时分。
    let (year, month, day, hour, minute, _) = time_parts(time);
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month_name = months
        .get((month.saturating_sub(1)) as usize)
        .unwrap_or(&"Jan");
    let age = SystemTime::now()
        .duration_since(time)
        .unwrap_or(Duration::from_secs(0));
    if age > Duration::from_secs(180 * 24 * 60 * 60) {
        format!("{} {:02}  {:04}", month_name, day, year)
    } else {
        format!("{} {:02} {:02}:{:02}", month_name, day, hour, minute)
    }
}

fn time_parts(time: SystemTime) -> (i32, u32, u32, u32, u32, u32) {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0);
    let days = seconds.div_euclid(86400);
    let remainder = seconds.rem_euclid(86400);
    let (year, month, day) = civil_from_days(days);
    let hour = (remainder / 3600) as u32;
    let minute = ((remainder % 3600) / 60) as u32;
    let second = (remainder % 60) as u32;
    (year, month, day, hour, minute, second)
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    // Howard Hinnant 的 civil-from-days 算法，避免引入日期库。
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    (
        (year + if month <= 2 { 1 } else { 0 }) as i32,
        month as u32,
        day as u32,
    )
}

fn copy_ascii_out<R: Read, W: Write>(reader: &mut R, writer: &mut W) -> io::Result<()> {
    // ASCII 下载：LF -> CRLF，并避免已有 CRLF 被重复转换。
    let mut buffer = [0u8; 65536];
    let mut output = Vec::with_capacity(65536);
    let mut last_was_cr = false;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        for &byte in &buffer[..read] {
            if byte == b'\n' && !last_was_cr {
                output.push(b'\r');
            }
            output.push(byte);
            last_was_cr = byte == b'\r';
            if output.len() >= 65536 {
                writer.write_all(&output)?;
                output.clear();
            }
        }
    }
    if !output.is_empty() {
        writer.write_all(&output)?;
    }
    writer.flush()
}

fn copy_ascii_in<R: Read, W: Write>(reader: &mut R, writer: &mut W) -> io::Result<()> {
    // ASCII 上传：CRLF -> LF；pending_cr 处理跨缓冲区的 CRLF。
    let mut buffer = [0u8; 65536];
    let mut output = Vec::with_capacity(65536);
    let mut pending_cr = false;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        for &byte in &buffer[..read] {
            if pending_cr {
                if byte == b'\n' {
                    output.push(b'\n');
                    pending_cr = false;
                    continue;
                }
                output.push(b'\r');
                pending_cr = false;
            }
            if byte == b'\r' {
                pending_cr = true;
            } else {
                output.push(byte);
            }
            if output.len() >= 65536 {
                writer.write_all(&output)?;
                output.clear();
            }
        }
    }
    if pending_cr {
        output.push(b'\r');
    }
    if !output.is_empty() {
        writer.write_all(&output)?;
    }
    writer.flush()
}

pub fn primary_ipv4() -> String {
    // UDP connect 不会发包，只让系统选择默认路由对应的本机 IPv4 地址。
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(address) = socket.local_addr() {
                if let IpAddr::V4(value) = address.ip() {
                    return value.to_string();
                }
            }
        }
    }
    "127.0.0.1".to_string()
}

pub fn local_addresses() -> Vec<String> {
    let primary = primary_ipv4();
    if primary == "127.0.0.1" {
        vec![primary]
    } else {
        vec![primary, "127.0.0.1".to_string()]
    }
}
