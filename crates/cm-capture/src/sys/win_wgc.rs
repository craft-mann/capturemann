//! Windows.Graphics.Capture: frames of one display or one window as CPU memory (BGRA), stamped with
//! the system time WGC reports. The screen source (`record/windows.rs`) turns them into the
//! recorder's events.
//!
//! One Direct3D 11 device per capture (hardware, WARP when there is no GPU). WGC delivers each
//! frame as a GPU texture on a thread-pool thread; the handler copies the wanted region into a
//! staging texture, maps it, copies the rows out and closes the frame at once, so the small frame
//! pool never runs dry. A resized window or a changed display mode makes the pool follow the new
//! content size (the encoder scales to the planned size). Cursor drawing is WGC's own.
//!
//! Everything touching the device context happens under one mutex, which `stop` also takes, so no
//! event reaches the sink after `stop` returns. Free-function helpers that need no Windows API are
//! in `record/wgc_logic.rs` and are tested on every OS.

use std::ffi::c_void;
use std::sync::{Arc, Mutex};

use cm_core::{CmError, PhysicalRect};
use cm_encode::{CpuFrame, PixelFormat};
use windows::core::{factory, IInspectable, Interface};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{HMODULE, HWND, RECT};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_BOX,
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
    D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::HMONITOR;
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

use crate::record::wgc_logic::{clamp_crop, copy_rows};
use crate::sys::win_names::crop_box;

const FORMAT: DirectXPixelFormat = DirectXPixelFormat::B8G8R8A8UIntNormalized;
/// Buffers in the frame pool: one being copied, one being filled, one spare.
const POOL_BUFFERS: i32 = 3;

/// What to capture. The ids are what `list_targets` reports (xcap: the `HMONITOR` and `HWND`
/// values as `u32`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WgcTarget {
    Monitor(u32),
    Window(u32),
}

/// What a capture reports.
pub enum WgcEvent {
    /// A frame; `stamp_100ns` is WGC's `SystemRelativeTime`.
    Frame { stamp_100ns: i64, frame: CpuFrame },
    /// The window or display went away.
    Closed,
    /// Grabbing a frame failed (the device was lost, say).
    Failed(String),
}

pub type WgcSink = Box<dyn FnMut(WgcEvent) + Send>;

/// COM interfaces here are used only under the capture's mutex, one thread at a time; the
/// device and its context are free-threaded objects, so moving them between threads is sound.
struct Gpu {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    winrt_device: IDirect3DDevice,
}

// SAFETY: see the type's comment: single use at a time under `Inner`'s mutex.
unsafe impl Send for Gpu {}

struct Staging {
    texture: ID3D11Texture2D,
    width: u32,
    height: u32,
    format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT,
}

// SAFETY: as for `Gpu`.
unsafe impl Send for Staging {}

/// A content size and the visible frame inside it, `(x, y, width, height)`, if there is one.
type VisibleBox = Option<(u32, u32, u32, u32)>;
type WindowBox = (u32, u32, VisibleBox);

struct Inner {
    gpu: Gpu,
    staging: Option<Staging>,
    pool_size: SizeInt32,
    sink: Option<WgcSink>,
    target: WgcTarget,
    crop: Option<PhysicalRect>,
    /// For a window: the visible frame inside the captured rectangle, per content size.
    visible: Option<WindowBox>,
}

/// A running capture. Dropping it stops it.
pub struct WgcCapture {
    inner: Arc<Mutex<Inner>>,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    item: GraphicsCaptureItem,
    arrived: i64,
    closed: i64,
    /// Set by the first `stop`: touching a closed pool or session again (the `Drop` after an explicit stop)
    /// ends the process with a fast-fail (0xC0000409).
    stopped: bool,
}

fn err(what: &str, e: windows::core::Error) -> CmError {
    CmError::platform_rejected(format!("Windows.Graphics.Capture: {what}: {e}"), None)
}

