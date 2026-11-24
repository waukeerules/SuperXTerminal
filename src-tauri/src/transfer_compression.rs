//! File Transfer's Compress checkbox: ssh's own compression (zlib, as -C or Compression=yes) for a file transfer's
//! connection, whatever the session's profile says. It starts as the profile has it (Settings > Profiles >
//! Connection > Compression, which reaches the transfer's ssh as the tab's -C) and the tab can turn it on or off,
//! reconnecting (`src/lib/transfer/FileTransfer.svelte`). sftp.rs gives the arguments it returns to ssh and to scp,
//! which take the same option.
//!
//! The files arrive as they were: only what crosses the network is compressed. That helps over a slow link, and
//! little on a LAN or for files that are compressed already (video, .zip, .gz).

/// The transfer's ssh arguments with compression `wanted` (None: as they are), and whether they compress.
///
/// Every -C is taken out and `-o Compression=yes|no` put first: ssh keeps the first value given for an option, so
/// that beats a Compression= of the user's own later on (in Extra ssh arguments), and a -C left in would set it
/// on whatever came first.
pub fn with_compression(mut args: Vec<String>, wanted: Option<bool>) -> (Vec<String>, bool) {
    let Some(on) = wanted else {
        let on = compresses(&args);
        return (args, on);
    };
    args.retain(|word| word != "-C");
    args.splice(0..0, ["-o".to_string(), format!("Compression={}", if on { "yes" } else { "no" })]);
    (args, on)
}

/// Whether ssh started with these arguments compresses: a -C, else the first Compression= given
fn compresses(args: &[String]) -> bool {
    if args.iter().any(|word| word == "-C") {
        return true;
    }
    args.iter()
        .find_map(|word| word.strip_prefix("Compression=").or_else(|| word.strip_prefix("-oCompression=")))
        .is_some_and(|value| value.eq_ignore_ascii_case("yes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Words, from one string
    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn it_starts_as_the_profile_has_it() {
        assert_eq!(with_compression(words("-p 22 -C sw1"), None), (words("-p 22 -C sw1"), true));
        assert_eq!(with_compression(words("-p 22 sw1"), None), (words("-p 22 sw1"), false));
        // the user's own option in Extra ssh arguments
        assert!(with_compression(words("-o Compression=yes sw1"), None).1);
    }

    #[test]
    fn ticked_it_compresses_and_unticked_it_doesnt_whatever_the_profile_says() {
        assert_eq!(with_compression(words("-p 22 sw1"), Some(true)), (words("-o Compression=yes -p 22 sw1"), true));
        // the profile's -C taken out, and no put first, ahead of the user's own yes
        let (args, on) = with_compression(words("-C -o Compression=yes sw1"), Some(false));
        assert_eq!(args, words("-o Compression=no -o Compression=yes sw1"));
        assert!(!on);
        assert!(!compresses(&args));
    }
}
