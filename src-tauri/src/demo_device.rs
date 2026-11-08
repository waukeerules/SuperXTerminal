//! Demo mode's stand-in devices (`demo.rs`): this program, run in a tab's pseudo terminal in place of ssh, acting as
//! a Cisco IOS switch, a Juniper, a Linux server or this computer's shell. It prints a login banner and a prompt, and
//! answers commands from the canned output in `src-tauri/demo/devices/` (ios.txt has the files' form), so the tab
//! shows realistic output, highlighted as a real device's would be, without anything real behind it.
//!
//! main.rs runs it when [`KIND_VAR`] is set, as it runs askpass. Lines are read as the pseudo terminal's line
//! discipline hands them over (it echoes what's typed), so there's no tab completion or `?` help. A few commands do
//! more than print: IOS's and Junos' configuration mode changes the prompt, `ping` replies at a steady pace, `cd`
//! changes a Linux prompt's folder, `clear` clears, and `exit` ends the session. Anything else unknown gets the
//! device's own complaint. Some sessions run a command or two as they open (see [`OPENINGS`]), so a fresh demo
//! window has output to show without typing.

use std::io::{BufRead, Write};
use std::time::Duration;

/// Set on the stand-in's command: what it acts as ("ios", "junos", "linux" or "shell")
pub const KIND_VAR: &str = "SXT_DEMO_DEVICE";
/// The session's host, from which its address is made up when the host is a name
pub const HOST_VAR: &str = "SXT_DEMO_HOST";
/// The session's name, shown in the prompt
pub const NAME_VAR: &str = "SXT_DEMO_NAME";
/// The session's icon, which picks a Linux server's release (Ubuntu, Red Hat, Debian...)
pub const ICON_VAR: &str = "SXT_DEMO_ICON";

/// Commands some sessions run as they open, by session name, so the demo's first screenshots have output in them
const OPENINGS: &[(&str, &[&str])] = &[
    ("core-sw-01", &["show ip interface brief"]),
    ("dist-sw-01", &["show interfaces status"]),
    ("lon-sw-01", &["show logging"]),
    ("edge-srx-01", &["show interfaces terse"]),
    ("lab-mx-01", &["show bgp summary"]),
    ("web-01", &["uptime", "df -h"]),
    ("db-01", &["df -h"]),
    ("k8s-ctl-01", &["kubectl get pods"]),
    ("lab-docker", &["docker ps"]),
];

/// Where a Cisco or Juniper is: its prompt says
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Exec,
    Config,
    /// IOS's interface configuration, `(config-if)#`
    ConfigIf,
}

/// What a line typed brings
#[derive(Debug, PartialEq)]
enum Reply {
    /// Text to print, then the prompt again
    Text(String),
    /// Replies to a ping, printed one at a time, `pace` apart, between a header and a summary
    Ping { header: String, replies: Vec<String>, footer: String, pace: Duration },
    /// The session ends, after printing this
    Exit(String),
}

/// One stand-in device
struct Device {
    /// "ios", "junos", "linux" or "shell"
    kind: String,
    /// The name in its prompt
    name: String,
    /// The Linux prompt's folder ("~" at the start)
    cwd: String,
    mode: Mode,
    /// Commands and their answers, from the kind's file: each answer with every way to ask for it
    answers: Vec<(Vec<String>, String)>,
    /// The values for the files' {placeholders}
    values: Vec<(&'static str, String)>,
}

/// The canned output for a kind of device
fn file_of(kind: &str) -> &'static str {
    match kind {
        "ios" => include_str!("../demo/devices/ios.txt"),
        "junos" => include_str!("../demo/devices/junos.txt"),
        "shell" => include_str!("../demo/devices/shell.txt"),
        _ => include_str!("../demo/devices/linux.txt"),
    }
}

