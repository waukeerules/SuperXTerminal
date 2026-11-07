//! Server load in the status bar: CPU, memory, the root filesystem and network traffic of the Linux
//! server an SSH tab is connected to, every few seconds, for the tab you're looking at.
//!
//! Only for a profile with "Show server load" on (off by default: a Cisco switch has no shell to run it, and would log
//! the attempt as a command). How the readings reach the server ([`LoadVia`]) differs:
//!
//! - **Mac:** the tab's ssh is started as an OpenSSH connection-sharing master (`ControlMaster`, `launch.rs`), and
//!   each reading runs over that same connection as a new channel: no second login. One short command per channel
//!   ([`READ`]), not a loop left running: a master waits for its other channels before it ends, so a loop would keep
//!   the tab open after you logged out.
//! - **Windows:** its OpenSSH can't share a connection (no ControlMaster), so the readings have a second login of
//!   their own, with the tab's ssh arguments and `BatchMode`: it gets in with a key (a key file, ssh-agent) and never
//!   asks for anything, so a server that wants a password gives no readings. That connection runs [`LOOP`], a reading
//!   every few seconds, so it's one login while the tab is in front, not one per reading; being its own connection,
//!   it doesn't hold the tab's open, and it ends when the tab leaves the front.
//!
//! The command only reads (`cat`, `df`): nothing on the server changes. A server without /proc/stat (BSD, macOS, a
//! network device) gives no CPU line, and the tab's readings stop, saying so.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

/// What each reading runs on the server: its counters, after a marker. Read only
pub const READ: &str = "echo @@; cat /proc/stat /proc/meminfo /proc/net/dev; df -P / | tail -n 1";

/// What the second login runs on Windows: [`READ`]'s reading every 3 seconds ([`EVERY`]), each between a line `@@`
/// and a line `@@end`, until the connection ends. In `sh` whatever the login shell (fish has no `while :`)
pub const LOOP: &str = "exec sh -c 'while :; do echo @@; cat /proc/stat /proc/meminfo /proc/net/dev; df -P / | tail -n 1; echo @@end; sleep 3; done'";

/// Seconds between readings: often enough to follow, seldom enough to cost the server nothing
const EVERY: Duration = Duration::from_secs(3);

/// How long one reading may take before it's given up (a busy server, a stalled connection)
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Readings in a row that fail with the connection there before the tab's readings stop: a server that refuses the
/// extra channel (a device that allows one session per connection) isn't asked forever
const MAX_FAILURES: u32 = 5;

/// The counters one reading gave, as the server keeps them (totals since it started)
#[derive(Clone, Debug, PartialEq)]
pub struct Counters {
    /// CPU time busy and in all, in the kernel's ticks: the `cpu` line of /proc/stat
    pub cpu: Option<(u64, u64)>,
    /// Memory in use, as a share of all of it: (MemTotal − MemAvailable) ÷ MemTotal
    pub mem: Option<f64>,
    /// The root filesystem's Capacity, as `df -P /` says it
    pub disk: Option<u8>,
    /// Bytes received and sent over the server's own interfaces (see [`counted_interface`])
    pub net: Option<(u64, u64)>,
}

/// The CPU's busy and total time from /proc/stat's `cpu` line. Busy leaves out idle and waiting for the disk (iowait),
/// as `top` does; guest time isn't added, as the kernel already counts it in user time
fn cpu_line(line: &str) -> Option<(u64, u64)> {
    let fields: Vec<u64> = line.split_whitespace().skip(1).map(|f| f.parse().unwrap_or(0)).collect();
    if fields.len() < 4 {
        return None;
    }
    // user nice system idle iowait irq softirq steal (guest guest_nice after, inside user and nice already)
    let total: u64 = fields.iter().take(8).sum();
    let idle = fields[3] + fields.get(4).copied().unwrap_or(0);
    Some((total - idle, total))
}

