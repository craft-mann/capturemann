//! The size of a window as the Windows capture API sees it. xcap lists the DWM frame of a window, but its
//! picture is the Windows.Graphics.Capture item, which was 7 px narrower and 2 px lower on a runner
//! (listed 754x512, image 747x510), so a window's listed bounds disagreed with what a capture returns.
//! The capture item's own size is the size of the picture, so bounds taken from it agree by construction.

use std::ffi::c_void;

/// The pixel size of the capture item for the window with this id (an `HWND`), or none when the OS refuses
/// to make one (a window that cannot be captured); the caller then keeps the size it has.
pub fn item_size(id: u32) -> Option<(u32, u32)> {
    use windows::core::factory;
    use windows::Graphics::Capture::GraphicsCaptureItem;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

    // The thread may not have a WinRT apartment yet. An error here means one exists (fine) or the mode differs (fine).
    // SAFETY: plain call, no pointers.
    let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>().ok()?;
    let hwnd = HWND(id as usize as *mut c_void);
    // SAFETY: a stale handle makes this return an error.
    let item: GraphicsCaptureItem = unsafe { interop.CreateForWindow(hwnd) }.ok()?;
    let size = item.Size().ok()?;
    (size.Width > 0 && size.Height > 0).then_some((size.Width as u32, size.Height as u32))
}
