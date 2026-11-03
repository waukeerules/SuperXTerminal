/**
 * Examples for a profile's Fetch command (Settings > Profiles > Login), for keys issued for a short time: picked from a
 * list beside the field, then edited. Each fills the command, its lifetime and, when none is set, a key file. The
 * names in {braces} other than {key_file} are Environment Variables the user sets (Tools > Environment Variables,
 * which lists the ones used but not defined). Commands run through the user's shell on the Mac (its ~/.zshrc read) and
 * PowerShell on Windows (its profile read), so OpenSSH's sftp and ssh-keygen work the same on both.
 */
export interface KeyFetchExample {
  name: string;
  command: string;
  /** Minutes the key is good for, as the issuer usually gives it */
  lifetime: number;
  keyFile: string;
  /** What to check or set before it works */
  note: string;
}

export function keyFetchExamples(mac: boolean): KeyFetchExample[] {
  return [
    {
      name: "CyberArk (MFA caching)",
      command: `sftp "{ad_username}@{domain}#{cyberark_region}@key@{cyberark_gateway}:/key" "{key_file}"`,
      lifetime: 240,
      keyFile: "~/.ssh/cyberark_key",
      note: "The key from CyberArk's key target; sign in at the link it shows, then paste the PIN. Set {ad_username}, {domain}, {cyberark_region} and {cyberark_gateway} (e.g. example.ssh.cyberark.cloud).",
    },
    {
      name: mac ? "A shell function of your own" : "A PowerShell function of your own",
      command: `pam_key "{key_file}"`,
      lifetime: 240,
      keyFile: "~/.ssh/cyberark_key",
      note: mac
        ? "A function in your ~/.zshrc that writes the key to the path it's given (the fetch reads ~/.zshrc)."
        : "A function in your PowerShell profile that writes the key to the path it's given.",
    },
    {
      name: "Teleport",
      command: `tsh login --proxy={teleport_proxy} --user={ad_username} --out="{key_file}" --format=openssh`,
      lifetime: 720,
      keyFile: "~/.ssh/teleport_key",
      note: "Writes the key and its certificate (key-cert.pub, which ssh picks up). Set {teleport_proxy}; the lifetime is your cluster's.",
    },
    {
      name: "Smallstep",
      command: `step ssh certificate {email} "{key_file}" --no-password --insecure --force`,
      lifetime: 960,
      keyFile: "~/.ssh/step_key",
      note: "A new key and certificate from your step CA; the lifetime is your CA's.",
    },
    {
      name: "Test: a throwaway key",
      command: mac
        ? `rm -f "{key_file}" "{key_file}.pub"; ssh-keygen -q -t ed25519 -N "" -f "{key_file}"`
        : `Remove-Item -ErrorAction SilentlyContinue "{key_file}","{key_file}.pub"; ssh-keygen -q -t ed25519 -N '""' -f "{key_file}"`,
      lifetime: 1,
      keyFile: "~/.ssh/fetch_test_key",
      note: "Makes a new key every minute, to try the fetch without a real issuer. Devices won't accept it.",
    },
  ];
}
