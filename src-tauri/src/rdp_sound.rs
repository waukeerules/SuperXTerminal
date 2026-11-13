//! Remote Desktop's sound, as SuperXPuTTY's Remote audio option set to "Play on this computer": what the remote
//! computer plays comes out of this computer's speakers.
//!
//! IronRDP's sound channel (RDPSND) takes a handler for the sound it's sent, and its client engine has one of its
//! own (ironrdp-rdpsnd-native: Opus or PCM, played through cpal). That one opens the stream to the speakers and
//! never starts it. On the Mac a stream starts by itself, so it played; on Windows (WASAPI) and Linux (ALSA) a
//! stream is made paused, so the remote computer showed its sound going to "Remote Audio", sent it, and nothing was
//! heard here. So the engine's is turned off and this one is attached as the session's own channel
//! (`with_static_channel` in rdp.rs), as the clipboard's is: IronRDP's stream to the speakers (`DecodeStream`),
//! started as it's opened ([`open`]), and saying in the log when the speakers can't be used.
//!
//! Only PCM is offered, not IronRDP's Opus as well. The channel tells the server the formats both sides have, and
//! the server then names a format by its place in that shorter list, while the handler is asked for the format at
//! that place in its own. A Windows server has no Opus, so its "format 0" was PCM and the handler's was Opus: the
//! sound went to the wrong decoder and nothing was heard. With one format offered the places can't differ. PCM is
//! 176 KB a second while sound plays, which a local network doesn't notice.
//!
//! The stream isn't `Send`, so it lives on a thread of its own for as long as one format plays; the sound reaches it
//! over a channel.

use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;

use cpal::traits::StreamTrait as _;
use ironrdp_rdpsnd::client::RdpsndClientHandler;
use ironrdp_rdpsnd::pdu::{AudioFormat, PitchPdu, VolumePdu, WaveFormat};
use ironrdp_rdpsnd_native::cpal::DecodeStream;

/// Plays the sound a Remote Desktop session sends, on this computer's default speakers
#[derive(Debug)]
pub struct Backend {
    /// The formats offered to the remote computer: IronRDP's PCM one alone (44.1 kHz, 16 bit, stereo), so the place
    /// the server names it by is its place here
    formats: Vec<AudioFormat>,
    /// The format playing now (its place in `formats`); another one closes the stream and opens a new one
    playing: Option<usize>,
    /// The thread that holds the stream, parked until told to end
    thread: Option<JoinHandle<()>>,
    /// Tells the stream's thread to let the stream go
    ended: Arc<AtomicBool>,
    /// Where the sound goes: to the stream's thread
    sound: Option<Sender<Vec<u8>>>,
}

impl Backend {
    /// A handler with nothing playing yet; the speakers are opened when the first sound comes
    pub fn new() -> Self {
        let formats = ironrdp_rdpsnd_native::cpal::RdpsndBackend::new()
            .get_formats()
            .iter()
            .filter(|format| format.format == WaveFormat::PCM)
            .cloned()
            .collect();
        Self { formats, playing: None, thread: None, ended: Arc::new(AtomicBool::new(false)), sound: None }
    }
}

/// Open this computer's default speakers for a format and start playing what comes over `sound`. The start is what
/// IronRDP's own handler leaves out: without it the stream stays paused on Windows and Linux, and takes nothing.
fn open(format: &AudioFormat, sound: Receiver<Vec<u8>>) -> Result<DecodeStream, String> {
    let stream = DecodeStream::new(format, sound).map_err(|error| error.report().to_string())?;
    stream.stream().play().map_err(|error| error.to_string())?;
    Ok(stream)
}

impl RdpsndClientHandler for Backend {
    /// The formats this computer can play
    fn get_formats(&self) -> &[AudioFormat] {
        &self.formats
    }

