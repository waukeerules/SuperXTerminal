/**
 * Examples for a profile's Fetch command (Settings > Profiles > Login), for keys issued for a short time: picked from a
 * list beside the field, then edited. Each fills the command, its lifetime and, when none is set, a key file. The
 * names in {braces} other than {key_file} are Environment Variables the user sets (Tools > Environment Variables,
 * which lists the ones used but not defined). Commands run through the user's shell on the Mac and Linux (its rc file
 * read: ~/.zshrc, ~/.bashrc) and PowerShell on Windows (its profile read), so OpenSSH's sftp and ssh-keygen work the
 * same on all three; where the shells differ (removing a file), the Mac and Linux share the Unix command.
 */
import { wordsFor, type System } from "../platform";

export interface KeyFetchExample {
  name: string;
  command: string;
  /** Minutes the key is good for, as the issuer usually gives it */
  lifetime: number;
  /** Where the key goes when the profile has none: in ~/.ssh, written as the system writes paths */
  keyFile: string;
  /** What to check or set before it works */
  note: string;
}

/**
 * The examples for a system: a Unix shell's commands on the Mac and Linux, PowerShell's on Windows
 * @param system which system's
 * @param onePassword Whether 1Password is turned on (Settings > General); off, its example isn't offered
 */
export function keyFetchExamples(system: System, onePassword = true): KeyFetchExample[] {
  const windows = system === "windows";
  const words = wordsFor(system);
  /** A file in ~/.ssh, as the system writes paths: %USERPROFILE%\.ssh\name on Windows (launch.rs reads it) */
  const sshFile = (name: string) => (windows ? `%USERPROFILE%\\.ssh\\${name}` : `~/.ssh/${name}`);
  const all: KeyFetchExample[] = [
    // CyberArk's documented command for an MFA caching key (its "MFA caching - *nix target" page):
    // sftp <username>@<login_suffix>#<subdomain>@key@<SSH gateway>:/key <key path>
    {
      name: "CyberArk: MFA caching key (sftp)",
      command: `sftp "{cyberark_username}@{cyberark_login_suffix}#{cyberark_subdomain}@key@{cyberark_gateway}:/key" "{key_file}"`,
      lifetime: 240,
      keyFile: sshFile("cyberark_key"),
      note: "Downloads the key from CyberArk's key target, after its sign-in (with MFA) in the tab. Set {cyberark_username}, {cyberark_login_suffix} (e.g. example.com), {cyberark_subdomain} (your tenant) and {cyberark_gateway} (your SSH gateway); set the lifetime to the one your CyberArk administrators chose.",
    },
    {
      name: windows ? "A PowerShell function of your own" : "A shell function of your own",
      command: `fetch_key "{key_file}"`,
      lifetime: 240,
      keyFile: sshFile("fetched_key"),
      note: `A function in ${words.fetchFunctionHome}, named as you like, that signs in and writes the key to the path it's given (the fetch reads it); set the lifetime to the key's.`,
    },
    {
      name: "1Password (an SSH Key item)",
      command: `op read "op://Private/SSH key/private key?ssh-format=openssh" --out-file "{key_file}" --force`,
      lifetime: 480,
      keyFile: sshFile("onepassword_key"),
      note: "Writes an SSH Key item's private key to the key file: change Private/SSH key to your vault and item. 1Password's own SSH agent is usually better, as the key never leaves 1Password: leave Key file empty with Offer SSH keys on.",
    },
    {
      name: "Teleport",
      command: `tsh login --proxy={teleport_proxy} --user={teleport_user} --out="{key_file}" --format=openssh`,
      lifetime: 720,
      keyFile: sshFile("teleport_key"),
      note: "Writes the key and its certificate (key-cert.pub, which ssh picks up); --format=openssh makes them OpenSSH's. Set {teleport_proxy} and {teleport_user}; 12 hours is Teleport's usual, else your cluster's.",
    },
    {
      name: "Smallstep",
      command: `step ssh certificate {email} "{key_file}" --no-password --insecure --force`,
      lifetime: 960,
      keyFile: sshFile("step_key"),
      note: "A new key and certificate from your step CA, the key unencrypted (--no-password needs --insecure). Set {email} to the identity your CA signs for; 16 hours is step-ca's usual, else your CA's.",
    },
    {
      name: "Test: a throwaway key",
      command: windows
        ? `Remove-Item -ErrorAction SilentlyContinue "{key_file}","{key_file}.pub"; ssh-keygen -q -t ed25519 -N '""' -f "{key_file}"`
        : `rm -f "{key_file}" "{key_file}.pub"; ssh-keygen -q -t ed25519 -N "" -f "{key_file}"`,
      lifetime: 1,
      keyFile: sshFile("fetch_test_key"),
      note: "Makes a new key every minute, to try the fetch without a real issuer. Devices won't accept it.",
    },
  ];
  return onePassword ? all : all.filter((e) => !e.name.startsWith("1Password"));
}
