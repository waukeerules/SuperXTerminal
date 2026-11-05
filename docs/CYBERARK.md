# CyberArk sessions

How SuperXTerminal connects through CyberArk's SSH gateway (Privileged Session Manager for SSH) while the session
itself keeps naming the real device. The app's side is built and covered by tests (`launch.rs`), and it has been
tried with CyberArk on the production network, on Windows (November 2026): see
[what's been checked there, and what hasn't](#to-check-in-production).

## What CyberArk needs

CyberArk isn't a jump host. ssh connects to the gateway (e.g. `example.cyberark.cloud`), and the *username* says who
you are and which target to open, for example:

```text
{email}@{domain}#{cyberark_region}@{cyberark_username}#{cyberark_address}@{host_address}#{cyberark_connector}
```

- `{email}`, `{domain}`, `{cyberark_region}`, `{cyberark_username}`, `{cyberark_address}` and `{cyberark_connector}` are
  each user's own Environment Variables (Tools > Environment Variables), so a shared Database session holds no one's
  login.
- The names are only names: what matters is that the profile's username and the Environment Variables use the same
  ones. The first part is whatever CyberArk signs you in as, e.g. `{ad_username}@{domain}` where the email address is
  the AD username at the domain, or one `{email}` variable holding it all. The fetch command (below) uses the same
  first part.
- `{host_address}` is the target: the session's own Host.
- The login is an SSH key that CyberArk issues for a short time (MFA caching). It's fetched beforehand; on Windows
  today that's a `.bat` (see [Getting the key](#getting-the-key)).

So a CyberArk session has two addresses: the **target** (the session's Host, which stays filled in) and the
**gateway** (where ssh actually connects).

## Why the session's Host stays filled in

With the target in the session's own Host field, the same shared session works both ways:

- **Through CyberArk**: its profile ("CyberArk") supplies the gateway, the key and the special username, and the
  target goes into the username as `{host_address}`.
- **Directly, with your own login** (read-only AD access, say): a local override (Edit Session > Local overrides…)
  sets your own profile and username. The username no longer has `{host_address}` in it, so ssh connects to the
  Host as an ordinary session. Nothing in the shared session changes; only you get the override.
- **Switching between them** keeps both: the session's menu has Use Local Overrides ✓ (and the dialog, Use these
  overrides), so turning the override off opens it through CyberArk again with your values kept for later; Tools >
  Use Local Overrides does every session at once. A preset ("Local AD login": `{ad_username}`, Default Settings) fills
  in the override for one session or a whole selection.

## How it works

### 1. `{host_address}` and `{host_port}` as built-in placeholders

Filled from the session's own Host and port (after local overrides), before the Environment Variables, so they're
never reported as "placeholders that have no value yet". Built-ins win over a variable of the same name, so a stray
Environment Variable called `host_address` can't redirect a session.

Before these, every `{key}` had to be an Environment Variable (`variables.rs`), so this username refused to start.

### 2. The profile's Host

Settings > Profiles > Connection has **Host** (at the profile's Port). It's used in two cases, as this table shows;
otherwise a session's own host wins:

| Session's Host | Username has `{host_address}`? | ssh connects to |
|---|---|---|
| set | no | the session's Host, as always |
| set | yes | the profile's Host (the gateway), the session's Host in the username |
| empty | no | the profile's Host, as a PuTTY saved session's Host Name |
| empty | yes | nothing: it refuses, as there's no target to put in the username |

- **Several profiles, one address**: sessions that leave Host empty and only pick a profile, each profile with its
  own username, all reach the profile's Host. On a computer without that profile the session says it has no host,
  as PuTTY would with a saved session it doesn't have.
- **Through a gateway**: the narrow rule keeps an override able to switch it off with the username alone. A local
  override's own username (no `{host_address}`) connects straight to the session's Host.
- **Ports**: the profile's Port when the profile's Host is used. Through a gateway, the session's port is the
  target's (it travels as `{host_port}`) and the gateway's is the profile's, else 22; with no host of its own, the
  profile's Port, else the session's.
- `{host_address}` in the username with no profile Host: the session doesn't start, and the tab says to set one.
  (Connecting to the target with a CyberArk username would only fail confusingly.)
- The tab says where it's going first: "Connecting to 192.168.0.50 through the gateway example.cyberark.cloud.", or
  "No host of its own: connecting to … (the Host of profile "…").".
- A jump host still applies, to the gateway.
- The app's log records `ssh command: … -l <user> <gateway>`; Verbose SSH (Settings > General > Diagnostics) shows
  the gateway's side of the login.

### 3. The username goes to ssh with `-l`

It already does (`launch.rs`). `user@host` form works too, as OpenSSH splits it at its *last* `@` (the macOS
function below connects that way); `-l` just leaves no doubt about where the username ends.

### 4. The CyberArk profile

| Field | Value |
|---|---|
| Username | the connection string above |
| Host | e.g. `example.cyberark.cloud` (the gateway) |
| Key file | the cached key (`~/.ssh/cyberark_key`, or the `.ppk` on Windows; `.ppk` files are converted for ssh) |
| SSH keys | either: with a key file, only that key is offered (`IdentitiesOnly`, as today), so no other key is spent on the gateway |
| No username | use ssh's own (the profile always gives one) |

The session's own Username stays blank (or the profile's is overridden per user), so the profile's connection
string applies.

## Getting the key

CyberArk issues the SSH key for a short time (MFA caching), from a special `key` target on its gateway. Fetching it
is itself an SSH login with a sign-in step: CyberArk prints a link, the link opens Okta in the browser, Okta shows a
PIN after sign-in, and the PIN is pasted back into the terminal, where the fetch's ssh is waiting at a prompt. Then
the key is written to a file, and sessions log in with it until it expires.

Today on Windows a `.bat` does the fetch with `psftp` and converts the key to `.ppk` with WinSCP for PuTTY.

### By hand, today

**macOS**: a shell function (in `~/.zshrc`, say), the Mac's version of the `.bat`. It fetches the key when there's
none or it's over 4 hours old (14400 seconds, the key's lifetime here), then connects to the target given as its
argument, e.g. `pam 192.168.0.50`:

```bash
function pam() {
    email="my_email"
    domain="example.com"
    cyberark_region="example"
    cyberark_username="PAM_EXAMPLE_001"
    cyberark_address="example.com"
    cyberark_connector="ConnectorPool"
    ssh_host="example.ssh.cyberark.cloud"
    key=~/.ssh/key

    if [ -f "$key" ]; then
        age=$(( $(date +%s) - $(stat -f %m "$key") ))
    fi
    if [ ! -f "$key" ] || [ "$age" -gt 14400 ]; then
        sftp "$email@$domain#$cyberark_region@key@$ssh_host:/key" ~/.ssh
    fi
    ssh -i "$key" "$email@$domain#$cyberark_region@$cyberark_username#$cyberark_address@$1#$cyberark_connector@$ssh_host"
}
```

- The fetch is `sftp` run interactively, downloading `/key` from the gateway's `key` target (`host:/key` form) into
  `~/.ssh`. The sign-in (the link to Okta, then the PIN pasted back) happens at its prompts, in the same terminal.
- The key's age is its file's modified time against now (`stat -f %m`, macOS's `stat`).
- The connection is `user@host` form: OpenSSH splits at the last `@`, so the gateway is `$ssh_host` and everything
  before it, `@` and `#` included, is the username. The target (`$1`) sits where SuperXTerminal puts `{host_address}`.
- `~/.ssh/key` is a generic name; a name of its own (`~/.ssh/cyberark_key`) keeps it apart from other keys.

**Windows**: the `.bat` fetches with `psftp` and converts the key to `.ppk` with WinSCP, for PuTTY. SuperXTerminal
uses Windows' own OpenSSH, so the macOS function's `sftp` and `ssh` lines work in PowerShell as they are, with the
key kept in OpenSSH's format; no conversion. (A `.ppk` works anyway: SuperXTerminal converts any profile's `.ppk`.)

### Built in: a profile's key fetch

Not CyberArk's alone: anything that issues SSH keys or certificates for a short time from a command fits, such as
HashiCorp Vault (`vault ssh`, signed certificates), Teleport (`tsh login`), Smallstep (`step ssh login`), Okta
Advanced Server Access (`sft`) or Azure (`az ssh cert`). A certificate written beside the key as `<key>-cert.pub` is
picked up by OpenSSH by itself.

Settings > Profiles > Login, under Key file:

| Field | What it's for |
|---|---|
| Fetch command | The command that gets a new key into the Key file, with `{key_file}` for that path and the usual `{placeholders}` (each user's `{email}` and so on). Profiles are per computer, so each machine has its own. |
| Lifetime | How long a key is good for, in minutes (240 here: CyberArk's 4 hours); 0 fetches only when there's none. |

**Examples…** beside the Fetch command fills it from a list for the system it's on (`keyFetchExamples.ts`): CyberArk
(`sftp "{ad_username}@{cyberark_region}@key@{cyberark_gateway}:/key" "{key_file}"`, 4 hours), a shell or
PowerShell function of your own, Teleport (`tsh login … --out`), Smallstep (`step ssh certificate`), and a test that
makes a throwaway key every minute. Picking one also sets the Lifetime, and the Key file when there's none, and says
what to set (its Environment Variables), for editing from there.

Below them the editor says how the next session will find the key: "Key: 37 min old, good for 4 h.", "The key is
5 h old, past its 4 h: the next session fetches a new one.", or "No key yet".

When a session using the profile opens (`launch.rs` and `PageHost.ts`):

1. **The key is checked first.** No key file, or one older than the lifetime (by the file's modified time), and the
   key is fetched before connecting. The tab says why: "The key is 4 h 12 min old (good for 4 h): fetching a new
   one." That's the "why can't I connect" answer, before ssh even starts.
2. **The fetch runs in the session's own tab** through the user's shell, its `~/.zshrc` read (`-i`) so a shell
   function works, or PowerShell on Windows. CyberArk's link and its PIN prompt show there; the link opens with
   Ctrl+click (or copy it), Okta signs in and shows the PIN, and the PIN is pasted into the tab. The app types
   nothing itself here: no stored password goes into a fetch.
3. **When the fetch ends well** (exit code 0), the tab goes on to connect, the key made the user's own (mode 600 on
   the Mac, the user's own access on Windows), as ssh refuses a key others can read. If the fetch fails, or ends well
   but the key is still missing or old, the tab says so (naming the profile's Fetch command and Key file) rather
   than fetch again and again; Enter tries again.
4. **One fetch at a time.** Tabs in a window opening together with one profile (Connect all, a layout) share it:
   the first fetches, the others say they're waiting, then connect.
5. **Expired while open.** A key only matters at login, so open sessions carry on. A reconnect after the lifetime
   fetches again first.

**Log in & cache key**, under the fetch's fields (always shown; greyed with the reason until there's a saved Fetch
command and Key file), runs the fetch now in a tab of its own ("Key fetch: CyberArk"),
Settings closing so its sign-in shows: sign in at the start of the day, and sessions then log in with the key. The
tab stays when it ends, saying the key is cached or why not; Enter runs it again. It's greyed while the profile has
unsaved changes, as it runs the saved command, and layouts leave such tabs out, so opening one never starts a
sign-in.

For a command like the macOS `pam` function above, the Fetch command is its `sftp` line (or a function of its own
that runs it), and the Key file the path it writes, e.g. `~/.ssh/cyberark_key`.

Not built yet:

- A hint after ssh's "Permission denied (publickey)" when the gateway refuses the key anyway (revoked early, or the
  lifetime set longer than the issuer's), with the key's age.
- **Forget key** in the profile editor.
- Tabs in different windows share nothing: each window fetches on its own.

The key is a credential, so it's kept only where the profile's Key file says, in the user's own folders (never the
settings folder, the Database table or anything shared), and the fetch command and lifetime are profile fields,
which are per user.

## To check in production

Checked on Windows, November 2026, with the CyberArk profile's Fetch command:

- ✅ A session with no key file fetches one first, in its own tab: the fetch reached CyberArk, the key file was made
  and saved, and the session then went on to connect through the gateway to the device.
- ✅ The connection string's parts and order for this tenant, and `-l` with `#` and `@` reaching the gateway
  unchanged: the session logged in to the device and it could be managed over SSH (its CLI).
- ✅ The key, once fetched, works outside the app too: a connection from another shell, with the same key, logged in
  to the device as well.
- ✅ An expired key: the session asks to reconnect, and reconnecting fetches and saves a new key, then connects.
- ✅ The fetch's sign-in in the tab: CyberArk's link opens with Ctrl+click.

Still to check:

- The PIN prompt: the PIN doesn't show as it's typed, where PuTTY showed it. The tab doesn't hide or show typing
  itself; it shows what the fetch's program echoes. The prompt is CyberArk's (a keyboard-interactive question to
  OpenSSH's `sftp`), which says whether the answer is echoed; PuTTY honours that, and Windows' OpenSSH seems to hide
  every answer whatever it says. Not something the app controls; pasting the PIN works either way.
- The Mac: all of the above. The link there opens with Ctrl+click too, though on the Mac Ctrl+click is also a
  right-click (or else by copying it).
- The gateway's host key: one key for every target, so known_hosts gets a single entry for the gateway.
- MFA or other keyboard-interactive prompts at the gateway, and whether the app's automatic password typing must stay
  out of them.
- File transfer and SCP through the gateway: probably not offered by PSM for SSH; the file transfer tab should say so
  rather than fail.

## Done, and still to do

- ✅ `{host_address}` / `{host_port}` built-ins (`launch.rs`, filled before `variables::missing`). Tests: a username
  holding them starts, and an Environment Variable of the same name doesn't win.
- ✅ Profile `host` (`profiles.rs`, `profiles.ts`, the editor with its change dot; read from `gatewayHost` too, its
  first name) and the table above. Tests: the gateway is used only with `{host_address}` in the username, at the
  profile's port; an override's own username connects to the Host; `{host_address}` without a profile Host refuses;
  a session with no host uses its profile's, at the profile's port; no host anywhere, or `{host_address}` with no
  session host, refuses with a message.
- ✅ The built-in key fetch and its lifetime ([Built in](#built-in-a-profiles-key-fetch)). Tests: a missing or old
  key is fetched first, saying why; a fresh one connects, made the user's own; lifetime 0 fetches only when
  there's none; no Fetch command is as before; the command gets `{key_file}` and the variables filled in, and an
  undefined one says so; the editor's key state.
- ✅ Log in & cache key: the fetch on its own, in a tab, from the profile.
- ✅ Tried in production with CyberArk, on Windows: the key fetched when missing or expired, then the session
  connected to the device ([To check in production](#to-check-in-production) has what's left).
- To do: the rest of [To check in production](#to-check-in-production), the Mac among it; the hint after "Permission
  denied (publickey)" and Forget key; then a section in a user guide.
