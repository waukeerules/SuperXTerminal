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
    {
      name: "CyberArk (MFA caching)",
      command: `sftp "{ad_username}@{cyberark_region}@key@{cyberark_gateway}:/key" "{key_file}"`,
      lifetime: 240,
      keyFile: sshFile("cyberark_key"),
      note: "The key from CyberArk's key target; sign in at the link it shows, then paste the PIN. Set {ad_username}, {cyberark_region} and {cyberark_gateway} (e.g. example.ssh.cyberark.cloud).",
    },
    {
      name: windows ? "A PowerShell function of your own" : "A shell function of your own",
      command: `pam_key "{key_file}"`,
      lifetime: 240,
      keyFile: sshFile("cyberark_key"),
      note: `A function in ${words.fetchFunctionHome} that writes the key to the path it's given (the fetch reads it).`,
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
      command: `tsh login --proxy={teleport_proxy} --user={ad_username} --out="{key_file}" --format=openssh`,
      lifetime: 720,
      keyFile: sshFile("teleport_key"),
      note: "Writes the key and its certificate (key-cert.pub, which ssh picks up). Set {teleport_proxy}; the lifetime is your cluster's.",
    },
    {
      name: "Smallstep",
      command: `step ssh certificate {email} "{key_file}" --no-password --insecure --force`,
      lifetime: 960,
      keyFile: sshFile("step_key"),
      note: "A new key and certificate from your step CA; the lifetime is your CA's.",
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
