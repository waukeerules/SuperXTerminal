/**
 * The words for 1Password secret references (op://vault/item/field), shared by Environment Variables, Settings >
 * Profiles' Password and Edit Session's Password: a value that's one is read from 1Password as a session opens
 * (src-tauri/src/onepassword.rs, through 1Password's CLI), and only the reference is kept here.
 */

/** A 1Password secret reference's shape, as an example */
export const OP_REFERENCE_EXAMPLE = "op://Vault/Item/password";

/** A key in 1Password, as a profile's Key file names it: its SSH Key item */
export const OP_KEY_EXAMPLE = "op://Vault/SSH key/private key";

/** What a reference as a Key file does: ssh asks 1Password's SSH agent for that key alone */
export const OP_KEY_HINT =
  `Or a key in 1Password, ${OP_KEY_EXAMPLE} (Copy Secret Reference on its private key): ssh asks 1Password's SSH ` +
  "agent for that key alone, by its public key, and offers no other, so a device that allows a few attempts isn't " +
  "used up by the agent's other keys. The private key never leaves 1Password.";

/**
 * Whether a value is a 1Password reference, quoted or not (as onepassword.rs's is_reference)
 * @param value A password, a variable's value or a key file
 */
export function isReference(value: string): boolean {
  return /^\s*["']?\s*op:\/\//i.test(value);
}

/**
 * A key file reference as the item it names, "Private/OMV key", for saying where a key comes from
 * @param value The reference, quoted or not
 */
export function referencedItem(value: string): string {
  const path = value.trim().replace(/^["']|["']$/g, "").replace(/^op:\/\//i, "").split("?")[0];
  return path.split("/").slice(0, 2).join("/");
}

/** What a reference in place of a password or a value does, and what it needs */
export const OP_REFERENCE_HINT =
  `Or a 1Password reference, ${OP_REFERENCE_EXAMPLE}: read from 1Password, with its approval, as a session uses it, and ` +
  "only the reference kept here. Needs 1Password's CLI (op) with Integrate with 1Password CLI turned on in 1Password's " +
  "Settings > Developer.";
