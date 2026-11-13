//! Remote Desktop's clipboard, as SuperXPuTTY's Clipboard option (RedirectClipboard): text copied in the remote
//! desktop is on this computer's clipboard, and text copied here pastes in the remote desktop.
//!
//! IronRDP's clipboard channel (CLIPRDR) takes a backend. Its client engine has one only for Windows, which wants a
//! window's message loop of its own, and a do-nothing stub elsewhere; so the engine's is turned off and this one is
//! attached as the session's own channel (`with_static_channel`), on the Mac, Windows and Linux alike. Its messages go back
//! through the engine's input channel, as the engine's own backend's do. This computer's clipboard
//! ([`crate::clipboard`]) is read when the tab gets the keyboard (`announce`), so a copy made in another app is
//! offered to the remote desktop as you go back.

use std::sync::{Arc, Mutex};

use ironrdp_client::rdp::RdpInputEvent;
use ironrdp_cliprdr::backend::{ClipboardMessage, CliprdrBackend, CliprdrBackendFactory};
use ironrdp_cliprdr::pdu::{
    ClipboardFormat, ClipboardFormatId, ClipboardGeneralCapabilityFlags, FileContentsRequest, FileContentsResponse, FormatDataRequest,
    FormatDataResponse, LockDataId,
};
use ironrdp_core::IntoOwned as _;
use tokio::sync::mpsc;

/// Where the backend's messages go (the session's input channel, known once the engine has been made), and the text
/// last passed either way, so the same copy isn't offered back and forth
#[derive(Clone, Debug, Default)]
pub struct Shared {
    pub sender: Arc<Mutex<Option<mpsc::UnboundedSender<RdpInputEvent>>>>,
    last: Arc<Mutex<String>>,
}

impl Shared {
    /// Send a message to the clipboard channel, once the engine's there to take it
    fn send(&self, message: ClipboardMessage) {
        if let Some(sender) = self.sender.lock().unwrap().as_ref() {
            let _ = sender.send(RdpInputEvent::Clipboard(message));
        }
    }

    /// The tab got the keyboard, or ⌘V on the Mac: text copied on this computer since is offered to the remote
    /// desktop (it asks for it when pasted); whether there was any
    pub fn announce(&self) -> bool {
        let Some(text) = crate::clipboard::read() else { return false };
        let mut last = self.last.lock().unwrap();
        if text.is_empty() || *last == text {
            return false;
        }
        *last = text;
        drop(last);
        self.send(ClipboardMessage::SendInitiateCopy(vec![ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT)]));
        true
    }
}

/// Makes the backend for each connection (and reconnection)
pub struct Factory(pub Shared);

impl CliprdrBackendFactory for Factory {
    /// A backend for a new connection, sharing the channel and the text last passed
    fn build_cliprdr_backend(&self) -> Box<dyn CliprdrBackend> {
        Box::new(Backend { shared: self.0.clone(), started: std::time::Instant::now() })
    }
}

/// The clipboard channel's backend for one connection
#[derive(Debug)]
pub struct Backend {
    shared: Shared,
    /// When the channel started: what Windows' clipboard already held then isn't fetched
    started: std::time::Instant,
}

impl ironrdp_core::AsAny for Backend {
    /// Itself, as IronRDP asks every backend for
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    /// Itself, as IronRDP asks every backend for
    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

impl CliprdrBackend for Backend {
    /// Where IronRDP would keep files passed; files aren't passed, so it's never made
    fn temporary_directory(&self) -> &str {
        ".cliprdr"
    }

    /// Long format names, as Windows' own clients ask for
    fn client_capabilities(&self) -> ClipboardGeneralCapabilityFlags {
        ClipboardGeneralCapabilityFlags::USE_LONG_FORMAT_NAMES
    }

    /// Nothing to do when the channel is ready: Windows asks for the format list next
    fn on_ready(&mut self) {}

    /// The channel's start: what this computer's clipboard holds is offered
    fn on_request_format_list(&mut self) {
        let formats = if crate::clipboard::read().is_some_and(|t| !t.is_empty()) {
            vec![ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT)]
        } else {
            Vec::new()
        };
        self.shared.send(ClipboardMessage::SendInitiateCopy(formats));
    }

    /// Nothing to do with the capabilities agreed: only text is passed
    fn on_process_negotiated_capabilities(&mut self, _capabilities: ClipboardGeneralCapabilityFlags) {}

    /// Copied in the remote desktop: its text is fetched at once, for this computer's clipboard. Windows also
    /// announces what its clipboard holds as the session starts; that isn't a copy of the user's, so it doesn't
    /// replace this computer's clipboard
    fn on_remote_copy(&mut self, available_formats: &[ClipboardFormat]) {
        if self.started.elapsed() < std::time::Duration::from_secs(3) {
            return;
        }
        if available_formats.iter().any(|f| f.id == ClipboardFormatId::CF_UNICODETEXT) {
            self.shared.send(ClipboardMessage::SendInitiatePaste(ClipboardFormatId::CF_UNICODETEXT));
        }
    }

