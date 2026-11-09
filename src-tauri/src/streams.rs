//! Telnet, Raw and Serial sessions, which SuperXPuTTY runs through plink (`-telnet`, `-raw`, `-serial`): here they're
//! byte streams in the core, read and written like a tab's pseudo terminal, so the page, logs, scripts and reconnect
//! work the same.
//!
//! - **Telnet:** a small client that negotiates as PuTTY does: it offers the terminal type (XTERM) and window size,
//!   lets the server echo and suppress go-ahead, refuses the rest, and sends Return as a Telnet new line (CR LF).
//! - **Raw:** a TCP connection, bytes as they are.
//! - **Serial:** a `/dev` device driven directly (termios) on the Mac (`/dev/cu.*`) and Linux (`/dev/ttyUSB*`,
//!   `/dev/ttyACM*`...), a COM port (Windows' serial API) on Windows, with PuTTY's line settings: 9600 baud, 8 data bits, no parity, 1 stop bit and XON/XOFF unless `-sercfg`
//!   says otherwise.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A serial port opened on Windows (`\\.\COM9`), closed when the last of its users lets go; it stands where the Mac
/// has the device's file descriptor, so the serial tab's code is the same on both
#[cfg(windows)]
struct OwnedFd(windows_sys::Win32::Foundation::HANDLE);

// SAFETY: a Windows handle can be used from any thread; reads and writes on it are what the reader and writer
// threads do, as with the Mac's file descriptor
#[cfg(windows)]
unsafe impl Send for OwnedFd {}
#[cfg(windows)]
unsafe impl Sync for OwnedFd {}

#[cfg(windows)]
impl Drop for OwnedFd {
    /// Close the port, so another program (or a new tab) can open it
    fn drop(&mut self) {
        // SAFETY: the handle CreateFileW gave, closed once
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

/// Which of a host name's addresses a connection uses, as PuTTY's Internet protocol version (Auto, IPv4, IPv6)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Family {
    #[default]
    Any,
    V4,
    V6,
}

impl Family {
    /// Whether an address is one this family uses
    fn takes(self, address: &std::net::SocketAddr) -> bool {
        match self {
            Family::Any => true,
            Family::V4 => address.is_ipv4(),
            Family::V6 => address.is_ipv6(),
        }
    }

    /// ssh's flag for it (-4 or -6), for a connection through a jump host; None for any
    fn ssh_flag(self) -> Option<&'static str> {
        match self {
            Family::Any => None,
            Family::V4 => Some("-4"),
            Family::V6 => Some("-6"),
        }
    }

    /// How it's named in a message
    fn name(self) -> &'static str {
        match self {
            Family::Any => "",
            Family::V4 => "IPv4 ",
            Family::V6 => "IPv6 ",
        }
    }
}

/// What a stream session connects to
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// `via`: an SSH jump host ("user@host:port") the connection goes through (`ssh -W`); `family`: which of the
    /// host's addresses it's reached at (the profile's Internet protocol version)
    Telnet { host: String, port: u16, via: Option<String>, family: Family },
    Raw { host: String, port: u16, via: Option<String>, family: Family },
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