fn make_gpu() -> Result<Gpu, CmError> {
    let create = |driver| {
        let mut device = None;
        let mut context = None;
        // SAFETY: the out parameters are live locals; no adapter, no feature-level list.
        unsafe {
            D3D11CreateDevice(
                None,
                driver,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
        }
        .map(|()| (device, context))
    };
    // No GPU (a virtual machine): WARP still captures, and the encoder then says it has no
    // hardware to encode with.
    let (device, context) = create(D3D_DRIVER_TYPE_HARDWARE)
        .or_else(|_| create(D3D_DRIVER_TYPE_WARP))
        .map_err(|e| err("Direct3D 11 device", e))?;
    let (Some(device), Some(context)) = (device, context) else {
        return Err(CmError::platform_rejected(
            "Windows.Graphics.Capture: no Direct3D 11 device",
            None,
        ));
    };
    let dxgi = device
        .cast::<IDXGIDevice>()
        .map_err(|e| err("DXGI device", e))?;
    // SAFETY: wraps the live DXGI device for WinRT.
    let winrt = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }
        .map_err(|e| err("WinRT device", e))?;
    let winrt_device = winrt
        .cast::<IDirect3DDevice>()
        .map_err(|e| err("WinRT device", e))?;
    Ok(Gpu {
        device,
        context,
        winrt_device,
    })
}

impl Inner {
    /// The rectangle of the content to keep, in content px.
    fn region(&mut self, w: u32, h: u32) -> (u32, u32, u32, u32) {
        if let WgcTarget::Window(id) = self.target {
            if self.visible.as_ref().is_none_or(|v| (v.0, v.1) != (w, h)) {
                self.visible = Some((w, h, window_visible_box(id, w, h)));
            }
            if let Some((_, _, Some(b))) = self.visible {
                return b;
            }
        }
        match clamp_crop(self.crop, w, h) {
            Some(r) => (r.x, r.y, r.w, r.h),
            None => (0, 0, w, h),
        }
    }

    fn staging_for(
        &mut self,
        w: u32,
        h: u32,
        src: &D3D11_TEXTURE2D_DESC,
    ) -> Result<&Staging, CmError> {
        let fits = self
            .staging
            .as_ref()
            .is_some_and(|s| (s.width, s.height, s.format) == (w, h, src.Format));
        if !fits {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: w,
                Height: h,
                MipLevels: 1,
                ArraySize: 1,
                Format: src.Format,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
            };
            let mut texture = None;
            // SAFETY: `desc` and the out parameter are live locals.
            unsafe {
                self.gpu
                    .device
                    .CreateTexture2D(&desc, None, Some(&mut texture))
            }
            .map_err(|e| err("staging texture", e))?;
            let texture = texture.ok_or_else(|| {
                CmError::platform_rejected("Windows.Graphics.Capture: no staging texture", None)
            })?;
            self.staging = Some(Staging {
                texture,
                width: w,
                height: h,
                format: src.Format,
            });
        }
        self.staging
            .as_ref()
            .ok_or_else(|| CmError::internal("no staging texture"))
    }

    /// Takes the next frame out of the pool and copies its pixels.
    fn grab(
        &mut self,
        pool: &Direct3D11CaptureFramePool,
    ) -> Result<Option<(i64, CpuFrame)>, CmError> {
        let frame = match pool.TryGetNextFrame() {
            Ok(f) => f,
            Err(_) => return Ok(None), // nothing there: a spurious event
        };
        let result = self.copy_frame(pool, &frame);
        let _ = frame.Close();
        result.map(Some)
    }

    fn copy_frame(
        &mut self,
        pool: &Direct3D11CaptureFramePool,
        frame: &windows::Graphics::Capture::Direct3D11CaptureFrame,
    ) -> Result<(i64, CpuFrame), CmError> {
        let stamp = frame
            .SystemRelativeTime()
            .map_err(|e| err("frame time", e))?
            .Duration;
        let size = frame.ContentSize().map_err(|e| err("content size", e))?;
        if (size.Width, size.Height) != (self.pool_size.Width, self.pool_size.Height) {
            // The window was resized or the display mode changed: later frames follow.
            if pool
                .Recreate(&self.gpu.winrt_device, FORMAT, POOL_BUFFERS, size)
                .is_ok()
            {
                self.pool_size = size;
            }
        }
        let (cw, ch) = (size.Width.max(1) as u32, size.Height.max(1) as u32);
        let (x, y, w, h) = self.region(cw, ch);
        let surface = frame.Surface().map_err(|e| err("frame surface", e))?;
        let access = surface
            .cast::<IDirect3DDxgiInterfaceAccess>()
            .map_err(|e| err("frame surface", e))?;
        // SAFETY: the surface of a capture frame is a Direct3D 11 texture.
        let texture: ID3D11Texture2D =
            unsafe { access.GetInterface() }.map_err(|e| err("frame texture", e))?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: writes the description into a live local.
        unsafe { texture.GetDesc(&mut desc) };
        let (w, h) = (
            w.min(desc.Width.saturating_sub(x)),
            h.min(desc.Height.saturating_sub(y)),
        );
        if w == 0 || h == 0 {
            return Err(CmError::internal("the captured frame is empty"));
        }
        let context = self.gpu.context.clone();
        let staging = self.staging_for(w, h, &desc)?.texture.clone();
        let region = D3D11_BOX {
            left: x,
            top: y,
            front: 0,
            right: x + w,
            bottom: y + h,
            back: 1,
        };
        // SAFETY: both textures are live; the box lies inside the source and the destination is
        // `w` x `h`. The map / unmap pair brackets the one copy out of the mapped memory.
        let rows = unsafe {
            context.CopySubresourceRegion(&staging, 0, 0, 0, 0, &texture, 0, Some(&region));
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .map_err(|e| err("map the frame", e))?;
            let pitch = mapped.RowPitch as usize;
            let mem = std::slice::from_raw_parts(mapped.pData as *const u8, pitch * h as usize);
            let rows = copy_rows(mem, pitch, 0, 0, w as usize, h as usize);
            context.Unmap(&staging, 0);
            rows
        }
        .ok_or_else(|| CmError::internal("the mapped frame is shorter than its rows"))?;
        let pixels = CpuFrame::new(w, h, w as usize * 4, PixelFormat::Bgra, rows)?;
        Ok((stamp, pixels))
    }
}