/// Whether an interface's traffic counts as the server's: not loopback, and not the virtual ones a container or VM
/// host has (Docker's veth and br- pairs, Kubernetes' cni0 and flannel, libvirt's virbr and vnet, tunnels). Their
/// traffic also crosses the real interface, so adding them up would count it two or three times: on a Docker host the
/// total would be several times what's on the wire. A bonded interface's members aren't told apart yet
pub fn counted_interface(name: &str) -> bool {
    const VIRTUAL: [&str; 15] =
        ["veth", "br-", "docker", "cni", "flannel", "virbr", "vnet", "cali", "weave", "kube", "vxlan", "tun", "tap", "wg", "tailscale"];
    name != "lo" && !VIRTUAL.iter().any(|prefix| name.starts_with(prefix))
}

/// One reading's counters, from what [`READ`] printed: everything after the last `@@`. None when it isn't a Linux
/// server's (no `cpu` line: a BSD or macOS host, or a device that ran something else)
pub fn parse(output: &str) -> Option<Counters> {
    let text = output.rsplit("@@").next().unwrap_or(output);
    let mut cpu = None;
    let (mut total, mut available) = (None::<u64>, None::<u64>);
    let (mut rx, mut tx, mut interfaces) = (0u64, 0u64, 0);
    let mut disk = None;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with("cpu ") {
            cpu = cpu_line(line);
        } else if let Some(rest) = line.strip_prefix("MemTotal:") {
            total = rest.split_whitespace().next().and_then(|v| v.parse().ok());
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            available = rest.split_whitespace().next().and_then(|v| v.parse().ok());
        } else if line.ends_with(" /") || line.ends_with("\t/") {
            // df -P /'s line: "filesystem 1024-blocks used available capacity mounted-on"; before the interfaces, as
            // a network filesystem's name has a colon too (nas:/export)
            disk = line.split_whitespace().rev().nth(1).and_then(|c| c.trim_end_matches('%').parse().ok());
        } else if let Some((name, counters)) = line.split_once(':') {
            // /proc/net/dev: "  eth0: rx_bytes rx_packets ... tx_bytes ..." (the 1st and 9th of 16 numbers); the
            // other lines with a colon (/proc/meminfo's) have far fewer numbers
            let name = name.trim();
            let numbers: Vec<u64> = counters.split_whitespace().filter_map(|n| n.parse().ok()).collect();
            if numbers.len() >= 16 && !name.is_empty() && !name.contains(char::is_whitespace) && counted_interface(name) {
                rx += numbers[0];
                tx += numbers[8];
                interfaces += 1;
            }
        }
    }
    cpu?;
    let mem = match (total, available) {
        (Some(total), Some(available)) if total > 0 => Some(total.saturating_sub(available) as f64 / total as f64 * 100.0),
        _ => None,
    };
    Some(Counters { cpu, mem, disk, net: (interfaces > 0).then_some((rx, tx)) })
}

/// What the status bar shows from a reading, and the one before it for what's a rate: CPU busy and network traffic
/// are changes since the last reading, so the first reading has neither
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Load {
    /// Percent of CPU time busy since the last reading
    pub cpu: Option<f64>,
    /// Percent of memory in use
    pub mem: Option<f64>,
    /// Percent of the root filesystem used
    pub disk: Option<u8>,
    /// Bytes a second received (↓) and sent (↑) since the last reading
    pub down: Option<f64>,
    pub up: Option<f64>,
}

/// The load shown for a reading, `seconds` after the one before (if there was one). A counter that went backwards
/// (the server restarted, an interface went away) gives no rate this time rather than a nonsense one
pub fn load(before: Option<&Counters>, now: &Counters, seconds: f64) -> Load {
    let cpu = match (before.and_then(|b| b.cpu), now.cpu) {
        (Some((busy0, total0)), Some((busy1, total1))) if total1 > total0 && busy1 >= busy0 => {
            Some((busy1 - busy0) as f64 / (total1 - total0) as f64 * 100.0)
        }
        _ => None,
    };
    let (down, up) = match (before.and_then(|b| b.net), now.net) {
        (Some((rx0, tx0)), Some((rx1, tx1))) if seconds > 0.0 && rx1 >= rx0 && tx1 >= tx0 => {
            (Some((rx1 - rx0) as f64 / seconds), Some((tx1 - tx0) as f64 / seconds))
        }
        _ => (None, None),
    };
    Load { cpu, mem: now.mem, disk: now.disk, down, up }
}