/// What the stream is: a Telnet or raw TCP connection (direct or through a jump host), or a serial line
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
    /// Connect, straight to the host or through the jump host's ssh (`-W`); returns the link and what reads from it
    fn open(host: &str, port: u16, via: &Option<String>, family: Family) -> Result<(Link, Box<dyn Read + Send>), String> {
        match via {
            None => {
                let socket = connect(host, port, family)?;
                let reader = socket.try_clone().map_err(|e| e.to_string())?;
                Ok((Link::Direct(Arc::new(socket)), Box::new(reader)))
            }
            Some(jump) => {
                let (jump_host, jump_port) = match jump.rsplit_once(':') {
                    Some((h, p)) if p.parse::<u16>().is_ok() => (h.to_string(), p.to_string()),
                    _ => (jump.clone(), "22".into()),
                };
                let mut child = std::process::Command::new(crate::launch::openssh("ssh"))
                    .args(family.ssh_flag())
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

    /// Send all of `bytes`
    fn write_all(&self, bytes: &[u8]) -> std::io::Result<()> {
        match self {
            Link::Direct(socket) => (&**socket).write_all(bytes),
            Link::Jump(_, stdin) => stdin.lock().unwrap().write_all(bytes),
        }
    }

    /// Close the connection: the socket, or the jump host's ssh
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
    /// Send all of `buf`
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Link::write_all(self, buf)?;
        Ok(buf.len())
    }

    /// Nothing's held back to flush
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Telnet's codes (RFC 854 and friends)
mod telnet {
    /// Interpret As Command: the byte before every command
    pub const IAC: u8 = 255;
    /// Asks the other side to stop an option
    pub const DONT: u8 = 254;
    /// Asks the other side to use an option
    pub const DO: u8 = 253;
    /// Refuses an option, or says it's stopped
    pub const WONT: u8 = 252;
    /// Offers an option, or agrees to it
    pub const WILL: u8 = 251;
    /// Starts a subnegotiation (the terminal type, the window size)
    pub const SB: u8 = 250;
    /// Ends a subnegotiation
    pub const SE: u8 = 240;
    /// Option: the server echoes what's typed
    pub const ECHO: u8 = 1;
    /// Option: Suppress Go Ahead, so characters go as they're typed
    pub const SGA: u8 = 3;
    /// Option: the terminal type ("xterm")
    pub const TTYPE: u8 = 24;
    /// Option: Negotiate About Window Size
    pub const NAWS: u8 = 31;
}

/// Open a stream; returns its writing half and its reader
pub fn open(target: &Target, cols: u16, rows: u16) -> Result<(Stream, Box<dyn Read + Send>), String> {
    match target {
        Target::Telnet { host, port, via, family } => {
            let (link, input) = Link::open(host, *port, via, *family)?;
            let size = Arc::new(Mutex::new((cols, rows)));
            let naws = Arc::new(Mutex::new(false));
            let writer = Arc::new(Mutex::new(Box::new(TelnetWriter { link: link.clone() }) as Box<dyn Write + Send>));
            let reader = TelnetReader { input, answers: link.clone(), size: size.clone(), naws: naws.clone(), pending: Vec::new() };
            Ok((Stream { writer, kind: Kind::Telnet { link, size, naws } }, Box::new(reader)))
        }
        Target::Raw { host, port, via, family } => {
            let (link, input) = Link::open(host, *port, via, *family)?;
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

/// A TCP connection to host:port at an address of `family`, giving up after 10 seconds per address
fn connect(host: &str, port: u16, family: Family) -> Result<TcpStream, String> {
    let addresses = (host, port).to_socket_addrs().map_err(|e| format!("Couldn't find {host}: {e}"))?;
    let mut last = format!("Couldn't find an {}address for {host}", family.name());
    for address in addresses.filter(|a| family.takes(a)) {
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
    /// Send typed bytes as Telnet: an IAC doubled, Return as CR LF
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.link.write_all(&telnet_out(buf))?;
        Ok(buf.len())
    }

    /// Nothing's held back to flush
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
    /// The next data, with Telnet's commands answered and taken out; 0 when the connection ends
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
    /// Read what's arrived on the serial line, waiting 200 ms at a time so a closed tab ends it; 0 once closed, an error when the device is unplugged
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
    /// Write to the serial line
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = unsafe { libc::write(self.fd.as_raw_fd(), buf.as_ptr().cast(), buf.len()) };
        if n < 0 { Err(std::io::Error::last_os_error()) } else { Ok(n as usize) }
    }

    /// Nothing's held back to flush
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

/// Open a COM port on Windows (COM9, or a full `\\.\` name), with the line's settings. Windows lets one program at a
/// time have a port, so another one using it (PuTTY, a second tab) is said plainly.
#[cfg(windows)]
fn open_serial(device: &str, line: &SerialLine) -> Result<OwnedFd, String> {
    use windows_sys::Win32::Devices::Communication::{COMMTIMEOUTS, SetCommTimeouts};
    use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, OPEN_EXISTING};
    // COM10 and up only open by their \\.\ name, which works for COM1-9 too
    let path = if device.starts_with(r"\\") { device.to_string() } else { format!(r"\\.\{device}") };
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    // SAFETY: a NUL-terminated name, no sharing, security or template
    let handle = unsafe { CreateFileW(wide.as_ptr(), GENERIC_READ | GENERIC_WRITE, 0, std::ptr::null(), OPEN_EXISTING, 0, std::ptr::null_mut()) };
    if handle == INVALID_HANDLE_VALUE {
        let error = std::io::Error::last_os_error();
        return Err(match error.raw_os_error() {
            Some(2) | Some(3) => format!("{device} isn't there: is its cable plugged in? (Edit Session lists the COM ports present)"),
            Some(5) => format!("{device} is in use by another program (PuTTY, or another tab): close it there first."),
            _ => format!("Couldn't open {device}: {error}"),
        });
    }
    let fd = OwnedFd(handle);
    configure(&fd, line).map_err(|e| format!("Couldn't set {device} to {}: {e}", line.to_sercfg()))?;
    // a read gives what's there as soon as anything is, else returns empty after 25 ms, so closing the tab ends it;
    // the port isn't opened for overlapped I/O, so Windows runs one read or write at a time, and a short wait keeps
    // typing prompt
    let timeouts = COMMTIMEOUTS {
        ReadIntervalTimeout: u32::MAX,
        ReadTotalTimeoutMultiplier: u32::MAX,
        ReadTotalTimeoutConstant: 25,
        WriteTotalTimeoutMultiplier: 0,
        WriteTotalTimeoutConstant: 5000,
    };
    // SAFETY: the port opened above, and a timeouts structure that lives through the call
    if unsafe { SetCommTimeouts(fd.0, &timeouts) } == 0 {
        return Err(format!("Couldn't set {device}'s timeouts: {}", std::io::Error::last_os_error()));
    }
    Ok(fd)
}

/// Set a COM port's line, as PuTTY's -sercfg: the speed, data bits, parity, stop bits and flow control, binary, with
/// DTR and RTS raised (RTS handed to the flow control for RTS/CTS)
#[cfg(windows)]
fn configure(fd: &OwnedFd, line: &SerialLine) -> Result<(), std::io::Error> {
    use windows_sys::Win32::Devices::Communication::{DCB, GetCommState, SetCommState};
    // SAFETY: a DCB the size Windows expects, read from and written back to the open port
    unsafe {
        let mut dcb: DCB = std::mem::zeroed();
        dcb.DCBlength = std::mem::size_of::<DCB>() as u32;
        if GetCommState(fd.0, &mut dcb) == 0 {
            return Err(std::io::Error::last_os_error());
        }
        dcb.BaudRate = line.speed;
        dcb.ByteSize = line.data_bits.clamp(5, 8);
        // NOPARITY, ODDPARITY, EVENPARITY, MARKPARITY, SPACEPARITY
        dcb.Parity = match line.parity {
            'o' => 1,
            'e' => 2,
            'm' => 3,
            's' => 4,
            _ => 0,
        };
        // ONESTOPBIT or TWOSTOPBITS
        dcb.StopBits = if line.stop_bits == 2 { 2 } else { 0 };
        dcb._bitfield = dcb_flags(line);
        dcb.XonChar = 0x11;
        dcb.XoffChar = 0x13;
        if SetCommState(fd.0, &dcb) == 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// The DCB's flag bits for a line: binary (bit 0), parity checking (1), CTS output flow (2), DTR on (bits 4-5 = 1),
/// XON/XOFF out (8) and in (9), RTS on or handshaking (bits 12-13 = 1 or 2); DSR flow, error replacement, discarding
/// NULs and aborting on errors stay off
#[cfg(windows)]
fn dcb_flags(line: &SerialLine) -> u32 {
    let mut flags = 1;
    if line.parity != 'n' {
        flags |= 1 << 1;
    }
    flags |= 1 << 4;
    match line.flow {
        'X' => flags |= (1 << 8) | (1 << 9) | (1 << 12),
        'R' => flags |= (1 << 2) | (2 << 12),
        _ => flags |= 1 << 12,
    }
    flags
}

/// Send a break on a COM port: the line held low for 300 ms, as PuTTY's Break does
#[cfg(windows)]
fn send_break(fd: &OwnedFd) -> Result<(), String> {
    use windows_sys::Win32::Devices::Communication::{ClearCommBreak, SetCommBreak};
    // SAFETY: the open port
    unsafe {
        if SetCommBreak(fd.0) == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        std::thread::sleep(Duration::from_millis(300));
        ClearCommBreak(fd.0);
    }
    Ok(())
}

/// Reading a COM port on Windows: each read returns what's arrived, or nothing after the port's short timeout, so a
/// closed tab ends a read on a quiet line
#[cfg(windows)]
impl Read for FdIo {
    /// What's arrived on the port, or 0 bytes once the tab has closed
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use windows_sys::Win32::Storage::FileSystem::ReadFile;
        loop {
            if self.closed.load(Ordering::Relaxed) {
                return Ok(0);
            }
            let mut got = 0u32;
            let wanted = buf.len().min(u32::MAX as usize) as u32;
            // SAFETY: the open port, a buffer of `wanted` bytes, no overlapped I/O
            if unsafe { ReadFile(self.fd.0, buf.as_mut_ptr(), wanted, &mut got, std::ptr::null_mut()) } == 0 {
                // the device went away (unplugged)
                return Err(std::io::Error::other(format!("the serial device went away: {}", std::io::Error::last_os_error())));
            }
            if got > 0 {
                return Ok(got as usize);
            }
        }
    }
}

/// Writing to a COM port on Windows
#[cfg(windows)]
impl Write for FdIo {
    /// Write to the port, as much as it takes in one go
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        use windows_sys::Win32::Storage::FileSystem::WriteFile;
        let mut put = 0u32;
        let wanted = buf.len().min(u32::MAX as usize) as u32;
        // SAFETY: the open port, `wanted` bytes of the buffer, no overlapped I/O
        if unsafe { WriteFile(self.fd.0, buf.as_ptr(), wanted, &mut put, std::ptr::null_mut()) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(put as usize)
    }

    /// Nothing's held back to flush
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The COM ports present on Windows, as Device Manager lists them (HKLM\HARDWARE\DEVICEMAP\SERIALCOMM, which Windows
/// keeps to the ports plugged in now), in number order: COM3 before COM10
#[cfg(windows)]
pub fn serial_devices() -> Vec<String> {
    use std::os::windows::process::CommandExt;
    /// Run reg without a console window flashing up
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let Ok(output) = std::process::Command::new("reg")
        .args(["query", r"HKLM\HARDWARE\DEVICEMAP\SERIALCOMM"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    else {
        return Vec::new();
    };
    // "    \Device\Silabser0    REG_SZ    COM9"
    let mut ports: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once("    REG_SZ    ").map(|(_, port)| port.trim().to_string()))
        .filter(|port| !port.is_empty())
        .collect();
    ports.sort_by_key(|p| (p.trim_start_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().unwrap_or(u32::MAX), p.clone()));
    ports.dedup();
    ports
}

/// The serial devices on this Mac or Linux computer, for choosing which one a COM name is: the Mac's `/dev/cu.*`,
/// Linux's USB adapters and boards (`/dev/ttyUSB*`, `/dev/ttyACM*`) ([`is_serial_device`])
#[cfg(not(windows))]
pub fn serial_devices() -> Vec<String> {
    let mac = cfg!(target_os = "macos");
    let mut devices: Vec<String> = std::fs::read_dir("/dev")
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|name| is_serial_device(name, mac))
                .map(|name| format!("/dev/{name}"))
                .collect()
        })
        .unwrap_or_default();
    devices.sort();
    devices
}

/// Whether a name in /dev is a serial device to offer. The Mac's are `cu.*` (its callout devices, which open without
/// waiting for a modem's carrier), but for its Bluetooth port and debug console. Linux's are a USB adapter's
/// `ttyUSB*`, a USB board's (Arduino, a console cable with CDC) `ttyACM*`, a Raspberry Pi's own `ttyAMA*`, and a
/// Bluetooth one's `rfcomm*`; not the `ttyS*` every PC has, mostly with nothing on them. Linux has no `cu.*`, so before
/// it had its own names it listed none
#[cfg_attr(windows, allow(dead_code))]
fn is_serial_device(name: &str, mac: bool) -> bool {
    if mac {
        name.starts_with("cu.") && name != "cu.Bluetooth-Incoming-Port" && name != "cu.debug-console"
    } else {
        ["ttyUSB", "ttyACM", "ttyAMA", "rfcomm"].iter().any(|kind| name.strip_prefix(kind).is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use telnet::*;

    #[test]
    fn serial_devices_are_the_macs_callouts_and_linuxs_usb_and_board_ports() {
        let names = ["cu.usbserial-1", "cu.Bluetooth-Incoming-Port", "cu.debug-console", "tty.usbserial-1", "ttyUSB0", "ttyACM1", "ttyAMA0", "rfcomm0", "ttyS0", "tty1", "ttyUSB"];
        let mac: Vec<&str> = names.iter().copied().filter(|n| is_serial_device(n, true)).collect();
        let linux: Vec<&str> = names.iter().copied().filter(|n| is_serial_device(n, false)).collect();
        assert_eq!(mac, ["cu.usbserial-1"]);
        assert_eq!(linux, ["ttyUSB0", "ttyACM1", "ttyAMA0", "rfcomm0"]);
    }

    #[test]
    fn a_connection_tries_only_the_addresses_of_its_internet_protocol_version() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(connect("127.0.0.1", port, Family::V4).is_ok());
        assert!(connect("127.0.0.1", port, Family::Any).is_ok());
        // 127.0.0.1 has no IPv6 address
        let error = connect("127.0.0.1", port, Family::V6).unwrap_err();
        assert_eq!(error, "Couldn't find an IPv6 address for 127.0.0.1");
    }

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

    /// A real serial device (SXT_SERIAL_PORT: COM9, or /dev/cu.usbserial-1; SXT_SERIAL_CFG for -sercfg, default
    /// 9600,8,n,1,N): open it, press Return, and print what comes back, e.g. a switch's prompt:
    /// `cargo test real_serial_port -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_serial_port() {
        let device = std::env::var("SXT_SERIAL_PORT").expect("SXT_SERIAL_PORT");
        let mut line = SerialLine::default();
        line.apply(&std::env::var("SXT_SERIAL_CFG").unwrap_or_else(|_| "9600,8,n,1,N".into())).unwrap();
        let (stream, mut reader) = open(&Target::Serial { device: device.clone(), line }, 80, 24).unwrap();
        let got = Arc::new(Mutex::new(Vec::new()));
        let into = got.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            while let Ok(n) = reader.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                into.lock().unwrap().extend_from_slice(&buffer[..n]);
            }
        });
        stream.writer.lock().unwrap().write_all(b"\r").unwrap();
        std::thread::sleep(Duration::from_secs(2));
        stream.close();
        let text = String::from_utf8_lossy(&got.lock().unwrap()).to_string();
        println!("{device} answered: {text:?}");
        assert!(!text.is_empty(), "{device} said nothing to Return");
    }
}
