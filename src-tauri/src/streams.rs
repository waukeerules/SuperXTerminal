//! Telnet, Raw and Serial sessions, which SuperXPuTTY runs through plink (`-telnet`, `-raw`, `-serial`): here they're
//! byte streams in the core, read and written like a tab's pseudo terminal, so the page, logs, scripts and reconnect
//! work the same.
//!
//! - **Telnet:** a small client that negotiates as PuTTY does: it offers the terminal type (XTERM) and window size,
//!   lets the server echo and suppress go-ahead, refuses the rest, and sends Return as a Telnet new line (CR LF).
//! - **Raw:** a TCP connection, bytes as they are.
//! - **Serial:** a `/dev/cu.*` device driven directly (termios), with PuTTY's line settings: 9600 baud, 8 data bits, no
//!   parity, 1 stop bit and XON/XOFF unless `-sercfg` says otherwise.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Windows has no serial support here yet: a stand-in for a device's descriptor, never made (`open_serial` refuses
/// first), so the serial tab code builds
#[cfg(not(unix))]
struct OwnedFd;

/// What a stream session connects to
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// `via`: an SSH jump host ("user@host:port") the connection goes through (`ssh -W`)
    Telnet { host: String, port: u16, via: Option<String> },
    Raw { host: String, port: u16, via: Option<String> },
    Serial { device: String, line: SerialLine },
}

/// A serial line's settings, as PuTTY's `-sercfg 115200,8,n,1,N`
#[derive(Clone, Debug, PartialEq)]
pub struct SerialLine {
    pub speed: u32,
    pub data_bits: u8,
    /// 'n' none, 'o' odd, 'e' even, 'm' mark, 's' space
    pub parity: char,
    /// 1 or 2 (PuTTY's 1.5 is taken as 2)
    pub stop_bits: u8,
    /// 'N' none, 'X' XON/XOFF, 'R' RTS/CTS, 'D' DSR/DTR (taken as none)
    pub flow: char,
}

impl Default for SerialLine {
    /// PuTTY's defaults
    fn default() -> SerialLine {
        SerialLine { speed: 9600, data_bits: 8, parity: 'n', stop_bits: 1, flow: 'X' }
    }
}

impl SerialLine {
    /// Apply PuTTY's `-sercfg` value: comma-separated settings in any order (a number over 9 is the speed, 5-9 the
    /// data bits, 1/1.5/2 stop bits, a letter parity or flow control)
    pub fn apply(&mut self, cfg: &str) -> Result<(), String> {
        for part in cfg.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            match part {
                "1" => self.stop_bits = 1,
                "1.5" | "2" => self.stop_bits = 2,
                "5" | "6" | "7" | "8" | "9" => self.data_bits = part.parse::<u8>().unwrap_or(8).min(8),
                "n" | "o" | "e" | "m" | "s" => self.parity = part.chars().next().unwrap(),
                "N" | "X" | "R" | "D" => self.flow = part.chars().next().unwrap(),
                _ => self.speed = part.parse().map_err(|_| format!("-sercfg: \"{part}\" isn't a serial setting"))?,
            }
        }
        Ok(())
    }

    /// As PuTTY's `-sercfg` value, e.g. "9600,8,n,1,X"
    pub fn to_sercfg(&self) -> String {
        format!("{},{},{},{},{}", self.speed, self.data_bits, self.parity, self.stop_bits, self.flow)
    }
}

/// The writing half of an open stream, and what it needs to close, resize and send special commands
pub struct Stream {
    pub writer: Arc<Mutex<Box<dyn Write + Send>>>,
    kind: Kind,
}

enum Kind {
    Telnet { link: Link, size: Arc<Mutex<(u16, u16)>>, naws: Arc<Mutex<bool>> },
    Raw { link: Link },
    Serial { fd: Arc<OwnedFd>, closed: Arc<AtomicBool>, line: Mutex<SerialLine> },
}

