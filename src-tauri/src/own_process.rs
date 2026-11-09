//! Running a helper program, such as 1Password's CLI, so that the Mac judges it as itself, not as SuperXTerminal.
//!
//! macOS (15 and later) guards each app's own data, such as 1Password's group container
//! (`~/Library/Group Containers/2BUA8C4S2C.com.1password`). It decides who is asking by the *responsible* process:
//! the app a program was started from, not the program itself. Run from SuperXTerminal, `op` counts as SuperXTerminal,
//! isn't let into 1Password's container, can't read whether "Integrate with 1Password CLI" is on, and says "account
//! is not signed in". Run from Terminal (as under `npm run tauri dev`), it counts as Terminal, which has been allowed,
//! so it works there and not in the installed app.
//!
//! Started here with its responsibility disclaimed, as Terminal, iTerm and VS Code start what they run, a program is
//! judged by its own signature: `op` is signed by 1Password's own team, which owns that container, so it's let in
//! without asking. That takes `posix_spawn` with Apple's `responsibility_spawnattrs_setdisclaim`, which
//! `std::process::Command` can't set; it's looked up when it's needed, and without it the program starts as before.
//!
//! On Windows and Linux there's nothing of the kind, and [`output`] is `Command::output`.

use std::process::{Command, Output};

/// Run `command` to its end, reading nothing (stdin is `/dev/null`) and returning what it wrote, as
/// `Command::output` does; on the Mac, as its own responsible process (see the module's comment). Only the program,
/// its arguments and its environment are taken from `command`.
#[cfg(target_os = "macos")]
pub fn output(command: Command) -> std::io::Result<Output> {
    mac::output(&command)
}

/// Run `command` to its end, reading nothing and returning what it wrote: `Command::output`, as there's no
/// responsible process to disclaim off the Mac
#[cfg(not(target_os = "macos"))]
pub fn output(mut command: Command) -> std::io::Result<Output> {
    command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    command.output()
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::{CString, OsStr, OsString};
    use std::io::{Error, Read, Result};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Command, Output};

    /// `responsibility_spawnattrs_setdisclaim(attrs, disclaim)`: in libSystem since macOS 10.14, but in no header,
    /// so it's found by name, and missing, isn't called
    type SetDisclaim = unsafe extern "C" fn(*mut libc::posix_spawnattr_t, libc::c_int) -> libc::c_int;

    /// The disclaim call, if this Mac has it
    fn set_disclaim() -> Option<SetDisclaim> {
        let name = c"responsibility_spawnattrs_setdisclaim";
        // SAFETY: dlsym with RTLD_DEFAULT and a NUL-terminated name only looks the symbol up
        let found = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
        // SAFETY: the symbol, when there, is the function of that signature
        (!found.is_null()).then(|| unsafe { std::mem::transmute::<*mut libc::c_void, SetDisclaim>(found) })
    }

    /// A C string of `s`, or the error for one holding a NUL
    fn c_string(s: &OsStr) -> Result<CString> {
        CString::new(s.as_bytes()).map_err(|e| Error::new(std::io::ErrorKind::InvalidInput, e))
    }

    /// The environment `command` would give its program: this process's, with the command's own changes
    fn environment(command: &Command) -> Result<Vec<CString>> {
        let mut vars: Vec<(OsString, OsString)> = std::env::vars_os().collect();
        for (key, value) in command.get_envs() {
            vars.retain(|(k, _)| k != key);
            if let Some(value) = value {
                vars.push((key.to_os_string(), value.to_os_string()));
            }
        }
        vars.into_iter()
            .map(|(k, v)| {
                let mut pair = k.into_vec();
                pair.push(b'=');
                pair.extend(v.into_vec());
                CString::new(pair).map_err(|e| Error::new(std::io::ErrorKind::InvalidInput, e))
            })
            .collect()
    }

    /// A pipe: (read end, write end)
    fn pipe() -> Result<(libc::c_int, libc::c_int)> {
        let mut ends = [0; 2];
        // SAFETY: ends has room for the two descriptors
        if unsafe { libc::pipe(ends.as_mut_ptr()) } != 0 {
            return Err(Error::last_os_error());
        }
        Ok((ends[0], ends[1]))
    }

    /// Everything written to the read end `fd`, closing it
    fn read_all(fd: libc::c_int) -> Vec<u8> {
        use std::os::fd::FromRawFd;
        // SAFETY: fd is a pipe's read end that nothing else owns
        let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
        let mut bytes = Vec::new();
        let _ = file.read_to_end(&mut bytes);
        bytes
    }

    /// [`super::output`] on the Mac: `posix_spawn` with the responsibility disclaimed, stdin from `/dev/null`, and
    /// stdout and stderr each a pipe. POSIX_SPAWN_CLOEXEC_DEFAULT keeps every other descriptor of this app (the
    /// tabs' terminals, its sockets) out of the program
    pub fn output(command: &Command) -> Result<Output> {
        let program = c_string(command.get_program())?;
        let mut args = vec![program.clone()];
        for arg in command.get_args() {
            args.push(c_string(arg)?);
        }
        let env = environment(command)?;
        let mut argv: Vec<*mut libc::c_char> = args.iter().map(|a| a.as_ptr() as *mut _).collect();
        argv.push(std::ptr::null_mut());
        let mut envp: Vec<*mut libc::c_char> = env.iter().map(|e| e.as_ptr() as *mut _).collect();
        envp.push(std::ptr::null_mut());

        let (out_read, out_write) = pipe()?;
        let (err_read, err_write) = pipe()?;
        let mut pid: libc::pid_t = 0;
        // SAFETY: the attributes and file actions are initialised before use and destroyed after; every pointer
        // passed lives until posix_spawn returns
        let spawned = unsafe {
            let mut attrs: libc::posix_spawnattr_t = std::mem::zeroed();
            let mut actions: libc::posix_spawn_file_actions_t = std::mem::zeroed();
            libc::posix_spawnattr_init(&mut attrs);
            libc::posix_spawn_file_actions_init(&mut actions);
            libc::posix_spawnattr_setflags(&mut attrs, libc::POSIX_SPAWN_CLOEXEC_DEFAULT as libc::c_short);
            if let Some(disclaim) = set_disclaim() {
                disclaim(&mut attrs, 1);
            }
            libc::posix_spawn_file_actions_addopen(&mut actions, 0, c"/dev/null".as_ptr(), libc::O_RDONLY, 0);
            libc::posix_spawn_file_actions_adddup2(&mut actions, out_write, 1);
            libc::posix_spawn_file_actions_adddup2(&mut actions, err_write, 2);
            let result = libc::posix_spawn(&mut pid, program.as_ptr(), &actions, &attrs, argv.as_ptr(), envp.as_ptr());
            libc::posix_spawn_file_actions_destroy(&mut actions);
            libc::posix_spawnattr_destroy(&mut attrs);
            libc::close(out_write);
            libc::close(err_write);
            result
        };
        if spawned != 0 {
            // SAFETY: the read ends are ours, and nothing will read them
            unsafe {
                libc::close(out_read);
                libc::close(err_read);
            }
            return Err(Error::from_raw_os_error(spawned));
        }
        // stderr read beside stdout, so neither pipe fills and stalls the program
        let stderr = std::thread::spawn(move || read_all(err_read));
        let stdout = read_all(out_read);
        let stderr = stderr.join().unwrap_or_default();
        let mut status = 0;
        loop {
            // SAFETY: pid is the child just started
            if unsafe { libc::waitpid(pid, &mut status, 0) } >= 0 {
                break;
            }
            let error = Error::last_os_error();
            if error.kind() != std::io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
        Ok(Output { status: std::process::ExitStatus::from_raw(status), stdout, stderr })
    }
}

