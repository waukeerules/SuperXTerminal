//! Remote Desktop's sound, as SuperXPuTTY's Remote audio option set to "Play on this computer": what the remote
//! computer plays comes out of this computer's speakers.
//!
//! IronRDP's sound channel (RDPSND) takes a handler for the sound it's sent, and its client engine has one of its
//! own (ironrdp-rdpsnd-native, played through cpal). This one is attached in its place, as the session's own channel
//! (`with_static_channel` in rdp.rs), as the clipboard's is, because the engine's went wrong three ways on Windows:
//!
//! - It opens the stream to the speakers and never starts it. On the Mac a stream starts by itself, so it played;
//!   on Windows (WASAPI) and Linux (ALSA) a stream is made paused, so the remote computer showed its sound going to
//!   "Remote Audio", sent it, and nothing was heard here. The stream is started as it's opened ([`open`]).
//! - It offers Opus and PCM. The channel tells the server the formats both sides have, and the server then names a
//!   format by its place in that shorter list, while the handler is asked for the format at that place in its own.
//!   A Windows server has no Opus, so its "format 0" was PCM and the handler's was Opus: the sound went to the
//!   wrong decoder. Only PCM is offered here ([`format`]), so the places can't differ. It's 176 KB a second while
//!   sound plays, which a local network doesn't notice.
//! - With nothing to play, it waits four seconds in the sound device's own callback and then leaves the buffer as
//!   it found it, so the speakers get whatever was in it: a click every so often while the remote computer is quiet.
//!   Here the callback never waits, and fills what it has no sound for with silence ([`fill`]).
//!
//! The stream isn't `Send`, so it lives on a thread of its own for as long as the session plays; the sound reaches
//! it through a queue ([`Sound`]). What can't be played (no speakers) is said in the log.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _};
use ironrdp_rdpsnd::client::RdpsndClientHandler;
use ironrdp_rdpsnd::pdu::{AudioFormat, PitchPdu, VolumePdu, WaveFormat};

/// CD sound, the one format offered: 44.1 kHz, 16 bit, stereo, uncompressed, which every Windows has
const RATE: u32 = 44_100;
/// See [`RATE`]
const CHANNELS: u16 = 2;
/// The most sound kept waiting for the speakers, in bytes: one second. More than that is sound arriving faster than
/// it plays (the two computers' clocks differ a little), and the oldest is dropped so the sound doesn't fall behind
const MOST_WAITING: usize = RATE as usize * CHANNELS as usize * 2;

/// The sound waiting to be played: 16-bit samples as the bytes they came in (low byte first), left and right in turn
type Sound = Arc<Mutex<VecDeque<u8>>>;

/// The format offered to the remote computer, as Windows writes it
fn format() -> AudioFormat {
    AudioFormat {
        format: WaveFormat::PCM,
        n_channels: CHANNELS,
        n_samples_per_sec: RATE,
        n_avg_bytes_per_sec: RATE * u32::from(CHANNELS) * 2,
        n_block_align: CHANNELS * 2,
        bits_per_sample: 16,
        data: None,
    }
}

/// Plays the sound a Remote Desktop session sends, on this computer's default speakers
#[derive(Debug)]
pub struct Backend {
    /// The formats offered to the remote computer: [`format`] alone, so the place the server names it by is its place
    /// here
    formats: Vec<AudioFormat>,
    /// The sound waiting for the speakers
    sound: Sound,
    /// The thread that holds the stream to the speakers, parked until told to end; none until the first sound
    thread: Option<JoinHandle<()>>,
    /// Tells the stream's thread to let the stream go
    ended: Arc<AtomicBool>,
}

impl Backend {
    /// A handler with nothing playing yet; the speakers are opened when the first sound comes
    pub fn new() -> Self {
        Self { formats: vec![format()], sound: Sound::default(), thread: None, ended: Arc::new(AtomicBool::new(false)) }
    }
}

/// Give the speakers their next samples: the sound waiting, and silence for the rest. Never waits: this runs in the
/// sound device's callback, where a wait is a gap, and samples left as they were are a click.
fn fill(out: &mut [i16], sound: &mut VecDeque<u8>) {
    for sample in out {
        *sample = match (sound.len() >= 2).then(|| (sound.pop_front(), sound.pop_front())) {
            Some((Some(low), Some(high))) => i16::from_le_bytes([low, high]),
            _ => 0,
        };
    }
}

/// Add a piece of sound to what's waiting, dropping the oldest beyond [`MOST_WAITING`] (whole samples of both
/// channels, so left and right stay in step)
fn add(sound: &mut VecDeque<u8>, piece: &[u8]) {
    sound.extend(piece);
    let frame = usize::from(CHANNELS) * 2;
    let over = sound.len().saturating_sub(MOST_WAITING).div_ceil(frame) * frame;
    sound.drain(..over.min(sound.len()));
}