/// A TCP connection: direct, or through an SSH jump host (`ssh -W host:port`, its pipes standing in for the socket)
#[derive(Clone)]
enum Link {
    Direct(Arc<TcpStream>),
    Jump(Arc<Mutex<std::process::Child>>, Arc<Mutex<std::process::ChildStdin>>),
}

impl Link {
    fn open(host: &str, port: u16, via: &Option<String>) -> Result<(Link, Box<dyn Read + Send>), String> {
        match via {
            None => {
                let socket = connect(host, port)?;
                let reader = socket.try_clone().map_err(|e| e.to_string())?;
                Ok((Link::Direct(Arc::new(socket)), Box::new(reader)))
            }
            Some(jump) => {
                let (jump_host, jump_port) = match jump.rsplit_once(':') {
                    Some((h, p)) if p.parse::<u16>().is_ok() => (h.to_string(), p.to_string()),
                    _ => (jump.clone(), "22".into()),
                };
                let mut child = std::process::Command::new("ssh")
                    .args(["-o", "BatchMode=yes", "-p", &jump_port, "-W", &format!("{host}:{port}"), &jump_host])
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::inherit())
                    .spawn()
                    .map_err(|e| format!("Couldn't start ssh for the jump host: {e}"))?;
                let stdin = child.stdin.take().ok_or("ssh has no input")?;
                let stdout = child.stdout.take().ok_or("ssh has no output")?;
                Ok((Link::Jump(Arc::new(Mutex::new(child)), Arc::new(Mutex::new(stdin))), Box::new(stdout)))
            }
        }
    }

    fn write_all(&self, bytes: &[u8]) -> std::io::Result<()> {
        match self {
            Link::Direct(socket) => (&**socket).write_all(bytes),
            Link::Jump(_, stdin) => stdin.lock().unwrap().write_all(bytes),
        }
    }

    fn close(&self) {
        match self {
            Link::Direct(socket) => {
                let _ = socket.shutdown(Shutdown::Both);
            }
            Link::Jump(child, _) => {
                let _ = child.lock().unwrap().kill();
            }
        }
    }
}

