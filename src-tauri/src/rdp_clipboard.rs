//! Remote Desktop's clipboard on the Mac, as SuperXPuTTY's Clipboard option (RedirectClipboard): text copied in the
//! remote desktop is on the Mac's clipboard, and text copied on the Mac pastes in the remote desktop.
//!
//! IronRDP's clipboard channel (CLIPRDR) takes a backend; its client engine has one only for Windows (a do-nothing
//! stub elsewhere), so this one is attached as the session's own channel (`with_static_channel`), replacing it. Its
//! messages go back through the engine's input channel, as the Windows backend's do. The Mac's clipboard is read
//! when the tab gets the keyboard (`announce`), so a copy made in another app is offered to Windows as you go back.

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
    fn send(&self, message: ClipboardMessage) {
        if let Some(sender) = self.sender.lock().unwrap().as_ref() {
            let _ = sender.send(RdpInputEvent::Clipboard(message));
        }
    }

    /// The tab got the keyboard: text copied on the Mac since is offered to Windows (it asks for it when pasted)
    pub fn announce(&self) {
        let Some(text) = mac::read() else { return };
        let mut last = self.last.lock().unwrap();
        if text.is_empty() || *last == text {
            return;
        }
        *last = text;
        drop(last);
        self.send(ClipboardMessage::SendInitiateCopy(vec![ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT)]));
    }
}

/// Makes the backend for each connection (and reconnection)
pub struct Factory(pub Shared);

impl CliprdrBackendFactory for Factory {
    fn build_cliprdr_backend(&self) -> Box<dyn CliprdrBackend> {
        Box::new(Backend { shared: self.0.clone() })
    }
}

#[derive(Debug)]
pub struct Backend {
    shared: Shared,
}

impl ironrdp_core::AsAny for Backend {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

impl CliprdrBackend for Backend {
    fn temporary_directory(&self) -> &str {
        ".cliprdr"
    }

    fn client_capabilities(&self) -> ClipboardGeneralCapabilityFlags {
        ClipboardGeneralCapabilityFlags::USE_LONG_FORMAT_NAMES
    }

    fn on_ready(&mut self) {}

    /// The channel's start: what the Mac's clipboard holds is offered
    fn on_request_format_list(&mut self) {
        let formats = if mac::read().is_some_and(|t| !t.is_empty()) {
            vec![ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT)]
        } else {
            Vec::new()
        };
        self.shared.send(ClipboardMessage::SendInitiateCopy(formats));
    }

    fn on_process_negotiated_capabilities(&mut self, _capabilities: ClipboardGeneralCapabilityFlags) {}

    /// Copied in the remote desktop: its text is fetched at once, for the Mac's clipboard
    fn on_remote_copy(&mut self, available_formats: &[ClipboardFormat]) {
        if available_formats.iter().any(|f| f.id == ClipboardFormatId::CF_UNICODETEXT) {
            self.shared.send(ClipboardMessage::SendInitiatePaste(ClipboardFormatId::CF_UNICODETEXT));
        }
    }

    /// Pasted in the remote desktop: the Mac's text
    fn on_format_data_request(&mut self, request: FormatDataRequest) {
        let response = match mac::read() {
            Some(text) if request.format == ClipboardFormatId::CF_UNICODETEXT => FormatDataResponse::new_unicode_string(&text),
            _ => FormatDataResponse::new_error(),
        };
        self.shared.send(ClipboardMessage::SendFormatData(response.into_owned()));
    }

    /// The remote desktop's text, onto the Mac's clipboard
    fn on_format_data_response(&mut self, response: FormatDataResponse<'_>) {
        if response.is_error() {
            return;
        }
        if let Ok(text) = response.to_unicode_string() {
            let text = text.trim_end_matches('\0').to_string();
            *self.shared.last.lock().unwrap() = text.clone();
            mac::write(&text);
        }
    }

    // files aren't passed (text only, for now)
    fn on_file_contents_request(&mut self, _request: FileContentsRequest) {}
    fn on_file_contents_response(&mut self, _response: FileContentsResponse<'_>) {}
    fn on_lock(&mut self, _data_id: LockDataId) {}
    fn on_unlock(&mut self, _data_id: LockDataId) {}
}

/// The Mac's general pasteboard, as text
#[cfg(target_os = "macos")]
mod mac {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::NSString;

    pub fn read() -> Option<String> {
        let board = NSPasteboard::generalPasteboard();
        let text = unsafe { board.stringForType(NSPasteboardTypeString) }?;
        Some(text.to_string())
    }

    pub fn write(text: &str) {
        let board = NSPasteboard::generalPasteboard();
        board.clearContents();
        let _ = unsafe { board.setString_forType(&NSString::from_str(text), NSPasteboardTypeString) };
    }
}

#[cfg(not(target_os = "macos"))]
mod mac {
    pub fn read() -> Option<String> {
        None
    }

    pub fn write(_text: &str) {}
}