#[cfg(test)]
mod tests {
    //! What [`output`] gives back and, on the Mac, whether its program is its own responsible process. Run against
    //! real programs: `/bin/sh` on the Mac and Linux, `cmd.exe` on Windows, and a copy of these tests.

    use super::*;

    /// A command printing "out" to stdout and "err" to stderr, then ending with 3
    fn speaking() -> Command {
        if cfg!(windows) {
            let mut command = Command::new("cmd.exe");
            command.args(["/C", "echo out& echo err 1>&2& exit 3"]);
            command
        } else {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", "echo out; echo err >&2; exit 3"]);
            command
        }
    }

    #[test]
    fn what_the_program_writes_and_how_it_ended_come_back() {
        let output = output(speaking()).unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "out");
        assert_eq!(String::from_utf8_lossy(&output.stderr).trim(), "err");
        assert_eq!(output.status.code(), Some(3));
    }

    #[test]
    fn a_program_that_isnt_there_is_an_error() {
        assert!(output(Command::new("/no/such/program")).is_err());
    }

    /// The Mac only, as responsibility is the Mac's alone. Asking about another process takes root, so the program run
    /// is a copy of these tests, running [`answer_responsibility`], which asks libSystem whether it's responsible for
    /// itself and prints the answer. Without the disclaim it says false: it's this test's responsibility, as `op` was
    /// SuperXTerminal's
    #[cfg(target_os = "macos")]
    #[test]
    fn on_the_mac_the_program_is_its_own_responsible_process() {
        // this test runs `output` on a copy of itself, which answers through its exit code (see `answer_responsibility`)
        let me = std::env::current_exe().unwrap();
        let mut command = Command::new(&me);
        command.args(["--exact", "own_process::tests::answer_responsibility", "--nocapture", "--include-ignored"]);
        command.env("SXT_ANSWER_RESPONSIBILITY", "1");
        let output = output(command).unwrap();
        let said = String::from_utf8_lossy(&output.stdout);
        assert!(said.contains("responsible for itself: true"), "the program wasn't its own responsible process: {said}");
    }

    /// Not a test of its own: run by [`on_the_mac_the_program_is_its_own_responsible_process`] in a copy of the
    /// tests, to say whether that copy is its own responsible process
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn answer_responsibility() {
        if std::env::var_os("SXT_ANSWER_RESPONSIBILITY").is_none() {
            return;
        }
        type Responsible = unsafe extern "C" fn(libc::pid_t) -> libc::pid_t;
        let name = c"responsibility_get_pid_responsible_for_pid";
        // SAFETY: a lookup by name; the symbol, when there, is the function of that signature
        let found = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
        assert!(!found.is_null(), "this Mac has no responsibility_get_pid_responsible_for_pid");
        let responsible = unsafe { std::mem::transmute::<*mut libc::c_void, Responsible>(found) };
        let me = std::process::id() as libc::pid_t;
        println!("responsible for itself: {}", unsafe { responsible(me) } == me);
    }
}