impl Write for Link {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Link::write_all(self, buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Telnet's codes (RFC 854 and friends)
mod telnet {
    pub const IAC: u8 = 255;
    pub const DONT: u8 = 254;
    pub const DO: u8 = 253;
    pub const WONT: u8 = 252;
    pub const WILL: u8 = 251;
    pub const SB: u8 = 250;
    pub const SE: u8 = 240;
    pub const ECHO: u8 = 1;
    pub const SGA: u8 = 3;
    pub const TTYPE: u8 = 24;
    pub const NAWS: u8 = 31;
}

/// Open a stream; returns its writing half and its reader
pub fn open(target: &Target, cols: u16, rows: u16) -> Result<(Stream, Box<dyn Read + Send>), String> {
    match target {
        Target::Telnet { host, port, via } => {
            let (link, input) = Link::open(host, *port, via)?;
            let size = Arc::new(Mutex::new((cols, rows)));
            let naws = Arc::new(Mutex::new(false));
            let writer = Arc::new(Mutex::new(Box::new(TelnetWriter { link: link.clone() }) as Box<dyn Write + Send>));
            let reader = TelnetReader { input, answers: link.clone(), size: size.clone(), naws: naws.clone(), pending: Vec::new() };
            Ok((Stream { writer, kind: Kind::Telnet { link, size, naws } }, Box::new(reader)))
        }
        Target::Raw { host, port, via } => {
            let (link, input) = Link::open(host, *port, via)?;
            let writer = Arc::new(Mutex::new(Box::new(link.clone()) as Box<dyn Write + Send>));
            Ok((Stream { writer, kind: Kind::Raw { link } }, input))
        }
        Target::Serial { device, line } => {
            let fd = Arc::new(open_serial(device, line)?);
            let closed = Arc::new(AtomicBool::new(false));
            let writer = Arc::new(Mutex::new(Box::new(FdIo { fd: fd.clone(), closed: closed.clone() }) as Box<dyn Write + Send>));
            let kind = Kind::Serial { fd: fd.clone(), closed: closed.clone(), line: Mutex::new(line.clone()) };
            Ok((Stream { writer, kind }, Box::new(FdIo { fd, closed })))
        }
    }
}

/// A TCP connection to host:port, giving up after 10 seconds per address
fn connect(host: &str, port: u16) -> Result<TcpStream, String> {
    let addresses = (host, port).to_socket_addrs().map_err(|e| format!("Couldn't find {host}: {e}"))?;
    let mut last = format!("Couldn't find {host}");
    for address in addresses {
        match TcpStream::connect_timeout(&address, Duration::from_secs(10)) {
            Ok(socket) => {
                let _ = socket.set_nodelay(true);
                return Ok(socket);
            }
            Err(e) => last = format!("Couldn't connect to {host} port {port}: {e}"),
        }
    }
    Err(last)
}

impl Stream {
    /// The tab was resized: Telnet tells the server (NAWS), if it asked
    pub fn resize(&self, cols: u16, rows: u16) {
        if let Kind::Telnet { link, size, naws } = &self.kind {
            *size.lock().unwrap() = (cols, rows);
            if *naws.lock().unwrap() {
                let _ = link.write_all(&naws_message(cols, rows));
            }
        }
    }

    /// End the connection; the reader then sees the end
    pub fn close(&self) {
        match &self.kind {
            Kind::Telnet { link, .. } | Kind::Raw { link } => link.close(),
            Kind::Serial { closed, .. } => closed.store(true, Ordering::Relaxed),
        }
    }

    /// A Special Command, as PuTTY's menu: Telnet's IAC codes, a serial Break; Err for ones this connection hasn't
    pub fn special(&self, command: &str) -> Result<(), String> {
        match &self.kind {
            Kind::Telnet { link, .. } => {
                let code = match command {
                    "break" => 243,
                    "ayt" => 246,
                    "interrupt" => 244,
                    "abort" => 238,
                    "ao" => 245,
                    "ec" => 247,
                    "el" => 248,
                    "ga" => 249,
                    "nop" => 241,
                    "eof" => 236,
                    "eor" => 239,
                    "susp" => 237,
                    "synch" => 242,
                    other => return Err(format!("Telnet has no {other}")),
                };
                link.write_all(&[telnet::IAC, code]).map_err(|e| e.to_string())
            }
            Kind::Serial { fd, .. } if command == "break" => {
                send_break(fd)
            }
            _ => Err("This connection has no special commands".into()),
        }
    }

    /// A serial tab's line settings now
    pub fn serial_line(&self) -> Option<SerialLine> {
        match &self.kind {
            Kind::Serial { line, .. } => Some(line.lock().unwrap().clone()),
            _ => None,
        }
    }

    /// Change a serial tab's line settings while it's open (the speed, parity...), as PuTTY's Change Settings
    pub fn set_serial_line(&self, new: SerialLine) -> Result<(), String> {
        match &self.kind {
            Kind::Serial { fd, line, .. } => {
                configure(fd, &new).map_err(|e| format!("Couldn't change the line to {}: {e}", new.to_sercfg()))?;
                *line.lock().unwrap() = new;
                Ok(())
            }
            _ => Err("Only a serial tab has line settings".into()),
        }
    }
}

/// NAWS: the window's width and height, IAC escaped
fn naws_message(cols: u16, rows: u16) -> Vec<u8> {
    let mut out = vec![telnet::IAC, telnet::SB, telnet::NAWS];
    for byte in [cols.to_be_bytes(), rows.to_be_bytes()].concat() {
        out.push(byte);
        if byte == telnet::IAC {
            out.push(telnet::IAC);
        }
    }
    out.extend([telnet::IAC, telnet::SE]);
    out
}

/// Typing to a Telnet server: IAC doubled, Return as a Telnet new line (CR LF), as PuTTY's default
struct TelnetWriter {
    link: Link,
}

impl Write for TelnetWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.link.write_all(&telnet_out(buf))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Bytes typed, as a Telnet client sends them
fn telnet_out(buf: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(buf.len() + 4);
    let mut i = 0;
    while i < buf.len() {
        match buf[i] {
            telnet::IAC => out.extend([telnet::IAC, telnet::IAC]),
            b'\r' => {
                out.extend(b"\r\n");
                if buf.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
            }
            byte => out.push(byte),
        }
        i += 1;
    }
    out
}

/// Reading from a Telnet server: data passed on, commands answered (as PuTTY negotiates) and dropped
struct TelnetReader {
    input: Box<dyn Read + Send>,
    answers: Link,
    size: Arc<Mutex<(u16, u16)>>,
    naws: Arc<Mutex<bool>>,
    /// The start of a command split across reads
    pending: Vec<u8>,
}

impl Read for TelnetReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            let mut incoming = vec![0u8; buf.len().max(1)];
            let n = self.input.read(&mut incoming)?;
            if n == 0 {
                return Ok(0);
            }
            let mut bytes = std::mem::take(&mut self.pending);
            bytes.extend_from_slice(&incoming[..n]);
            let (data, answers, rest) = telnet_in(&bytes, *self.size.lock().unwrap());
            self.pending = rest;
            if answers.iter().any(|a| a.starts_with(&[telnet::IAC, telnet::WILL, telnet::NAWS])) {
                *self.naws.lock().unwrap() = true;
            }
            for answer in answers {
                let _ = self.answers.write_all(&answer);
            }
            if !data.is_empty() {
                let count = data.len().min(buf.len());
                buf[..count].copy_from_slice(&data[..count]);
                // a read never holds more data than came in, so nothing is left over
                return Ok(count);
            }
        }
    }
}