    /// A piece of sound in the format at `format_no`: the speakers are opened for the first one (and again when the
    /// format changes), and it's passed to them
    fn wave(&mut self, format_no: usize, _ts: u32, data: Cow<'_, [u8]>) {
        if self.playing != Some(format_no) {
            self.close();
        }
        if self.thread.is_none() {
            let Some(format) = self.formats.get(format_no).cloned() else { return };
            let (sound, received) = mpsc::channel();
            self.sound = Some(sound);
            self.playing = Some(format_no);
            self.ended.store(false, Ordering::Relaxed);
            let ended = Arc::clone(&self.ended);
            let thread = std::thread::Builder::new().name("rdp-sound".into()).spawn(move || match open(&format, received) {
                Ok(stream) => {
                    crate::applog::info(format!(
                        "Remote Desktop's sound is playing on this computer ({} Hz, {} channels)",
                        format.n_samples_per_sec, format.n_channels
                    ));
                    // the stream plays for as long as it's kept: until the session closes it or changes format
                    while !ended.load(Ordering::Relaxed) {
                        std::thread::park();
                    }
                    drop(stream);
                }
                Err(error) => crate::applog::warn(format!("Remote Desktop's sound can't be played on this computer: {error}")),
            });
            self.thread = thread.ok();
        }
        if let Some(sound) = &self.sound {
            // a stream that couldn't be opened has let go of its end: the sound is dropped
            let _ = sound.send(data.into_owned());
        }
    }

    /// The remote computer's volume: left to this computer's own
    fn set_volume(&mut self, _volume: VolumePdu) {}

    /// The remote computer's pitch: not used
    fn set_pitch(&mut self, _pitch: PitchPdu) {}

    /// Stop playing and let the speakers go (the session ended, or the format changed)
    fn close(&mut self) {
        self.sound = None;
        self.playing = None;
        if let Some(thread) = self.thread.take() {
            self.ended.store(true, Ordering::Relaxed);
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}

impl Drop for Backend {
    /// The session is gone: its stream's thread ends with it
    fn drop(&mut self) {
        self.close();
    }
}

// The stream to the speakers, opened as a session's first sound opens it: against this computer's real default
// output device, with silence, so nothing is heard. A machine with no sound device (a build machine) has nothing to
// open, and the test then has nothing to check. The same on Windows, the Mac and Linux.
#[cfg(test)]
mod tests {
    use cpal::traits::HostTrait as _;

    use super::*;

    #[test]
    fn sound_sent_by_the_remote_computer_is_taken_by_the_speakers() {
        if cpal::default_host().default_output_device().is_none() {
            return;
        }
        let formats = Backend::new().get_formats().to_vec();
        let pcm = formats.first().expect("a format is offered");
        // a channel that holds nothing: a piece of sound is only taken when the stream asks for it, which a paused
        // stream (Windows' and Linux's, until started) never does
        let (sound, received) = mpsc::sync_channel(0);
        let stream = open(pcm, received).expect("the speakers open");
        let silence = vec![0u8; 4096];
        let taken = (0..100).any(|_| {
            std::thread::sleep(std::time::Duration::from_millis(20));
            sound.try_send(silence.clone()).is_ok()
        });
        drop(stream);
        assert!(taken, "the stream never asked for sound: it isn't playing");
    }

    #[test]
    fn the_format_the_server_names_is_the_one_played() {
        // the server is told the formats both sides have and names one by its place among those; with more than one
        // offered here, a server without the first (Windows has no Opus) named PCM as 0 and Opus was played
        let backend = Backend::new();
        let [only] = backend.get_formats() else { panic!("one format is offered, so its place can't differ") };
        // one every Windows server has, as Windows writes it
        assert_eq!((only.format, only.n_channels, only.n_samples_per_sec, only.bits_per_sample), (WaveFormat::PCM, 2, 44100, 16));
        assert_eq!((only.n_block_align, only.n_avg_bytes_per_sec, &only.data), (4, 176_400, &None));
    }
}
