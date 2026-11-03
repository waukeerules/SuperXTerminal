# CyberArk sessions

How SuperXTerminal connects through CyberArk's SSH gateway (Privileged Session Manager for SSH) while the session
itself keeps naming the real device. The app's side is built and covered by tests (`launch.rs`); CyberArk itself
can only be tried on the production network, so the parts that are guesses are marked as such, with
[what to check there](#to-check-in-production).

## What CyberArk needs

CyberArk isn't a jump host. ssh connects to the gateway (e.g. `example.cyberark.cloud`), and the *username* says who
you are and which target to open, for example:

```text
{ad_username}#{cyberark_region}@{cyberark_username}#{cyberark_address}@{host_address}#{cyberark_connector}
```

- `{ad_username}`, `{cyberark_region}`, `{cyberark_username}`, `{cyberark_address}` and `{cyberark_connector}` are
  each user's own Environment Variables (Tools > Environment Variables), so a shared Database session holds no one's
  login.
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

It already does (`launch.rs`). That matters here: `user@host` form would split this username at its last `@`, while
`-l` passes `#` and `@` through as they are.

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

On Windows today a `.bat` fetches the key with `psftp` from CyberArk's `key` target and converts it to `.ppk` with
WinSCP for PuTTY. The same on the Mac with OpenSSH, untested:

```bash
printf 'get /key %s\nquit\n' ~/.ssh/cyberark_key | sftp -b - "<user>@<domain>#<region>@key@example.ssh.cyberark.cloud"
```

```bash
chmod 600 ~/.ssh/cyberark_key
```

No conversion is needed for OpenSSH. (A guess to check: whether CyberArk hands out an OpenSSH key or a PuTTY one; if
it's `.ppk`, SuperXTerminal converts it already, as for any profile's key file.)

Later, possibly: a profile **Before connecting, run** command, so the key is fetched when it's missing or older than
its lifetime, instead of by hand.

## To check in production

- The connection string's exact parts and order for this tenant (region, connector), and whether `-l` with `#` and
  `@` reaches the gateway unchanged.
- The key: its format, its lifetime, and what the gateway says when it has expired (so the tab can say "fetch a new
  key").
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
- To do: this document's Mac key command, tried in production; then a section in a user guide.