/// Split what a Telnet server sent into data, the answers to its commands, and an incomplete command at the end
fn telnet_in(bytes: &[u8], (cols, rows): (u16, u16)) -> (Vec<u8>, Vec<Vec<u8>>, Vec<u8>) {
    use telnet::*;
    let mut data = Vec::with_capacity(bytes.len());
    let mut answers = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != IAC {
            data.push(bytes[i]);
            i += 1;
            continue;
        }
        let Some(&command) = bytes.get(i + 1) else { return (data, answers, bytes[i..].to_vec()) };
        match command {
            IAC => {
                data.push(IAC);
                i += 2;
            }
            DO | DONT | WILL | WONT => {
                let Some(&option) = bytes.get(i + 2) else { return (data, answers, bytes[i..].to_vec()) };
                match (command, option) {
                    (DO, TTYPE) => answers.push(vec![IAC, WILL, TTYPE]),
                    (DO, NAWS) => {
                        answers.push(vec![IAC, WILL, NAWS]);
                        answers.push(naws_message(cols, rows));
                    }
                    (DO, SGA) => answers.push(vec![IAC, WILL, SGA]),
                    (DO, other) => answers.push(vec![IAC, WONT, other]),
                    (WILL, ECHO) | (WILL, SGA) => answers.push(vec![IAC, DO, option]),
                    (WILL, other) => answers.push(vec![IAC, DONT, other]),
                    _ => {}
                }
                i += 3;
            }
            SB => {
                let Some(end) = bytes[i..].windows(2).position(|w| w == [IAC, SE]) else { return (data, answers, bytes[i..].to_vec()) };
                // TTYPE SEND: say what this terminal is
                if bytes.get(i + 2) == Some(&TTYPE) && bytes.get(i + 3) == Some(&1) {
                    let mut answer = vec![IAC, SB, TTYPE, 0];
                    answer.extend(b"XTERM");
                    answer.extend([IAC, SE]);
                    answers.push(answer);
                }
                i += end + 2;
            }
            // GA, NOP, DM and the rest carry no data
            _ => i += 2,
        }
    }
    (data, answers, Vec::new())
}

/// Reading and writing a serial device's file descriptor. Reads wait in short steps, so closing the tab (`closed`)
/// ends a read on a quiet line.
struct FdIo {
    fd: Arc<OwnedFd>,
    closed: Arc<AtomicBool>,
}