/// How a tab's readings reach its server: over its own shared connection (the Mac), or a second login (Windows)
#[derive(Clone, Debug, PartialEq)]
pub enum LoadVia {
    /// The master's socket ([`master_args`]); each reading a channel over it
    Shared(PathBuf),
    /// The ssh arguments of a second login that runs [`LOOP`] ([`separate_args`])
    Separate(Vec<String>),
}

/// What a tab's watch tells its page
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LoadEvent {
    /// A reading
    Load(Load),
    /// No readings for this tab: the server isn't Linux, or won't give the extra channel; `why` for the status bar
    Unavailable { why: String },
}

/// The ssh arguments that make a tab's connection the sharing master, at `path`: ControlPersist off, so it ends with
/// the tab's session as usual. Before the destination, as every option
pub fn master_args(path: &Path) -> Vec<String> {
    vec![
        "-o".into(),
        "ControlMaster=yes".into(),
        "-o".into(),
        format!("ControlPath={}", path.display()),
        "-o".into(),
        "ControlPersist=no".into(),
    ]
}

/// A new place for a tab's sharing socket: in the temporary folder, short (a socket's path has a limit of about 100
/// characters on a Mac) and its own, so no other ssh's master is taken for it
pub fn new_socket_path() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("sxt-{}-{n}.sock", std::process::id()))
}

/// The second login's ssh arguments (Windows), from the tab's own `args` (its options, then the destination): no
/// terminal (-T for -tt); BatchMode, so it uses a key or the agent and never asks for a password or about a host key
/// (it fails instead, and the tab says why); none of the tab's port forwards, which the tab already holds, nor its
/// agent or X11 forwarding; no
/// password first, as only the tab's own login is typed; then [`LOOP`] after the destination. ssh takes an option's
/// first value, so these come before the tab's own
pub fn separate_args(args: &[String]) -> Vec<String> {
    let mut out: Vec<String> = ["-T", "-o", "BatchMode=yes", "-o", "ClearAllForwardings=yes"].map(String::from).to_vec();
    let mut rest = args.iter().peekable();
    while let Some(arg) = rest.next() {
        // the profile's agent and X11 forwarding are the tab's: the readings need neither
        if arg == "-tt" || arg == "-t" || arg == "-A" || arg == "-X" {
            continue;
        }
        if arg == "-o" && rest.peek().is_some_and(|o| o.to_ascii_lowercase().starts_with("preferredauthentications=")) {
            rest.next();
            continue;
        }
        out.push(arg.clone());
    }
    out.push(LOOP.into());
    out
}

