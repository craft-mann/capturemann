//! Opening the system browser (never an embedded web view, RFC 8252).

use cm_core::CmError;

pub trait BrowserOpener: Send + Sync {
    fn open(&self, url: &str) -> Result<(), CmError>;
}

/// Opens the URL with the platform's default browser.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemBrowser;

impl BrowserOpener for SystemBrowser {
    fn open(&self, url: &str) -> Result<(), CmError> {
        // The URL carries `state` and the code challenge: keep it out of the message.
        open::that(url).map_err(|e| {
            CmError::internal(format!("could not open the system browser: {}", e.kind()))
        })
    }
}