    /// Pasted in the remote desktop: this computer's text
    fn on_format_data_request(&mut self, request: FormatDataRequest) {
        let response = match crate::clipboard::read() {
            Some(text) if request.format == ClipboardFormatId::CF_UNICODETEXT => FormatDataResponse::new_unicode_string(&text),
            _ => FormatDataResponse::new_error(),
        };
        self.shared.send(ClipboardMessage::SendFormatData(response.into_owned()));
    }

    /// The remote desktop's text, onto this computer's clipboard
    fn on_format_data_response(&mut self, response: FormatDataResponse<'_>) {
        if response.is_error() {
            return;
        }
        if let Ok(text) = response.to_unicode_string() {
            let text = text.trim_end_matches('\0').to_string();
            *self.shared.last.lock().unwrap() = text.clone();
            crate::clipboard::write(&text);
        }
    }

    /// files aren't passed (text only, for now)
    fn on_file_contents_request(&mut self, _request: FileContentsRequest) {}
    /// Files aren't passed
    fn on_file_contents_response(&mut self, _response: FileContentsResponse<'_>) {}
    /// Nothing is locked, as files aren't passed
    fn on_lock(&mut self, _data_id: LockDataId) {}
    /// Nothing is locked, as files aren't passed
    fn on_unlock(&mut self, _data_id: LockDataId) {}
}

/// The Mac's general pasteboard, as text. Windows' clipboard is in [`crate::clipboard`], which both systems go through
#[cfg(target_os = "macos")]
pub(crate) mod mac {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::NSString;

    /// The text on the Mac's clipboard, if any
    pub fn read() -> Option<String> {
        let board = NSPasteboard::generalPasteboard();
        let text = unsafe { board.stringForType(NSPasteboardTypeString) }?;
        Some(text.to_string())
    }

    /// Put text on the Mac's clipboard, in place of what it held
    pub fn write(text: &str) {
        let board = NSPasteboard::generalPasteboard();
        board.clearContents();
        let _ = unsafe { board.setString_forType(&NSString::from_str(text), NSPasteboardTypeString) };
    }
}

#[cfg(test)]
mod tests {
    //! The backend driven as IronRDP's clipboard channel drives it, against this computer's real clipboard (put back
    //! after each test); the session's input channel is a channel the test reads.
    use super::*;

    /// A backend for a connection made `age` ago, and where its messages for the remote desktop arrive
    fn backend(age: std::time::Duration) -> (Backend, mpsc::UnboundedReceiver<RdpInputEvent>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let shared = Shared::default();
        *shared.sender.lock().unwrap() = Some(sender);
        (Backend { shared, started: std::time::Instant::now() - age }, receiver)
    }

    /// The backend's next message for the remote desktop
    fn message(receiver: &mut mpsc::UnboundedReceiver<RdpInputEvent>) -> ClipboardMessage {
        match receiver.try_recv() {
            Ok(RdpInputEvent::Clipboard(message)) => message,
            _ => panic!("the backend sent no clipboard message"),
        }
    }

    /// Windows to a Windows Server lost the clipboard both ways: the backend read and wrote only the Mac's. Only
    /// where there's a clipboard: the Mac and Windows
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn text_copied_on_this_computer_pastes_in_the_remote_desktop() {
        let _held = crate::clipboard::hold_for_test();
        let before = crate::clipboard::read().unwrap_or_default();
        crate::clipboard::write("show running-config – ünïcode");
        let copied = crate::clipboard::read_for_test("show running-config – ünïcode");
        let (mut backend, mut receiver) = backend(std::time::Duration::ZERO);

        backend.on_request_format_list();
        let offered = message(&mut receiver);
        backend.on_format_data_request(FormatDataRequest { format: ClipboardFormatId::CF_UNICODETEXT });
        let pasted = message(&mut receiver);
        crate::clipboard::write(&before);

        assert_eq!(copied, "show running-config – ünïcode");
        let ClipboardMessage::SendInitiateCopy(formats) = offered else { panic!("the text wasn't offered") };
        assert!(formats.iter().any(|f| f.id == ClipboardFormatId::CF_UNICODETEXT), "text is offered to the remote desktop");
        let ClipboardMessage::SendFormatData(response) = pasted else { panic!("the paste wasn't answered") };
        assert_eq!(response.to_unicode_string().unwrap().trim_end_matches('\0'), "show running-config – ünïcode");
    }

    /// The other way. Only where there's a clipboard: the Mac and Windows
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn text_copied_in_the_remote_desktop_is_on_this_computers_clipboard() {
        let _held = crate::clipboard::hold_for_test();
        let before = crate::clipboard::read().unwrap_or_default();
        let (mut backend, mut receiver) = backend(std::time::Duration::from_secs(10));

        backend.on_remote_copy(&[ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT)]);
        let asked = message(&mut receiver);
        backend.on_format_data_response(FormatDataResponse::new_unicode_string("GigabitEthernet1/0/1 – ünïcode"));
        let read = crate::clipboard::read_for_test("GigabitEthernet1/0/1 – ünïcode");
        crate::clipboard::write(&before);

        assert!(matches!(asked, ClipboardMessage::SendInitiatePaste(ClipboardFormatId::CF_UNICODETEXT)), "the copy's text is asked for");
        assert_eq!(read, "GigabitEthernet1/0/1 – ünïcode");
    }
}
