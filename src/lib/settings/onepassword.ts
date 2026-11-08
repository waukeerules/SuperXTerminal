/**
 * The words for 1Password secret references (op://vault/item/field), shared by Environment Variables, Settings >
 * Profiles' Password and Edit Session's Password: a value that's one is read from 1Password as a session opens
 * (src-tauri/src/onepassword.rs, through 1Password's CLI), and only the reference is kept here.
 */

/** A 1Password secret reference's shape, as an example */
export const OP_REFERENCE_EXAMPLE = "op://Vault/Item/password";

/** What a reference in place of a password or a value does, and what it needs */
export const OP_REFERENCE_HINT =
  `Or a 1Password reference, ${OP_REFERENCE_EXAMPLE}: read from 1Password, with its approval, as a session uses it, and ` +
  "only the reference kept here. Needs 1Password's CLI (op) with Integrate with 1Password CLI turned on in 1Password's " +
  "Settings > Developer.";
