# 1Password

How SuperXTerminal uses 1Password for passwords and SSH keys, so they don't have to be copied into the app. Built
and covered by tests (`src-tauri/src/onepassword.rs`, `launch.rs`), and tried against 1Password on Windows and the
Mac (November 2026).

It's off until you turn it on: Settings > General > 1Password > **Use 1Password references**. Off, nothing is read
from 1Password, its CLI never runs, and Settings and Edit Session don't offer references. A reference already
typed somewhere isn't read: the tab says "1Password is turned off" and the session goes on without it. 1Password's SSH
agent works either way, as ssh reaches it by itself.

There are two parts, and you can use either on its own:

- **SSH keys** come from 1Password's SSH agent. ssh asks the agent, and 1Password asks you to approve each use. The
  private key never leaves 1Password.
- **Passwords and Environment Variables** can be *secret references*, `op://vault/item/field`. The app reads each one
  with 1Password's command-line tool (`op`) when a session uses it. Only the reference is stored in the app.

## Setting up 1Password

In the 1Password app, open Settings > Developer:

1. Turn on **Use the SSH Agent**, for keys.
2. Turn on **Integrate with 1Password CLI**, for references.

On Windows, 1Password's agent takes over the pipe Windows' own OpenSSH agent uses (`\\.\pipe\openssh-ssh-agent`).
Turn off the Windows service "OpenSSH Authentication Agent" first, as 1Password's setup says.

For references, install 1Password's CLI:

| | |
|---|---|
| Windows | `winget install 1password-cli` |
| Mac | `brew install 1password-cli` |

Then start SuperXTerminal again, and turn on Settings > General > 1Password > **Use 1Password references**.
**Check 1Password** there should list the vaults it can
read; if it can't, it says why, with the same install command and a link to
[1Password's CLI guide](https://www.1password.dev/cli/get-started).

The app looks for `op` on the PATH, and also where winget and Homebrew put it. An app started before `op` was
installed keeps its old PATH, and a Mac app never gets the shell's PATH at all.

## SSH keys

**Any key in 1Password.** Leave the profile's Key file empty, with SSH keys set to offer the keys (Settings >
Profiles > Login). ssh asks the agent, which offers each key it has.

**One key in particular.** Put a reference to the key in the profile's Key file. Use 1Password's
**Copy Secret Reference** on the key's private key; the quotes it adds are fine:

```text
"op://Private/SSH Key - Lab Server/private key"
```

The app reads that item's public key (the public key isn't secret, and the private key is never read) and passes ssh
`-i <public key> -o IdentitiesOnly=yes`, so ssh asks the agent for that key alone. The tab says so:

```text
Logging in with the key Private/SSH Key - Lab Server from 1Password, through its SSH agent.
```

Pin the key for a Cisco device. It closes the connection after three failed attempts, and an agent holding several
keys offers each one, so the right key, or the password, may never get its turn.

On the Mac, ssh uses the system's agent unless `~/.ssh/config` says otherwise. For a key in 1Password the app points
it at 1Password's agent (`IdentityAgent`, its `~/.1password/agent.sock` link, else its socket in
`~/Library/Group Containers/...`). On Windows nothing extra is needed.

The profile's Fetch command has a 1Password example too, which writes an SSH Key item's private key to the Key file.
The agent is usually better: with the Fetch command, the key sits on disk.

## Passwords and Environment Variables

These can each be a reference:

- a session's own password (Edit Session > Password)
- a profile's password (Settings > Profiles > Login > Password)
- an Environment Variable's value (Tools > Environment Variables), such as the one a shared session's
  `-pw {admin_password}` uses

```text
op://Lab/Cisco Switch/password
```

When a session uses one, the app runs `op read` and 1Password asks you to approve it. A reference isn't a secret, so
an Environment Variable holding one needn't be marked Secret.

A shared Database session can say `-pw {admin_password}`, and each person's `{admin_password}` can point at their own
vault. Nobody's password is in the table, nor in each person's Windows Credential Manager or Keychain.

A value read is kept in memory for 10 minutes, so tabs opened together ask once. It's forgotten when Environment
Variables or a profile's password are saved, and never written anywhere.

## When something can't be read

The session still opens, and the tab says why in yellow at the top, for example:

```text
op://Lab/Cisco Switch/password wasn't read: the request was turned down in 1Password.
```

Without the password, ssh asks for it in the tab. Without the key, ssh connects without it. The usual causes:

- **1Password is turned off:** turn on Settings > General > 1Password > Use 1Password references.
- **op isn't installed:** the message gives the install command.
- **op isn't signed in:** turn on Integrate with 1Password CLI, and keep 1Password running.
- **The approval was turned down,** or not given within two minutes.
- **The reference is wrong:** the message is op's own, such as an item that isn't in the vault.

Opening a tab doesn't hold up the window while 1Password waits for you: tabs, Remote Desktop and Connect external
open in the background.
