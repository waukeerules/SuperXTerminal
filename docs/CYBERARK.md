# CyberArk sessions

How SuperXTerminal connects through CyberArk's SSH gateway (Privileged Session Manager for SSH) while the session
itself keeps naming the real device. The app's side is built and covered by tests (`launch.rs`); CyberArk itself
can only be tried on the production network, so the parts that are guesses are marked as such, with
[what to check there](#to-check-in-production).

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

### Built in (proposed, not built)

A profile gets a **Key fetch** section, used only when it has a Key file:

| Field | What it's for |
|---|---|
| Fetch command | The command that gets a new key, e.g. the function's `sftp` line, with `{key_file}` for where to write it and the usual `{placeholders}` (each user's `{email}` and so on). One per system if they differ: macOS, Windows. |
| Lifetime | How long a key is good for, in minutes (240 here: CyberArk's 4 hours). |

When a session using the profile opens:

1. **The key is checked first.** No key file, or one older than the lifetime (by the file's modified time), and the
   key is fetched before connecting. The tab says why: "The CyberArk key is 4 h 12 min old (good for 4 h):
   fetching a new one." That's the "why can't I connect" answer, before ssh even starts.
2. **The fetch runs in the session's own tab**, as an ordinary terminal program, so CyberArk's link and its PIN
   prompt are right there. The link opens with Ctrl+click (or copy it), Okta signs in and shows the PIN, and the PIN
   is pasted into the tab. The app types nothing itself here: no stored password goes into a fetch.
3. **When the fetch ends**, and the key file is new, the tab goes on to connect with it. If the fetch fails or the
   file didn't change, the tab says so, and Enter tries again.
4. **One fetch at a time.** Several sessions opening together (Connect all, a layout) share it: the first tab
   fetches, and the others wait for it, saying so, then connect.
5. **Expired while open.** A key only matters at login, so open sessions carry on. A reconnect after the lifetime
   fetches again first.
6. **When the gateway refuses the key anyway** (revoked early, or the lifetime is set longer than CyberArk's), the
   tab adds a hint after ssh's "Permission denied (publickey)": the key's age, and that deleting it or Fetch now
   gets a new one. The profile's editor has **Fetch now** and shows the key's age.

The key is a credential, so:

- It lives in the user's own folder (`keys/cyberark-<profile>.key`, readable by the user alone: mode 600 on the
  Mac, the user's own access on Windows, as `.ppk` copies are kept today), never in the settings folder, the
  Database table or anything shared.
- It's deleted once it's past its lifetime and a fetch has replaced it, or by **Forget key** in the profile.
- The fetch command and lifetime are profile fields, which are per user; a team shares the command by writing it
  into its own setup notes.

What's needed: profile fields (`key_fetch_mac`, `key_fetch_windows`, `key_lifetime_minutes`), the age check and
the fetch-then-connect step in the tab (`PageHost.ts`, a pseudo terminal running the fetch, then the session), the
one-at-a-time lock, and the hint after "Permission denied (publickey)". All of it can be built and unit tested here
with a stand-in command; only CyberArk and Okta themselves need production.

## To check in production

- The connection string's exact parts and order for this tenant (region, connector), and whether `-l` with `#` and
  `@` reaches the gateway unchanged.
- The key: its format, its lifetime, and what the gateway says when it has expired (so the tab can say "fetch a new
  key").
- The fetch in a tab: that CyberArk's link and PIN prompt show as they do in Terminal, that the link opens with
  Ctrl+click on the Mac (where Ctrl+click is also a right-click) or else by copying it, and that a pasted PIN is
  taken.
- The gateway's host key: one key for every target, so known_hosts gets a single entry for the gateway.
- MFA or other keyboard-interactive prompts at the gateway, and whether the app's automatic password typing must stay
  out of them.
- File transfer and SCP through the gateway: probably not offered by PSM for SSH; the file transfer tab should say so
  rather than fail.
- Windows: the same profile fields, with the `.ppk` from the existing `.bat`.

## Done, and still to do

- ✅ `{host_address}` / `{host_port}` built-ins (`launch.rs`, filled before `variables::missing`). Tests: a username
  holding them starts, and an Environment Variable of the same name doesn't win.
- ✅ Profile `host` (`profiles.rs`, `profiles.ts`, the editor with its change dot; read from `gatewayHost` too, its
  first name) and the table above. Tests: the gateway is used only with `{host_address}` in the username, at the
  profile's port; an override's own username connects to the Host; `{host_address}` without a profile Host refuses;
  a session with no host uses its profile's, at the profile's port; no host anywhere, or `{host_address}` with no
  session host, refuses with a message.
- To do: the macOS function tried in a session tab in production (it's the Mac's counterpart of the `.bat`, for
  Terminal); then the built-in key fetch and its lifetime
  ([Built in](#built-in-proposed-not-built)); then a section in a user guide.