/// Open this computer's default speakers and start playing what's put in `sound`. The start is what IronRDP's own
/// handler leaves out: without it the stream stays paused on Windows and Linux, and takes nothing.
fn open(sound: Sound) -> Result<cpal::Stream, String> {
    let device = cpal::default_host().default_output_device().ok_or("there's no sound device to play on")?;
    let config = cpal::StreamConfig { channels: CHANNELS, sample_rate: RATE, buffer_size: cpal::BufferSize::Default };
    let stream = device
        .build_output_stream(
            &config,
            move |out: &mut [i16], _: &cpal::OutputCallbackInfo| fill(out, &mut sound.lock().unwrap_or_else(|poisoned| poisoned.into_inner())),
            |error| crate::applog::warn(format!("Remote Desktop's sound stopped: {error}")),
            None,
        )
        .map_err(|error| error.to_string())?;
    stream.play().map_err(|error| error.to_string())?;
    Ok(stream)
}

impl RdpsndClientHandler for Backend {
    /// The formats this computer can play
    fn get_formats(&self) -> &[AudioFormat] {
        &self.formats
    }

    /// A piece of sound: the speakers are opened for the first one, and it joins what's waiting for them
    fn wave(&mut self, format_no: usize, _ts: u32, data: Cow<'_, [u8]>) {
        // one format is offered, so any other number is a server's mistake
        if format_no != 0 {
            return;
        }
        if self.thread.is_none() {
            self.ended.store(false, Ordering::Relaxed);
            let (ended, sound) = (Arc::clone(&self.ended), Arc::clone(&self.sound));
            let thread = std::thread::Builder::new().name("rdp-sound".into()).spawn(move || match open(sound) {
                Ok(stream) => {
                    crate::applog::info("Remote Desktop's sound is playing on this computer");
                    // the stream plays for as long as it's kept: until the session closes it
                    while !ended.load(Ordering::Relaxed) {
                        std::thread::park();
                    }
                    drop(stream);
                }
                Err(error) => crate::applog::warn(format!("Remote Desktop's sound can't be played on this computer: {error}")),
            });
            self.thread = thread.ok();
        }
        add(&mut self.sound.lock().unwrap_or_else(|poisoned| poisoned.into_inner()), &data);
    }

    /// The remote computer's volume: left to this computer's own
    fn set_volume(&mut self, _volume: VolumePdu) {}

    /// The remote computer's pitch: not used
    fn set_pitch(&mut self, _pitch: PitchPdu) {}

    /// Stop playing and let the speakers go (the session ended, or the server closed its sound)
    fn close(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.ended.store(true, Ordering::Relaxed);
            thread.thread().unpark();
            let _ = thread.join();
        }
        self.sound.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clear();
    }
}

impl Drop for Backend {
    /// The session is gone: its stream's thread ends with it
    fn drop(&mut self) {
        self.close();
    }
}

// What the speakers are given, sample by sample ([`fill`], [`add`]), and the stream itself, opened as a session's
// first sound opens it: against this computer's real default output device, with silence, so nothing is heard. A
// machine with no sound device (a build machine) has nothing to open, and that test then has nothing to check. The
// same on Windows, the Mac and Linux.
#[cfg(test)]
mod tests {
    use super::*;

    /// Samples as the bytes the remote computer sends them
    fn bytes(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|sample| sample.to_le_bytes()).collect()
    }

    #[test]
    fn with_nothing_to_play_the_speakers_get_silence_and_no_click() {
        let mut sound = VecDeque::new();
        // the device's buffer comes with whatever was in it; left alone, that was the click
        let mut out = [1234i16; 8];
        fill(&mut out, &mut sound);
        assert_eq!(out, [0; 8]);
        // sound that ends partway through a buffer is followed by silence, and half a sample isn't played
        add(&mut sound, &bytes(&[100, -100, 200]));
        sound.push_back(7);
        let mut out = [1234i16; 6];
        fill(&mut out, &mut sound);
        assert_eq!(out, [100, -100, 200, 0, 0, 0]);
    }

    #[test]
    fn sound_that_arrives_faster_than_it_plays_doesnt_fall_behind() {
        let mut sound = VecDeque::new();
        // a second and a half arrives with none played: the oldest half second goes, on a whole sample of both channels
        let half_second: Vec<i16> = (0..RATE).map(|n| (n % 3) as i16).collect();
        for _ in 0..3 {
            add(&mut sound, &bytes(&half_second));
        }
        assert_eq!(sound.len(), MOST_WAITING);
        assert_eq!(sound.len() % (usize::from(CHANNELS) * 2), 0);
    }

    #[test]
    fn sound_sent_by_the_remote_computer_is_taken_by_the_speakers() {
        if cpal::default_host().default_output_device().is_none() {
            return;
        }
        // a stream that isn't playing (Windows' and Linux's, until started) never asks for sound, and it stays waiting
        let sound = Sound::default();
        add(&mut sound.lock().unwrap(), &vec![0u8; 4096]);
        let stream = open(Arc::clone(&sound)).expect("the speakers open");
        let taken = (0..100).any(|_| {
            std::thread::sleep(std::time::Duration::from_millis(20));
            sound.lock().unwrap().is_empty()
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