/// For a window: the visible frame (without the invisible resize border) inside the captured
/// `w` x `h` rectangle, when that rectangle is the window rectangle; `None` keeps the whole
/// capture (the encoder then scales it to the planned size).
fn window_visible_box(id: u32, w: u32, h: u32) -> VisibleBox {
    let hwnd = HWND(id as usize as *mut c_void);
    let (mut outer, mut frame) = (RECT::default(), RECT::default());
    // SAFETY: both calls only write into the RECTs; a stale handle makes them return an error.
    let ok = unsafe {
        GetWindowRect(hwnd, &mut outer).is_ok()
            && DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut frame as *mut RECT as *mut c_void,
                std::mem::size_of::<RECT>() as u32,
            )
            .is_ok()
    };
    if !ok || (outer.right - outer.left, outer.bottom - outer.top) != (w as i32, h as i32) {
        return None;
    }
    let (x, y, bw, bh) = crop_box(
        (outer.left, outer.top, outer.right, outer.bottom),
        (frame.left, frame.top, frame.right, frame.bottom),
    );
    (bw > 0 && bh > 0).then_some((x as u32, y as u32, bw as u32, bh as u32))
}

impl WgcCapture {
    /// Whether this Windows can capture with WGC at all (Windows 10 1903 and later).
    pub fn supported() -> bool {
        GraphicsCaptureSession::IsSupported().unwrap_or(false)
    }