#[cfg(unix)]
impl Read for FdIo {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let raw = self.fd.as_raw_fd();
        loop {
            if self.closed.load(Ordering::Relaxed) {
                return Ok(0);
            }
            let mut poll = libc::pollfd { fd: raw, events: libc::POLLIN, revents: 0 };
            let ready = unsafe { libc::poll(&mut poll, 1, 200) };
            if ready < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if ready == 0 {
                continue;
            }
            // the device went away (unplugged)
            if poll.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 && poll.revents & libc::POLLIN == 0 {
                return Err(std::io::Error::other("the serial device went away"));
            }
            let n = unsafe { libc::read(raw, buf.as_mut_ptr().cast(), buf.len()) };
            return if n < 0 { Err(std::io::Error::last_os_error()) } else { Ok(n as usize) };
        }
    }
}

#[cfg(unix)]
impl Write for FdIo {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = unsafe { libc::write(self.fd.as_raw_fd(), buf.as_ptr().cast(), buf.len()) };
        if n < 0 { Err(std::io::Error::last_os_error()) } else { Ok(n as usize) }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Open a serial device raw, with the line's settings
#[cfg(unix)]
fn open_serial(device: &str, line: &SerialLine) -> Result<OwnedFd, String> {
    let path = std::ffi::CString::new(device).map_err(|e| e.to_string())?;
    // O_NONBLOCK so opening doesn't wait for carrier; reads block again once it's set up
    let raw = unsafe { libc::open(path.as_ptr(), libc::O_RDWR | libc::O_NOCTTY | libc::O_NONBLOCK) };
    if raw < 0 {
        return Err(format!("Couldn't open {device}: {}", std::io::Error::last_os_error()));
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    configure(&fd, line).map_err(|e| format!("Couldn't set {device} to {}: {e}", line.to_sercfg()))?;
    unsafe {
        let flags = libc::fcntl(raw, libc::F_GETFL);
        libc::fcntl(raw, libc::F_SETFL, flags & !libc::O_NONBLOCK);
    }
    Ok(fd)
}

/// Set a serial device's line: raw, with the speed, data bits, parity, stop bits and flow control
#[cfg(unix)]
fn configure(fd: &OwnedFd, line: &SerialLine) -> Result<(), std::io::Error> {
    let raw = fd.as_raw_fd();
    unsafe {
        let mut tio: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(raw, &mut tio) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        libc::cfmakeraw(&mut tio);
        tio.c_cflag |= libc::CREAD | libc::CLOCAL;
        tio.c_cflag &= !libc::CSIZE;
        tio.c_cflag |= match line.data_bits {
            5 => libc::CS5,
            6 => libc::CS6,
            7 => libc::CS7,
            _ => libc::CS8,
        };
        tio.c_cflag &= !(libc::PARENB | libc::PARODD);
        match line.parity {
            'o' => tio.c_cflag |= libc::PARENB | libc::PARODD,
            'e' => tio.c_cflag |= libc::PARENB,
            _ => {}
        }
        if line.stop_bits == 2 {
            tio.c_cflag |= libc::CSTOPB;
        } else {
            tio.c_cflag &= !libc::CSTOPB;
        }
        tio.c_cflag &= !libc::CRTSCTS;
        tio.c_iflag &= !(libc::IXON | libc::IXOFF | libc::IXANY);
        match line.flow {
            'X' => tio.c_iflag |= libc::IXON | libc::IXOFF,
            'R' => tio.c_cflag |= libc::CRTSCTS,
            _ => {}
        }
        // a read waits for at least one byte
        tio.c_cc[libc::VMIN] = 1;
        tio.c_cc[libc::VTIME] = 0;
        if libc::cfsetspeed(&mut tio, line.speed as libc::speed_t) != 0 || libc::tcsetattr(raw, libc::TCSANOW, &tio) != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Send a break on a serial line
#[cfg(unix)]
fn send_break(fd: &OwnedFd) -> Result<(), String> {
    if unsafe { libc::tcsendbreak(fd.as_raw_fd(), 0) } == 0 { Ok(()) } else { Err(std::io::Error::last_os_error().to_string()) }
}

/// Serial lines aren't supported on Windows yet
#[cfg(not(unix))]
const NO_SERIAL: &str = "Serial sessions aren't supported on Windows yet.";

/// Serial lines aren't supported on Windows yet
#[cfg(not(unix))]
fn open_serial(_device: &str, _line: &SerialLine) -> Result<OwnedFd, String> {
    Err(NO_SERIAL.into())
}

/// Serial lines aren't supported on Windows yet
#[cfg(not(unix))]
fn configure(_fd: &OwnedFd, _line: &SerialLine) -> Result<(), std::io::Error> {
    Err(std::io::Error::other(NO_SERIAL))
}

/// Serial lines aren't supported on Windows yet
#[cfg(not(unix))]
fn send_break(_fd: &OwnedFd) -> Result<(), String> {
    Err(NO_SERIAL.into())
}

/// Never reached on Windows: no serial device is ever opened
#[cfg(not(unix))]
impl Read for FdIo {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other(NO_SERIAL))
    }
}

/// Never reached on Windows: no serial device is ever opened
#[cfg(not(unix))]
impl Write for FdIo {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other(NO_SERIAL))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The serial devices on this Mac (`/dev/cu.*`), for choosing which one a COM name is
pub fn serial_devices() -> Vec<String> {
    let mut devices: Vec<String> = std::fs::read_dir("/dev")
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|name| name.starts_with("cu.") && name != "cu.Bluetooth-Incoming-Port" && name != "cu.debug-console")
                .map(|name| format!("/dev/{name}"))
                .collect()
        })
        .unwrap_or_default();
    devices.sort();
    devices
}