/// The ssh that takes one reading over the master at `path`. It must never log in on its own: with no master there
/// (the tab still logging in, or gone), `-F none` keeps ~/.ssh/config out, `ProxyCommand=false` makes the direct
/// connection ssh would fall back to fail at once, and BatchMode stops it asking for anything. The destination isn't
/// used: the master already has its own
fn reading_command(path: &Path) -> Command {
    let mut command = Command::new(crate::launch::openssh("ssh"));
    command
        .args(["-F", "none", "-T", "-o", "ControlMaster=no", "-o", "BatchMode=yes", "-o", "ProxyCommand=false"])
        .arg("-o")
        .arg(format!("ControlPath={}", path.display()))
        .args(["sxt-server-load", READ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command
}

/// One reading's output, or None when it failed or took too long
fn read_once(path: &Path, stop: &AtomicBool) -> Option<String> {
    let mut child = reading_command(path).spawn().ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut output = String::new();
                child.stdout.take()?.read_to_string(&mut output).ok()?;
                return (status.success() || output.contains("@@")).then_some(output);
            }
            Ok(None) if started.elapsed() < READ_TIMEOUT && !stop.load(Ordering::Relaxed) => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// Wait for `how_long`, ending early on stop; true if stopped
fn pause(how_long: Duration, stop: &AtomicBool) -> bool {
    let until = Instant::now() + how_long;
    while Instant::now() < until {
        if stop.load(Ordering::Relaxed) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    stop.load(Ordering::Relaxed)
}

/// Take readings the tab's way until stopped, sending each load to `send` (false: nobody's listening)
fn watch(via: LoadVia, stop: Arc<AtomicBool>, send: impl Fn(LoadEvent) -> bool) {
    match via {
        LoadVia::Shared(path) => watch_shared(path, stop, send),
        LoadVia::Separate(args) => watch_separate(args, stop, send),
    }
}

/// Take readings over the master at `path` until stopped, sending each load to `send` (false: nobody's listening)
fn watch_shared(path: PathBuf, stop: Arc<AtomicBool>, send: impl Fn(LoadEvent) -> bool) {
    let mut before: Option<(Counters, Instant)> = None;
    let mut failures = 0;
    while !stop.load(Ordering::Relaxed) {
        // no socket yet: the tab's ssh is still logging in (the master starts once it has)
        if !path.exists() {
            failures = 0;
            if pause(Duration::from_secs(1), &stop) {
                return;
            }
            continue;
        }
        match read_once(&path, &stop) {
            Some(output) => match parse(&output) {
                Some(now) => {
                    failures = 0;
                    let at = Instant::now();
                    let seconds = before.as_ref().map_or(0.0, |(_, then)| at.duration_since(*then).as_secs_f64());
                    if !send(LoadEvent::Load(load(before.as_ref().map(|(c, _)| c), &now, seconds))) {
                        return;
                    }
                    before = Some((now, at));
                }
                None => {
                    send(LoadEvent::Unavailable { why: "not a Linux server (no /proc/stat)".into() });
                    return;
                }
            },
            None if stop.load(Ordering::Relaxed) => return,
            None => {
                failures += 1;
                if failures >= MAX_FAILURES {
                    send(LoadEvent::Unavailable { why: "the server didn't answer the readings".into() });
                    return;
                }
            }
        }
        if pause(EVERY, &stop) {
            return;
        }
    }
}

/// Each reading [`LOOP`] prints, as it's printed: the lines between a `@@` line and an `@@end` one (a login script's
/// greeting before the first left out), sent to `send` until it's false or the output ends
fn readings(output: impl BufRead, send: impl Fn(String) -> bool) {
    let mut reading: Option<String> = None;
    for line in output.lines() {
        let Ok(line) = line else { return };
        match line.trim_end_matches('\r') {
            "@@" => reading = Some(String::new()),
            "@@end" => {
                if let Some(text) = reading.take()
                    && !send(text)
                {
                    return;
                }
            }
            text => {
                if let Some(reading) = reading.as_mut() {
                    reading.push_str(text);
                    reading.push('\n');
                }
            }
        }
    }
}

/// How long the second login may take to give its first reading, and the longest gap after that before it's given
/// up: past ConnectTimeout, a login and a slow reading
const SEPARATE_TIMEOUT: Duration = Duration::from_secs(30);

/// Why there are no readings when the second login ended without one: on Windows that's nearly always a login that
/// wanted a password (BatchMode won't type one), or a host key not yet in known_hosts
const NO_KEY_LOGIN: &str = "on Windows the readings need a login of their own with a key (a key file or ssh-agent), and the server didn't let it in";

/// What following one second login came to
#[derive(Debug, PartialEq)]
enum Followed {
    /// Stopped, the server isn't Linux, or nobody's listening: nothing more to do
    Done,
    /// Its output ended (or went quiet too long) after this many readings: 0 is a login that never got in
    Ended(u32),
}

/// Follow a second login's readings, as [`readings`] gives them on `chunks`, sending each load; `before` is kept
/// across logins, so a rate after a reconnect is still the change since the last reading
fn follow(chunks: &std::sync::mpsc::Receiver<String>, stop: &AtomicBool, before: &mut Option<(Counters, Instant)>, send: &impl Fn(LoadEvent) -> bool) -> Followed {
    let mut count = 0;
    let mut last = Instant::now();
    loop {
        if stop.load(Ordering::Relaxed) {
            return Followed::Done;
        }
        match chunks.recv_timeout(Duration::from_millis(100)) {
            Ok(text) => {
                let Some(now) = parse(&text) else {
                    send(LoadEvent::Unavailable { why: "not a Linux server (no /proc/stat)".into() });
                    return Followed::Done;
                };
                count += 1;
                last = Instant::now();
                let seconds = before.as_ref().map_or(0.0, |(_, then)| last.duration_since(*then).as_secs_f64());
                if !send(LoadEvent::Load(load(before.as_ref().map(|(c, _)| c), &now, seconds))) {
                    return Followed::Done;
                }
                *before = Some((now, last));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) if last.elapsed() < SEPARATE_TIMEOUT => {}
            Err(_) => return Followed::Ended(count),
        }
    }
}

/// The second login's ssh (Windows), with no console window of its own
fn separate_command(args: &[String]) -> Command {
    let mut command = Command::new(crate::launch::openssh("ssh"));
    command.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}

/// Take readings over a second login (Windows) until stopped. A login that never gets in isn't tried again (each try
/// is a failed login in the server's log, which fail2ban counts); one that drops after giving readings is, a few times
fn watch_separate(args: Vec<String>, stop: Arc<AtomicBool>, send: impl Fn(LoadEvent) -> bool) {
    let mut before = None;
    let mut failures = 0;
    while !stop.load(Ordering::Relaxed) {
        let Ok(mut child) = separate_command(&args).spawn() else {
            send(LoadEvent::Unavailable { why: "ssh couldn't be started for the readings".into() });
            return;
        };
        let (tx, chunks) = std::sync::mpsc::channel();
        if let Some(output) = child.stdout.take() {
            let _ = std::thread::Builder::new()
                .name("server-load-output".into())
                .spawn(move || readings(BufReader::new(output), |text| tx.send(text).is_ok()));
        }
        let followed = follow(&chunks, &stop, &mut before, &send);
        let _ = child.kill();
        let _ = child.wait();
        match followed {
            Followed::Done => return,
            Followed::Ended(0) if failures == 0 => {
                send(LoadEvent::Unavailable { why: NO_KEY_LOGIN.into() });
                return;
            }
            Followed::Ended(0) => failures += 1,
            Followed::Ended(_) => failures = 1,
        }
        if failures >= MAX_FAILURES {
            send(LoadEvent::Unavailable { why: "the server didn't answer the readings".into() });
            return;
        }
        if pause(EVERY, &stop) {
            return;
        }
    }
}

/// The watch running: one at most, for the tab in front (its pseudo terminal's id, and its Stop)
#[derive(Default)]
pub struct ServerLoads {
    watching: Mutex<Option<(u32, Arc<AtomicBool>)>>,
}

impl ServerLoads {
    /// Stop whatever is being watched
    fn stop(&self) {
        if let Some((_, stop)) = self.watching.lock().unwrap().take() {
            stop.store(true, Ordering::Relaxed);
        }
    }
}

/// Watch the server load of the tab with pseudo terminal `id`, sending it on `on_event`; any other tab's watch stops
/// (on Windows, ending its second login). False when the tab has no way to read it (its profile hasn't Show server
/// load on, or it isn't SSH), so the status bar shows nothing for it. A socket is the tab's (pty.rs), gone when the
/// tab closes
#[tauri::command]
pub fn server_load_watch(loads: tauri::State<'_, ServerLoads>, ptys: tauri::State<'_, crate::pty::Ptys>, id: u32, on_event: tauri::ipc::Channel<LoadEvent>) -> bool {
    loads.stop();
    let Some(via) = ptys.server_load(id) else { return false };
    let stop = Arc::new(AtomicBool::new(false));
    *loads.watching.lock().unwrap() = Some((id, stop.clone()));
    let _ = std::thread::Builder::new()
        .name(format!("server-load-{id}"))
        .spawn(move || watch(via, stop, move |event| on_event.send(event).is_ok()));
    true
}

/// Stop watching (the tab in front changed to one without readings, or its session ended)
#[tauri::command]
pub fn server_load_stop(loads: tauri::State<'_, ServerLoads>) {
    loads.stop();
}

#[cfg(test)]
mod tests {
    //! The counters are read from a real reading of a Linux server (an OpenMediaVault host running Docker and
    //! Kubernetes, in tests/fixtures, shortened: fewer CPUs, memory lines and interfaces, and its long intr line cut,
    //! none of which the numbers use), and a second reading made from it with known changes.

    use super::*;

    /// The server's reading, as the tab gets it
    const READING: &str = include_str!("../tests/fixtures/openmediavault-load.txt");

    #[test]
    fn reads_memory_and_disk_from_a_real_servers_reading() {
        let counters = parse(READING).expect("a Linux server's reading");
        // (32768384 − 27530008) ÷ 32768384
        assert_eq!(counters.mem.map(|m| m.round()), Some(16.0));
        assert_eq!(counters.disk, Some(50));
        // user nice system idle iowait irq softirq steal; busy leaves out idle and iowait
        let total = 154695184 + 103517561 + 61314333 + 6986646950u64 + 12450978 + 3413284;
        assert_eq!(counters.cpu, Some((total - 6986646950 - 12450978, total)));
    }

    #[test]
    fn counts_the_real_interface_not_dockers_and_kubernetes_virtual_ones() {
        let counters = parse(READING).unwrap();
        // enp5s0 alone: lo, br-*, docker0, veth*, flannel.1 and cni0 left out
        assert_eq!(counters.net, Some((835029956856, 820693645263)));
        for name in ["lo", "veth0e381be2", "br-8a76cc15fa4f", "docker0", "cni0", "flannel.1", "virbr0", "tun0", "wg0"] {
            assert!(!counted_interface(name), "{name}");
        }
        for name in ["enp5s0", "eth0", "ens18", "wlan0", "bond0"] {
            assert!(counted_interface(name), "{name}");
        }
    }

    #[test]
    fn works_out_cpu_and_traffic_from_the_change_between_two_readings() {
        let first = parse(READING).unwrap();
        // 3 s later: 300 more ticks of CPU, 60 of them busy; 30000 bytes in and 9000 out on enp5s0
        let later = READING
            .replace("cpu  154695184 103517561 61314333 6986646950", "cpu  154695244 103517561 61314333 6986647190")
            .replace("enp5s0: 835029956856", "enp5s0: 835029986856")
            .replace("26668717 820693645263", "26668717 820693654263");
        let second = parse(&later).unwrap();
        let load = load(Some(&first), &second, 3.0);
        assert_eq!(load.cpu.map(|c| c.round()), Some(20.0));
        assert_eq!(load.down, Some(10000.0));
        assert_eq!(load.up, Some(3000.0));
        assert_eq!(load.mem.map(|m| m.round()), Some(16.0));
        assert_eq!(load.disk, Some(50));
    }

    #[test]
    fn the_first_reading_has_no_rates_and_a_counter_going_backwards_gives_none() {
        let first = parse(READING).unwrap();
        let only = load(None, &first, 0.0);
        assert_eq!((only.cpu, only.down, only.up), (None, None, None));
        assert!(only.mem.is_some());
        // the server restarted: its counters started again from nothing
        let restarted = Counters { cpu: Some((10, 100)), net: Some((5, 5)), ..first.clone() };
        let after = load(Some(&first), &restarted, 3.0);
        assert_eq!((after.cpu, after.down, after.up), (None, None, None));
    }

    #[test]
    fn a_server_without_proc_stat_gives_no_reading() {
        // a BSD or macOS host: cat fails, df still answers
        assert_eq!(parse("@@\ncat: /proc/stat: No such file or directory\n/dev/disk1s1 1000 500 500 50% /\n"), None);
        // a Cisco switch, had it been asked
        assert_eq!(parse("@@\n% Invalid input detected at '^' marker.\n"), None);
    }

    #[test]
    fn a_network_root_filesystem_still_gives_its_capacity() {
        let reading = READING.replace("/dev/sdc2        228140460", "nas:/export/root 228140460");
        assert_eq!(parse(&reading).unwrap().disk, Some(50));
    }

    #[test]
    fn only_what_follows_the_last_marker_is_read() {
        // the remote shell's own greeting (a motd from a login script) before the marker isn't read as counters
        let reading = format!("cpu 1 1 1 1\nMemTotal: 1 kB\n{READING}");
        assert_eq!(parse(&reading).unwrap().mem.map(|m| m.round()), Some(16.0));
    }

    #[test]
    fn the_master_lives_and_ends_with_the_tabs_session() {
        let path = PathBuf::from("/tmp/sxt-1-0.sock");
        assert_eq!(master_args(&path), ["-o", "ControlMaster=yes", "-o", "ControlPath=/tmp/sxt-1-0.sock", "-o", "ControlPersist=no"]);
        // a socket's path must stay short (a Mac's limit is about 100 characters)
        assert!(new_socket_path().as_os_str().len() < 100);
        assert_ne!(new_socket_path(), new_socket_path());
    }

    /// What LOOP prints for one reading: the fixture between its markers
    fn printed(reading: &str) -> String {
        format!("@@\n{}\n@@end\n", reading.rsplit("@@").next().unwrap().trim())
    }

    #[test]
    fn windows_second_login_uses_the_tabs_arguments_never_asks_and_runs_the_loop() {
        // the tab's ssh, as launch.rs builds it: a terminal, the password tried first, a port forward, the host
        let tab: Vec<String> = ["-tt", "-p", "2222", "-l", "admin", "-o", "PreferredAuthentications=keyboard-interactive,password,publickey", "-A", "-X", "-L", "8080:localhost:80", "10.0.0.1"]
            .map(String::from)
            .to_vec();
        let second = separate_args(&tab);
        // options first, as ssh takes the first value it's given: no terminal, never a question, no forwards
        assert_eq!(second[..5], ["-T", "-o", "BatchMode=yes", "-o", "ClearAllForwardings=yes"]);
        assert!(!second.contains(&"-tt".to_string()), "{second:?}");
        // nor the profile's agent or X11 forwarding
        assert!(!second.iter().any(|a| a == "-A" || a == "-X"), "{second:?}");
        assert!(!second.iter().any(|a| a.starts_with("PreferredAuthentications")), "{second:?}");
        // the same server and user, then the loop
        assert!(second.windows(2).any(|w| w == ["-p", "2222"]) && second.windows(2).any(|w| w == ["-l", "admin"]), "{second:?}");
        assert_eq!(second[second.len() - 2..], ["10.0.0.1", LOOP]);
    }

    #[test]
    fn the_loop_only_reads_and_marks_each_reading() {
        assert!(LOOP.contains(READ.trim_start_matches("echo @@; ")), "{LOOP}");
        assert!(LOOP.contains("echo @@end") && LOOP.contains("sleep 3"), "{LOOP}");
        // in sh whatever the login shell, and no # (a comment in sh) in a marker
        assert!(LOOP.starts_with("exec sh -c '") && !LOOP.contains('#'), "{LOOP}");
    }

    #[test]
    fn each_reading_the_loop_prints_is_read_as_it_comes_a_greeting_before_it_left_out() {
        let output = format!("Welcome to Ubuntu\r\ncpu 9 9 9 9\r\n{}{}", printed(READING), printed(READING).replace('\n', "\r\n"));
        let got = Mutex::new(Vec::new());
        readings(std::io::Cursor::new(output), |text| {
            got.lock().unwrap().push(text);
            true
        });
        let got = got.into_inner().unwrap();
        assert_eq!(got.len(), 2);
        for text in &got {
            assert!(!text.contains("Welcome") && !text.contains('\r'), "{text}");
            assert_eq!(parse(text).and_then(|c| c.disk), Some(50));
        }
    }

    /// Follow `chunks` (sent, then the sender gone, as when ssh ends) and give what it came to and what it sent
    fn followed(chunks: &[String]) -> (Followed, Vec<LoadEvent>) {
        let (tx, rx) = std::sync::mpsc::channel();
        for chunk in chunks {
            tx.send(chunk.clone()).unwrap();
        }
        drop(tx);
        let sent = Mutex::new(Vec::new());
        let came = follow(&rx, &AtomicBool::new(false), &mut None, &|event| {
            sent.lock().unwrap().push(event);
            true
        });
        (came, sent.into_inner().unwrap())
    }

    #[test]
    fn windows_readings_come_to_the_status_bar_and_a_login_that_never_got_in_is_told() {
        let reading = READING.rsplit("@@").next().unwrap().to_string();
        let (came, sent) = followed(&[reading.clone(), reading]);
        assert_eq!(came, Followed::Ended(2));
        assert!(matches!(&sent[..], [LoadEvent::Load(first), LoadEvent::Load(second)] if first.cpu.is_none() && second.mem.is_some()), "{sent:?}");
        // ssh ended without a reading: BatchMode turned down a password login; watch_separate says so once
        let (came, sent) = followed(&[]);
        assert_eq!((came, sent.len()), (Followed::Ended(0), 0));
        assert!(NO_KEY_LOGIN.contains("key"));
        // not Linux: said, and no more
        let (came, sent) = followed(&["cat: /proc/stat: No such file or directory\n".into()]);
        assert_eq!(came, Followed::Done);
        assert!(matches!(&sent[..], [LoadEvent::Unavailable { why }] if why.contains("not a Linux server")), "{sent:?}");
    }

    #[test]
    fn a_second_login_that_cant_get_in_is_told_at_once_with_the_real_ssh() {
        // the system's own ssh (Windows' OpenSSH, the Mac's), to a port nothing listens on: it ends without a reading,
        // and the tab is told why once, rather than the watch trying again or hanging
        let args = separate_args(&["-p".into(), "1".into(), "-o".into(), "ConnectTimeout=5".into(), "127.0.0.1".into()]);
        let sent = Mutex::new(Vec::new());
        let started = Instant::now();
        watch_separate(args, Arc::new(AtomicBool::new(false)), |event| {
            sent.lock().unwrap().push(event);
            true
        });
        let sent = sent.into_inner().unwrap();
        assert!(matches!(&sent[..], [LoadEvent::Unavailable { why }] if why == NO_KEY_LOGIN), "{sent:?}");
        assert!(started.elapsed() < Duration::from_secs(15), "{:?}", started.elapsed());
    }

    #[test]
    fn a_reading_can_never_log_in_on_its_own() {
        let command = reading_command(Path::new("/tmp/sxt-1-0.sock"));
        let args: Vec<String> = command.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        for needed in ["-F", "none", "ControlMaster=no", "BatchMode=yes", "ProxyCommand=false", "ControlPath=/tmp/sxt-1-0.sock"] {
            assert!(args.iter().any(|a| a == needed), "{needed} in {args:?}");
        }
        assert_eq!(args.last().map(String::as_str), Some(READ));
    }
}
