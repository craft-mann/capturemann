//! macOS microphone and screen-recording authorization via AVFoundation / CoreGraphics.

use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

use crate::permission::PermissionState;

fn map(s: AVAuthorizationStatus) -> PermissionState {
    if s == AVAuthorizationStatus::Authorized {
        PermissionState::Authorized
    } else if s == AVAuthorizationStatus::Denied {
        PermissionState::Denied
    } else if s == AVAuthorizationStatus::Restricted {
        PermissionState::Restricted
    } else {
        PermissionState::NotDetermined
    }
}

/// Reads the microphone authorization status. Never shows a prompt.
pub fn microphone_status() -> PermissionState {
    // SAFETY: `AVMediaTypeAudio` is a constant NSString exported by AVFoundation; the class method
    // takes no other input and only reads TCC state.
    let status = unsafe {
        match AVMediaTypeAudio {
            Some(t) => AVCaptureDevice::authorizationStatusForMediaType(t),
            None => return PermissionState::NotDetermined,
        }
    };
    map(status)
}

/// Shows the system prompt if the status is not determined and waits for the answer.
pub fn request_microphone(timeout: Duration) -> PermissionState {
    let current = microphone_status();
    if current != PermissionState::NotDetermined {
        return current;
    }
    let (tx, rx) = mpsc::channel::<bool>();
    let block = RcBlock::new(move |granted: Bool| {
        let _ = tx.send(granted.as_bool());
    });
    // SAFETY: the block is retained by AVFoundation until it runs (RcBlock is reference counted)
    // and only sends on a channel, which is `Send`. The media type constant is valid.
    unsafe {
        if let Some(t) = AVMediaTypeAudio {
            AVCaptureDevice::requestAccessForMediaType_completionHandler(t, &block);
        }
    }
    match rx.recv_timeout(timeout) {
        Ok(true) => PermissionState::Authorized,
        Ok(false) => PermissionState::Denied,
        Err(_) => microphone_status(),
    }
}

/// Screen Recording access (needed by ScreenCaptureKit audio). Never shows a prompt.
pub fn screen_recording_granted() -> bool {
    objc2_core_graphics::CGPreflightScreenCaptureAccess()
}