#[cfg(test)]
mod tests {
    use super::*;
    use telnet::*;

    #[test]
    fn reads_sercfg_as_putty() {
        let mut line = SerialLine::default();
        line.apply("115200,7,e,2,N").unwrap();
        assert_eq!(line, SerialLine { speed: 115200, data_bits: 7, parity: 'e', stop_bits: 2, flow: 'N' });
        assert_eq!(line.to_sercfg(), "115200,7,e,2,N");
        let mut again = SerialLine::default();
        again.apply(&line.to_sercfg()).unwrap();
        assert_eq!(again, line);
        assert!(line.apply("fast").is_err());
    }

    #[test]
    fn negotiates_as_putty() {
        let from_server = [b'h', IAC, DO, TTYPE, IAC, DO, NAWS, IAC, WILL, ECHO, IAC, DO, 39, b'i', IAC, IAC, IAC, SB, TTYPE, 1, IAC, SE, b'!'];
        let (data, answers, rest) = telnet_in(&from_server, (80, 24));
        assert_eq!(data, vec![b'h', b'i', IAC, b'!']);
        assert!(rest.is_empty());
        assert_eq!(answers[0], vec![IAC, WILL, TTYPE]);
        assert_eq!(answers[1], vec![IAC, WILL, NAWS]);
        assert_eq!(answers[2], vec![IAC, SB, NAWS, 0, 80, 0, 24, IAC, SE]);
        assert_eq!(answers[3], vec![IAC, DO, ECHO]);
        assert_eq!(answers[4], vec![IAC, WONT, 39]);
        assert_eq!(answers[5][..4], [IAC, SB, TTYPE, 0]);
        // a command cut off at the end waits for the next read
        let (data, _, rest) = telnet_in(&[b'x', IAC, DO], (80, 24));
        assert_eq!((data, rest), (vec![b'x'], vec![IAC, DO]));
    }

    #[test]
    fn sends_return_as_a_telnet_new_line() {
        assert_eq!(telnet_out(b"ls\r"), b"ls\r\n");
        assert_eq!(telnet_out(b"a\r\nb"), b"a\r\nb");
        assert_eq!(telnet_out(&[IAC]), vec![IAC, IAC]);
    }
}
