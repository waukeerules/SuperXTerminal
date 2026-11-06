//! Server load in the status bar: CPU, memory, the root filesystem and network traffic of the Linux
//! server an SSH tab is connected to, every few seconds, for the tab you're looking at.
//!
//! Only for a profile with "Show server load" on (off by default: a Cisco switch has no shell to run it, and would log
//! the attempt as a command), on a Mac. The tab's ssh is then started as an OpenSSH connection-sharing master
//! (`ControlMaster`, `launch.rs`), and each reading runs over that same connection as a new channel: no second login.
//! The reading is one short command per channel ([`READ`]), not a loop left running: a master waits for its other
//! channels before it ends, so a loop would keep the tab open after you logged out. Windows' OpenSSH can't share a
//! connection (no ControlMaster), so there's none of this there yet.
//!
//! The command only reads (`cat`, `df`): nothing on the server changes. A server without /proc/stat (BSD, macOS, a
//! network device) gives no CPU line, and the tab's readings stop, saying so.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

/// What each reading runs on the server: its counters, after a marker. Read only
pub const READ: &str = "echo @@; cat /proc/stat /proc/meminfo /proc/net/dev; df -P / | tail -n 1";

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

/// Take readings over the master at `path` until stopped, sending each load to `send` (false: nobody's listening)
fn watch(path: PathBuf, stop: Arc<AtomicBool>, send: impl Fn(LoadEvent) -> bool) {
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

/// Watch the server load of the tab with pseudo terminal `id`, sending it on `on_event`; any other tab's watch stops.
/// False when the tab has no shared connection (its profile hasn't Show server load on, it isn't SSH, or this is
/// Windows), so the status bar shows nothing for it. Its socket is the tab's (pty.rs), gone when the tab closes
#[tauri::command]
pub fn server_load_watch(loads: tauri::State<'_, ServerLoads>, ptys: tauri::State<'_, crate::pty::Ptys>, id: u32, on_event: tauri::ipc::Channel<LoadEvent>) -> bool {
    loads.stop();
    let Some(path) = ptys.server_load_socket(id) else { return false };
    let stop = Arc::new(AtomicBool::new(false));
    *loads.watching.lock().unwrap() = Some((id, stop.clone()));
    let _ = std::thread::Builder::new()
        .name(format!("server-load-{id}"))
        .spawn(move || watch(path, stop, move |event| on_event.send(event).is_ok()));
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
