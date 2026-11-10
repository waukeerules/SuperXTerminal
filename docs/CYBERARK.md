# CyberArk sessions

How SuperXTerminal connects through CyberArk's Privileged Session Manager for SSH (PSM for SSH, also called PSMP)
while the session itself keeps naming the real device. The app's side is built and covered by tests (`launch.rs`),
and it has been tried with CyberArk on a production network, on Windows (November 2026).

## What PSM for SSH is

From CyberArk's [Introduction to PSM for SSH](https://docs.cyberark.com/pam-self-hosted/latest/en/content/pasimp/introduction-to-psmp.htm):

- It sits between the user's own SSH client and the target devices. The user connects to the PSM for SSH machine on
  the standard SSH port; it authenticates the user to CyberArk's Vault, takes the privileged account's credentials
  from the Vault (as the user's permissions in the Safe allow) and opens the session to the target, by SSH or Telnet.
- The user never sees the target's password (privileged single sign-on). Accounts kept in the Vault without a
  password are a non-privileged SSO session instead: the user types the target's password when asked.
- Every session is recorded as text in the Vault for auditing, and commands the policy doesn't allow can be blocked.
- Users authenticate to the Vault with a CyberArk or LDAP password, RADIUS, or an SSH key. Native SFTP clients and
  `scp` work through it too.

So CyberArk isn't a jump host: ssh connects to the PSM for SSH address (the **gateway**), and the *username* says who
you are and which target to open.

## The connection string

CyberArk's [Connect to Unix machines (using PSM for SSH)](https://docs.cyberark.com/privilege-cloud-shared-services/latest/en/content/privilege%20cloud/privcloud-connect-using-ssh.htm)
(Privilege Cloud) and [Connect through PSM for SSH](https://docs.cyberark.com/pam-self-hosted/latest/en/content/pasimp/psso-pmsp.htm)
(self-hosted) give it as:

```text
<ssh client> [-i <key file>] User@TargetUser#DomainAddress@TargetAddress[#TargetPort]@ProxyAddress
```

| Part | What it is | Required |
|---|---|---|
| `User` | Your CyberArk (Vault) user name. It may hold one `@` of its own (`john@example.com`); with an MFA caching key it must be the full name in that form, not a short one | Yes |
| `TargetUser` | The account on the target, e.g. `root` (case sensitive) | Yes |
| `DomainAddress` | The domain server, or the address a centrally managed account (or SSH certificate group) is filed under | No |
| `TargetAddress` | The target: IPv4, IPv6 (with `-` in place of `:`) or a DNS name (the DNS name, if that's how the account was defined) | Yes |
| `TargetPort` | The target's port when the account doesn't set one: 22 means SSH, 23 Telnet | No |
| `ProxyAddress` | The PSM for SSH machine's address or DNS name | Yes |

- `@` separates the parts, and `#` comes before an optional one. Both are CyberArk's defaults, which an
  administrator can change ([PSM for SSH Syntax Delimiter](https://docs.cyberark.com/pam-self-hosted/latest/en/content/pasimp/configuring-psmp-syntax-delimiters.htm)).
- Your tenant may add parts of its own to this form (a tenant name after your user name, a connector pool after the
  target): use the string your CyberArk administrators give you.
- PSM for SSH asks for any required part that's missing, except over `scp`, which must be given them all.

### In SuperXTerminal

Everything before the last `@ProxyAddress` is the username. It goes in the **profile's** Username, with each person's
parts as Environment Variables and the target as `{host_address}`, for example:

```text
{cyberark_user}@{target_user}@{host_address}
```

- `{cyberark_user}`, `{target_user}` and any other part are each user's own Environment Variables (Tools >
  Environment Variables), so a shared Database session holds no one's login. The names are only names: what matters
  is that the profile's username and the Environment Variables use the same ones.
- `{host_address}` is the target: the session's own Host (and `{host_port}` its port, for `#{host_port}`).
- `ProxyAddress` is the profile's Host (below), so it isn't part of the username.
- The login is a password, or an SSH key that CyberArk issues for a short time with MFA caching
  ([Getting the key](#getting-the-key)).

So a CyberArk session has two addresses: the **target** (the session's Host, which stays filled in) and the
**gateway** (where ssh actually connects).

## Why the session's Host stays filled in

With the target in the session's own Host field, the same shared session works both ways:

- **Through CyberArk**: its profile ("CyberArk") supplies the gateway, the key and the connection string, and the
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
- The tab says where it's going first: "Connecting to 192.168.0.50 through the gateway psmp.example.com.", or
  "No host of its own: connecting to … (the Host of profile "…").".
- A jump host still applies, to the gateway.
- The app's log records `ssh command: … -l <user> <gateway>`; Verbose SSH (Settings > General > Diagnostics) shows
  the gateway's side of the login.

### 3. The username goes to ssh with `-l`

It already does (`launch.rs`). `user@host` form would work too, as OpenSSH splits it at its *last* `@`; `-l` just
leaves no doubt about where the username ends, whatever `@` and `#` it holds.

### 4. The CyberArk profile

| Field | Value |
|---|---|
| Username | the connection string, without `@ProxyAddress` |
| Host | the PSM for SSH address (the gateway) |
| Key file | the MFA caching key (`~/.ssh/cyberark_key`, say; on Windows `%USERPROFILE%\.ssh\cyberark_key` or `~/...` both work, or the `.ppk`, which is converted for ssh); empty to log in with a password |
| SSH keys | either: with a key file, only that key is offered (`IdentitiesOnly`), so no other key is spent on the gateway |
| No username | use ssh's own (the profile always gives one) |

The session's own Username stays blank (or the profile's is overridden per user), so the profile's connection
string applies.

## Getting the key

With MFA caching, you sign in once (with your identity provider's MFA) and CyberArk gives you an SSH key good for a
set time, which then logs you in to every target you're allowed, with no further sign-in until it expires. From
CyberArk's [Authenticate to *nix accounts with MFA caching](https://docs.cyberark.com/access/latest/en/privilege-cloud/pasimp/mfa-caching-access/)
and [MFA caching (PSM for SSH)](https://docs.cyberark.com/privilege-cloud-standard/latest/en/content/pasimp/mfa-caching.htm):

- Your administrators turn it on and set how long a key lasts (`GracePeriodMinutes`), and whether it needs a
  passphrase. It needs PSM for SSH 12.1 or later, with OpenSSH 7.8 or later on its machine.
- The key is generated in CyberArk's web portal (PSM for SSH MFA caching), with its REST API, or, where CyberArk's
  [MFA caching - *nix target](https://docs.cyberark.com/access/latest/en/infrastructure/sas-linux-mfa-cache/) page
  applies, downloaded with `sftp <username>@<login_suffix>#<subdomain>@key@<SSH gateway>:/key <key path>` after a
  sign-in. It comes as OpenSSH, PEM or PPK. SuperXTerminal uses Windows' and the Mac's own OpenSSH, so take the OpenSSH one (a `.ppk`
  works anyway: the app converts any profile's `.ppk`).
- It's used as any key is, `ssh -i <key file>`, with the full user name (`name@example.com`) in the connection
  string. ssh refuses a key others can read; CyberArk's docs say to `chmod 600` it, which the app does itself after a
  fetch.
- Administrators can't log in through PSM for SSH with a key, and an administrator can revoke a key early.

Downloading the key by hand each time works: point the profile's Key file at where you save it. Or let the profile
fetch it.

### Built in: a profile's key fetch

Not CyberArk's alone: anything that issues SSH keys or certificates for a short time from a command fits, such as
HashiCorp Vault (`vault ssh`, signed certificates), Teleport (`tsh login`), Smallstep (`step ssh login`), Okta
Advanced Server Access (`sft`) or Azure (`az ssh cert`). A certificate written beside the key as `<key>-cert.pub` is
picked up by OpenSSH by itself.

Settings > Profiles > Login, under Key file:

| Field | What it's for |
|---|---|
| Fetch command | The command that gets a new key into the Key file, with `{key_file}` for that path and the usual `{placeholders}` (each user's own, as in the username). Profiles are per computer, so each machine has its own. |
| Lifetime | How long a key is good for, in minutes: your CyberArk key's validity period (e.g. 240 for 4 hours); 0 fetches only when there's none. |

**Examples…** beside the Fetch command fills it from a list for the system it's on (`keyFetchExamples.ts`): CyberArk
(the `sftp` download of an MFA caching key, as CyberArk documents it), a shell or PowerShell function of your own, Teleport
(`tsh login … --out`), Smallstep (`step ssh certificate`), and a test that makes a throwaway key every minute. Picking
one also sets the Lifetime, and the Key file when there's none, and says what to set (its Environment Variables), for
editing from there. Any command that signs you in and writes the key to `{key_file}` will do: CyberArk's REST API
called from a script, say.

Below them the editor says how the next session will find the key: "Key: 37 min old, good for 4 h.", "The key is
5 h old, past its 4 h: the next session fetches a new one.", or "No key yet".

When a session using the profile opens (`launch.rs` and `PageHost.ts`):

1. **The key is checked first.** No key file, or one older than the lifetime (by the file's modified time), and the
   key is fetched before connecting. The tab says why: "The key is 4 h 12 min old (good for 4 h): fetching a new
   one." That's the "why can't I connect" answer, before ssh even starts.
2. **The fetch runs in the session's own tab** through the user's shell, its `~/.zshrc` read (`-i`) so a shell
   function works, or PowerShell on Windows. A sign-in it asks for shows there: a link opens with Ctrl+click (or copy
   it), and a code or PIN the identity provider shows is pasted into the tab. The app types nothing itself here: no
   stored password goes into a fetch.
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

Not built yet:

- A hint after ssh's "Permission denied (publickey)" when the gateway refuses the key anyway (revoked early, or the
  lifetime set longer than the issuer's), with the key's age.
- **Forget key** in the profile editor.
- Tabs in different windows share nothing: each window fetches on its own.

The key is a credential, so it's kept only where the profile's Key file says, in the user's own folders (never the
settings folder, the Database table or anything shared), and the fetch command and lifetime are profile fields,
which are per user.

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
  connected to the device.
- To do: trying it on the Mac; the hint after "Permission denied (publickey)" and Forget key; then a section in a
  user guide.