/// A file's answers: each run of "## command" lines and the text after them, up to the next "##"; the comment lines
/// ("# ...") before the first are skipped
fn parse(file: &str) -> Vec<(Vec<String>, String)> {
    let mut answers: Vec<(Vec<String>, String)> = Vec::new();
    let mut asking = false;
    for line in file.lines() {
        if let Some(command) = line.strip_prefix("## ") {
            if !asking {
                answers.push((Vec::new(), String::new()));
            }
            answers.last_mut().unwrap().0.push(normalise(command));
            asking = true;
        } else if let Some((_, text)) = answers.last_mut() {
            asking = false;
            text.push_str(line);
            text.push('\n');
        }
    }
    answers
}

/// A command with its spaces evened out and in lower case, as it's matched
fn normalise(command: &str) -> String {
    command.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Whether what was typed asks for this command, each word shortened as IOS and Junos allow ("sh ip int br" for
/// "show ip interface brief"), the same number of words
fn abbreviates(typed: &str, command: &str) -> bool {
    let (typed, command): (Vec<&str>, Vec<&str>) = (typed.split(' ').collect(), command.split(' ').collect());
    typed.len() == command.len() && typed.iter().zip(&command).all(|(t, c)| c.starts_with(t))
}

/// A Linux release's name, short name, version and kernel, from the session's icon
fn release(icon: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    match icon.split('#').next().unwrap_or_default() {
        "redhat" | "database" => ("Red Hat Enterprise Linux 9.4 (Plow)", "Red Hat Enterprise Linux", "9.4", "5.14.0-427.13.1.el9_4.x86_64"),
        "rockylinux" | "centos" | "almalinux" => ("Rocky Linux 9.4 (Blue Onyx)", "Rocky Linux", "9.4", "5.14.0-427.16.1.el9_4.x86_64"),
        "debian" | "raspberrypi" | "storage" => ("Debian GNU/Linux 12 (bookworm)", "Debian GNU/Linux", "12", "6.1.0-26-amd64"),
        "proxmox" => ("Debian GNU/Linux 12 (bookworm)", "Debian GNU/Linux", "12", "6.8.12-2-pve"),
        _ => ("Ubuntu 24.04.1 LTS", "Ubuntu", "24.04", "6.8.0-48-generic"),
    }
}

/// The host's own IPv4 address, else one made up from its name (10.20.x.y), the same each time
fn address(host: &str) -> String {
    if host.parse::<std::net::Ipv4Addr>().is_ok() {
        return host.to_string();
    }
    let sum = host.bytes().fold(7u32, |sum, b| sum.wrapping_mul(31).wrapping_add(b as u32));
    format!("10.20.{}.{}", 10 + sum % 80, 2 + (sum / 80) % 250)
}

impl Device {
    /// A device of this kind for this session, its clock at `now`
    fn new(kind: &str, name: &str, host: &str, icon: &str, now: chrono::DateTime<chrono::Local>) -> Device {
        let (os, os_name, os_version, kernel) = release(icon);
        let name = if name.is_empty() { host.split('.').next().unwrap_or("demo") } else { name };
        let uptime = if kind == "ios" { "6 weeks, 6 days, 6 hours, 27 minutes" } else { "48 days,  6:27" };
        let values = vec![
            ("host", name.to_string()),
            ("ip", address(host)),
            ("date", now.format("%b %e").to_string()),
            ("year", now.format("%Y").to_string()),
            ("iso_date", now.format("%Y-%m-%d").to_string()),
            ("time", now.format("%H:%M:%S").to_string()),
            ("weekday", now.format("%a").to_string()),
            ("uptime", uptime.to_string()),
            ("os", os.to_string()),
            ("os_name", os_name.to_string()),
            ("os_version", os_version.to_string()),
            ("kernel", kernel.to_string()),
        ];
        Device {
            kind: kind.to_string(),
            name: name.to_string(),
            cwd: "~".into(),
            mode: Mode::Exec,
            answers: parse(file_of(kind)),
            values,
        }
    }

    /// Text with its {placeholders} filled in, its lines ending CR LF (a pseudo console on Windows doesn't add the CR).
    /// `{ip:15}` is the value padded to 15 characters, to keep a table's columns lined up
    fn fill(&self, text: &str) -> String {
        let mut text = text.to_string();
        for (key, value) in &self.values {
            text = text.replace(&format!("{{{key}}}"), value);
            let padded = format!("{{{key}:");
            while let Some(start) = text.find(&padded) {
                let Some(end) = text[start..].find('}').map(|e| start + e) else { break };
                let width: usize = text[start + padded.len()..end].parse().unwrap_or(0);
                text.replace_range(start..=end, &format!("{value:width$}"));
            }
        }
        text.replace("\r\n", "\n").replace('\n', "\r\n")
    }

    /// The canned answer to a command, if the file has one: exact first, then (on a Cisco or Juniper) shortened
    fn canned(&self, typed: &str) -> Option<String> {
        let exact = self.answers.iter().find(|(asks, _)| asks.iter().any(|a| a == typed));
        let network = matches!(self.kind.as_str(), "ios" | "junos");
        let short = || self.answers.iter().find(|(asks, _)| asks.iter().any(|a| !a.starts_with('@') && abbreviates(typed, a)));
        exact.or_else(|| if network { short() } else { None }).map(|(_, text)| self.fill(text))
    }

    /// The prompt, as the device draws it
    fn prompt(&self) -> String {
        match (self.kind.as_str(), self.mode) {
            ("ios", Mode::Exec) => format!("{}#", self.name),
            ("ios", Mode::Config) => format!("{}(config)#", self.name),
            ("ios", Mode::ConfigIf) => format!("{}(config-if)#", self.name),
            ("junos", Mode::Exec) => format!("netops@{}> ", self.name),
            ("junos", _) => format!("netops@{}# ", self.name),
            ("shell", _) if cfg!(windows) => "PS C:\\Users\\demo> ".into(),
            ("shell", _) => format!("demo@Demo-MacBook-Pro {} % ", self.cwd),
            _ => format!("\x1b[1;32mdemo@{}\x1b[0m:\x1b[1;34m{}\x1b[0m$ ", self.name, self.cwd),
        }
    }

    /// What the banner says as the session opens
    fn banner(&self) -> String {
        self.canned("@banner").unwrap_or_default()
    }

    /// The reply to a line typed
    fn answer(&mut self, line: &str) -> Reply {
        let typed = normalise(line);
        if typed.is_empty() {
            return Reply::Text(String::new());
        }
        match self.kind.as_str() {
            "ios" => self.ios(&typed),
            "junos" => self.junos(&typed),
            _ => self.shell(&typed, line.trim()),
        }
    }

    /// A Cisco IOS device's reply: configuration mode and ping, else canned, else "% Invalid input"
    fn ios(&mut self, typed: &str) -> Reply {
        let first = typed.split(' ').next().unwrap_or_default();
        let is = |command: &str| abbreviates(typed, command);
        if self.mode != Mode::Exec {
            if is("end") {
                self.mode = Mode::Exec;
            } else if is("exit") {
                self.mode = if self.mode == Mode::ConfigIf { Mode::Config } else { Mode::Exec };
            } else if let Some(command) = typed.strip_prefix("do ") {
                return self.ios_exec(command);
            } else if "interface".starts_with(first) && first.len() >= 3 {
                self.mode = Mode::ConfigIf;
            }
            // other configuration lines are taken as they are
            return Reply::Text(String::new());
        }
        if is("configure terminal") {
            self.mode = Mode::Config;
            return Reply::Text("Enter configuration commands, one per line.  End with CNTL/Z.\r\n".into());
        }
        if is("exit") || is("logout") || is("quit") {
            return Reply::Exit(String::new());
        }
        self.ios_exec(typed)
    }

    /// An IOS command at the `#` prompt (or after `do`)
    fn ios_exec(&self, typed: &str) -> Reply {
        if let Some(target) = typed.strip_prefix("ping ") {
            return Reply::Ping {
                header: format!("Type escape sequence to abort.\r\nSending 5, 100-byte ICMP Echos to {target}, timeout is 2 seconds:\r\n"),
                replies: vec!["!".into(); 5],
                footer: "\r\nSuccess rate is 100 percent (5/5), round-trip min/avg/max = 1/1/2 ms\r\n".into(),
                pace: Duration::from_millis(120),
            };
        }
        self.canned(typed).map(Reply::Text).unwrap_or_else(|| {
            let caret = " ".repeat(self.prompt().len());
            Reply::Text(format!("{caret}^\r\n% Invalid input detected at '^' marker.\r\n\r\n"))
        })
    }

    /// A Juniper's reply: configuration mode, commit and ping, else canned, else "unknown command"
    fn junos(&mut self, typed: &str) -> Reply {
        let is = |command: &str| abbreviates(typed, command);
        if self.mode != Mode::Exec {
            if is("exit") || is("quit") {
                self.mode = Mode::Exec;
                return Reply::Text("Exiting configuration mode\r\n\r\n".into());
            }
            if is("commit") {
                return Reply::Text("commit complete\r\n\r\n[edit]\r\n".into());
            }
            return Reply::Text("\r\n[edit]\r\n".into());
        }
        if is("configure") {
            self.mode = Mode::Config;
            return Reply::Text("Entering configuration mode\r\n\r\n[edit]\r\n".into());
        }
        if is("exit") || is("quit") {
            return Reply::Exit(String::new());
        }
        if let Some(target) = typed.strip_prefix("ping ") {
            let target = target.split(' ').next().unwrap_or_default();
            return self.unix_ping(target, 5);
        }
        self.canned(typed).map(Reply::Text).unwrap_or_else(|| {
            let caret = " ".repeat(self.prompt().len());
            Reply::Text(format!("{caret}^\r\nunknown command.\r\n"))
        })
    }

    /// A Linux server's or this computer's shell's reply: exit, cd, clear, echo and ping, else canned, else
    /// "command not found"
    fn shell(&mut self, typed: &str, line: &str) -> Reply {
        let typed = typed.strip_prefix("sudo ").unwrap_or(typed);
        let line = line.strip_prefix("sudo ").unwrap_or(line);
        let first = typed.split(' ').next().unwrap_or_default();
        match first {
            "exit" | "logout" => return Reply::Exit(if self.kind == "linux" { "logout\r\n".into() } else { String::new() }),
            "clear" | "cls" => return Reply::Text("\x1b[H\x1b[2J\x1b[3J".into()),
            "cd" => {
                let to = line.split_whitespace().nth(1).unwrap_or("~");
                self.cwd = match to {
                    "~" | "" => "~".into(),
                    ".." => "~".into(),
                    other if other.starts_with('/') || other.starts_with('~') => other.to_string(),
                    other => format!("{}/{other}", self.cwd.trim_end_matches('/')),
                };
                return Reply::Text(String::new());
            }
            "echo" => return Reply::Text(format!("{}\r\n", line.split_once(' ').map(|(_, rest)| rest).unwrap_or(""))),
            "ping" => {
                let words: Vec<&str> = typed.split(' ').skip(1).collect();
                let count = words.windows(2).find(|w| w[0] == "-c").and_then(|w| w[1].parse().ok()).unwrap_or(4).clamp(1, 20);
                let target = words.iter().filter(|w| !w.starts_with('-') && w.parse::<u32>().is_err()).last().copied().unwrap_or("localhost");
                return self.unix_ping(target, count);
            }
            _ => {}
        }
        self.canned(typed).map(Reply::Text).unwrap_or_else(|| {
            let shown = line.split_whitespace().next().unwrap_or_default();
            Reply::Text(match self.kind.as_str() {
                "shell" if cfg!(windows) => format!("{shown}: The term '{shown}' is not recognized as a name of a cmdlet, function, script file, or executable program.\r\n"),
                "shell" => format!("zsh: command not found: {shown}\r\n"),
                _ => format!("{shown}: command not found\r\n"),
            })
        })
    }

    /// A ping as Unix (and Junos) prints it: `count` replies from the target, made up from its name
    fn unix_ping(&self, target: &str, count: usize) -> Reply {
        let ip = address(target);
        let header = format!("PING {target} ({ip}): 56 data bytes\r\n");
        let replies = (0..count).map(|seq| format!("64 bytes from {ip}: icmp_seq={seq} ttl=62 time={}.{:03} ms\r\n", 1 + seq % 2, 211 + seq * 87)).collect();
        let footer = format!("\r\n--- {target} ping statistics ---\r\n{count} packets transmitted, {count} packets received, 0.0% packet loss\r\nround-trip min/avg/max/stddev = 1.211/1.598/2.472/0.411 ms\r\n");
        Reply::Ping { header, replies, footer, pace: Duration::from_millis(400) }
    }
}

/// The canned answer of a device of this kind and name to a command, filled in as the device would print it (file
/// transfer's stand-in reads a switch's `dir` this way, sftp.rs); None when it has none
pub fn canned_answer(kind: &str, name: &str, host: &str, command: &str) -> Option<String> {
    Device::new(kind, name, host, "", chrono::Local::now()).canned(&normalise(command))
}

/// Print a reply; false when the session ends
fn print(out: &mut impl Write, reply: Reply) -> bool {
    let going = !matches!(reply, Reply::Exit(_));
    match reply {
        Reply::Text(text) | Reply::Exit(text) => {
            let _ = out.write_all(text.as_bytes());
        }
        Reply::Ping { header, replies, footer, pace } => {
            let _ = out.write_all(header.as_bytes());
            for reply in replies {
                let _ = out.flush();
                std::thread::sleep(pace);
                let _ = out.write_all(reply.as_bytes());
            }
            let _ = out.write_all(footer.as_bytes());
        }
    }
    let _ = out.flush();
    going
}

/// Run as a stand-in device if this program was started as one; true when it was (and has finished)
pub fn run() -> bool {
    let Ok(kind) = std::env::var(KIND_VAR) else { return false };
    let var = |name: &str| std::env::var(name).unwrap_or_default();
    let mut device = Device::new(&kind, &var(NAME_VAR), &var(HOST_VAR), &var(ICON_VAR), chrono::Local::now());
    // Ctrl+C ends a command on a real device, not the session: the stand-in has nothing running to stop
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_IGN);
    }
    let mut out = std::io::stdout();
    let _ = out.write_all(device.banner().as_bytes());
    let opening = OPENINGS.iter().find(|(name, _)| *name == device.name).map(|(_, commands)| *commands).unwrap_or_default();
    for command in opening {
        let _ = write!(out, "{}{command}\r\n", device.prompt());
        let reply = device.answer(command);
        print(&mut out, reply);
    }
    let _ = out.write_all(device.prompt().as_bytes());
    let _ = out.flush();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let reply = device.answer(&line);
        if !print(&mut out, reply) {
            break;
        }
        let _ = out.write_all(device.prompt().as_bytes());
        let _ = out.flush();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A device of this kind, its clock at a fixed time
    fn device(kind: &str, name: &str, host: &str, icon: &str) -> Device {
        use chrono::TimeZone;
        Device::new(kind, name, host, icon, chrono::Local.with_ymd_and_hms(2026, 11, 7, 9, 41, 7).unwrap())
    }

    /// The text of a reply that's text
    fn text(reply: Reply) -> String {
        match reply {
            Reply::Text(text) => text,
            other => panic!("not text: {other:?}"),
        }
    }

    #[test]
    fn a_cisco_answers_commands_shortened_as_ios_allows_with_its_own_name_and_address() {
        let mut sw = device("ios", "core-sw-01", "core-sw-01.example.net", "coreswitch");
        let brief = text(sw.answer("sh ip int br"));
        assert!(brief.starts_with("Interface "), "{brief}");
        // the address keeps the table's columns lined up
        let address = format!("{:15}", address("core-sw-01.example.net"));
        assert!(brief.contains(&format!("Vlan10                 {address} YES NVRAM  up                    up\r\n")), "{brief}");
        assert!(text(sw.answer("show running-config")).contains("hostname core-sw-01\r\n"));
        assert!(text(sw.answer("show clock")).contains("09:41:07"));
        assert!(text(sw.answer("show bananas")).contains("% Invalid input detected at '^' marker."));
    }

    #[test]
    fn a_ciscos_prompt_follows_it_into_configuration_and_out() {
        let mut sw = device("ios", "lab-sw-01", "lab-sw-01.lab.example.net", "switch");
        assert_eq!(sw.prompt(), "lab-sw-01#");
        sw.answer("conf t");
        assert_eq!(sw.prompt(), "lab-sw-01(config)#");
        sw.answer("int gi1/0/5");
        assert_eq!(sw.prompt(), "lab-sw-01(config-if)#");
        assert_eq!(text(sw.answer("no shutdown")), "");
        assert!(text(sw.answer("do show vlan brief")).contains("management"));
        sw.answer("end");
        assert_eq!(sw.prompt(), "lab-sw-01#");
        assert_eq!(sw.answer("exit"), Reply::Exit(String::new()));
    }

    #[test]
    fn a_juniper_and_a_linux_server_answer_in_their_own_way() {
        let mut srx = device("junos", "edge-srx-01", "edge-srx-01.example.net", "firewall");
        assert_eq!(srx.prompt(), "netops@edge-srx-01> ");
        assert!(text(srx.answer("show int terse")).contains("ge-0/0/0.0              up    up   inet     203.0.113.2/30"));
        assert!(text(srx.answer("show bananas")).ends_with("unknown command.\r\n"));

        let mut web = device("linux", "web-01", "web-01.example.net", "ubuntu");
        assert!(web.banner().contains("Welcome to Ubuntu 24.04.1 LTS"));
        assert!(text(web.answer("sudo df -h")).contains("/var/lib/postgresql"));
        // Linux commands aren't shortened
        assert_eq!(text(web.answer("df")), "df: command not found\r\n");
        web.answer("cd /etc");
        assert!(web.prompt().contains("/etc"));
        assert_eq!(web.answer("exit"), Reply::Exit("logout\r\n".into()));

        let db = device("linux", "db-01", "db-01.example.net", "database");
        assert!(db.banner().contains("Red Hat Enterprise Linux 9.4"));
    }

    #[test]
    fn every_answer_in_the_files_is_found_by_its_command() {
        for kind in ["ios", "junos", "linux", "shell"] {
            let mut d = device(kind, "demo-01", "demo-01.example.net", "");
            for (asks, _) in parse(file_of(kind)) {
                for ask in asks.iter().filter(|a| !a.starts_with('@')) {
                    let reply = d.answer(ask);
                    assert!(matches!(reply, Reply::Text(_)), "{kind}: {ask} gave {reply:?}");
                    assert!(!text(reply).contains("Invalid input") && d.canned(ask).is_some(), "{kind}: {ask} isn't answered");
                }
            }
        }
    }

    #[test]
    fn a_ping_replies_one_at_a_time() {
        let mut sw = device("ios", "core-sw-01", "core-sw-01.example.net", "coreswitch");
        match sw.answer("ping 10.20.10.1") {
            Reply::Ping { header, replies, footer, .. } => {
                assert!(header.contains("Sending 5, 100-byte ICMP Echos to 10.20.10.1"));
                assert_eq!(replies.concat(), "!!!!!");
                assert!(footer.contains("Success rate is 100 percent (5/5)"));
            }
            other => panic!("not a ping: {other:?}"),
        }
    }
}