    /// Starts delivering frames of `target` to `sink`.
    pub fn start(
        target: WgcTarget,
        show_cursor: bool,
        crop: Option<PhysicalRect>,
        sink: WgcSink,
    ) -> Result<Self, CmError> {
        if !Self::supported() {
            return Err(CmError::platform_rejected(
                "Screen recording needs Windows 10 version 1903 or later",
                None,
            ));
        }
        // SAFETY: initialises the Windows Runtime for this thread; "already initialised" and
        // "other apartment" answers are fine, the runtime is usable either way.
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
            .map_err(|e| err("capture item factory", e))?;
        // SAFETY: the handle values come from `list_targets`; a stale one makes the call fail.
        let item: GraphicsCaptureItem = unsafe {
            match target {
                WgcTarget::Monitor(id) => {
                    interop.CreateForMonitor(HMONITOR(id as usize as *mut c_void))
                }
                WgcTarget::Window(id) => interop.CreateForWindow(HWND(id as usize as *mut c_void)),
            }
        }
        .map_err(|e| {
            CmError::not_found(format!(
                "The {} is not available for capture ({e})",
                match target {
                    WgcTarget::Monitor(_) => "display",
                    WgcTarget::Window(_) => "window",
                }
            ))
        })?;
        let size = item.Size().map_err(|e| err("capture item size", e))?;
        let gpu = make_gpu()?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &gpu.winrt_device,
            FORMAT,
            POOL_BUFFERS,
            size,
        )
        .map_err(|e| err("frame pool", e))?;
        let inner = Arc::new(Mutex::new(Inner {
            gpu,
            staging: None,
            pool_size: size,
            sink: Some(sink),
            target,
            crop,
            visible: None,
        }));

        let on_frame = Arc::clone(&inner);
        let arrived = pool
            .FrameArrived(
                &TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new(
                    move |pool, _| {
                        let Some(pool) = pool.as_ref() else {
                            return Ok(());
                        };
                        let mut g = on_frame.lock().unwrap_or_else(|e| e.into_inner());
                        if g.sink.is_none() {
                            return Ok(()); // stopped
                        }
                        let got = g.grab(pool);
                        let Some(sink) = g.sink.as_mut() else {
                            return Ok(());
                        };
                        match got {
                            Ok(Some((stamp_100ns, frame))) => {
                                sink(WgcEvent::Frame { stamp_100ns, frame })
                            }
                            Ok(None) => {}
                            Err(e) => {
                                log::warn!("capture frame failed: {e}");
                                sink(WgcEvent::Failed(e.to_string()));
                            }
                        }
                        Ok(())
                    },
                ),
            )
            .map_err(|e| err("frame handler", e))?;
        let on_close = Arc::clone(&inner);
        let closed = item
            .Closed(
                &TypedEventHandler::<GraphicsCaptureItem, IInspectable>::new(move |_, _| {
                    let mut g = on_close.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(sink) = g.sink.as_mut() {
                        sink(WgcEvent::Closed);
                    }
                    Ok(())
                }),
            )
            .map_err(|e| err("close handler", e))?;

        let session = pool
            .CreateCaptureSession(&item)
            .map_err(|e| err("capture session", e))?;
        // Best effort: the cursor setting needs Windows 10 2004, the border switch Windows 11.
        if let Err(e) = session.SetIsCursorCaptureEnabled(show_cursor) {
            log::debug!("cursor capture setting not applied: {e}");
        }
        if let Err(e) = session.SetIsBorderRequired(false) {
            log::debug!("capture border setting not applied: {e}");
        }
        session.StartCapture().map_err(|e| err("start", e))?;
        Ok(WgcCapture {
            inner,
            pool,
            session,
            item,
            arrived,
            closed,
            stopped: false,
        })
    }

    /// Stops the capture. When this returns no event is delivered any more.
    pub fn stop(&mut self) {
        if std::mem::replace(&mut self.stopped, true) {
            return;
        }
        log::info!("wgc stop: removing the handlers");
        let _ = self.pool.RemoveFrameArrived(self.arrived);
        let _ = self.item.RemoveClosed(self.closed);
        log::info!("wgc stop: closing the session");
        let _ = self.session.Close();
        log::info!("wgc stop: closing the pool");
        let _ = self.pool.Close();
        log::info!("wgc stop: waiting for the last frame");
        // A handler that is mid-frame holds the lock; taking it waits for that frame, and
        // clearing the sink means no later handler call can reach the owner.
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.sink = None;
        g.staging = None;
        log::info!("wgc stop: done");
    }
}

impl Drop for WgcCapture {
    fn drop(&mut self) {
        self.stop();
    }
}
