# ironrdp-connector 0.10.0, patched

This is IronRDP's `ironrdp-connector` 0.10.0 as published on crates.io (MIT or Apache-2.0, Devolutions), with one
fix the published releases don't have yet. `src-tauri/Cargo.toml` puts it in place of the published crate with
`[patch.crates-io]`.

## What's changed

Only `src/connection.rs`: `ClientConnector::step` now first hands a PDU on the MCS message channel, arriving while
licensing, the capabilities exchange or connection finalization waits on the I/O channel, to
`step_message_channel_pdu`, which answers an Auto-Detect Request (or ignores anything else) and leaves the phase
waiting. Everything marked "SuperXTerminal's patch" there.

Why: Windows Server 2022 with RDS licensing starts continuous network detection early, and sends a probe in the
middle of licensing. The published connector handed it to the licensing decoder, and the RDP tab showed
"[decode during LicenseExchangeState::PlatformChallenge] ... invalid `securityHeaderFlags`". That's IronRDP issue
#1629, fixed upstream by #2128 (merged 2026-10-29); this is that fix, cut down to what 0.10.0 has.

The tests are `rdp::tests::connecting` in `src-tauri/src/rdp.rs`.

## When to drop it

Once a release of `ironrdp-client` depends on an `ironrdp-connector` with #2128 in it: upgrade, delete this folder and
the `[patch.crates-io]` lines, and check the tests above still pass.
