//! Windows backend: Media Foundation camera enumeration and capture (an `IMFSourceReader` per
//! open camera) and the camera privacy switches. All `unsafe` in the Windows build lives in this
//! folder, each block with a SAFETY comment; the logic that decides things (formats, modes, stamps)
//! is in `crate::convert` and tested without Windows.
//!
//! Linux: Video4Linux2 (`v4l2` for the ioctls, `linux` for the devices and the source,
//! `linux_writer` for the file), with the decisions in `crate::v4l_logic`. Any other system: `none`.

#[cfg(windows)]
mod consent;
#[cfg(windows)]
mod devices;
#[cfg(windows)]
mod mf;
#[cfg(windows)]
mod source;
#[cfg(windows)]
mod writer;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod linux_writer;
#[cfg(not(any(windows, target_os = "linux")))]
mod none;
#[cfg(target_os = "linux")]
mod v4l2;

#[cfg(target_os = "linux")]
pub use linux::{
    list_cameras, permission_status, request_access, system_source, system_writer_factory,
};
#[cfg(not(any(windows, target_os = "linux")))]
pub use none::{
    list_cameras, permission_status, request_access, system_source, system_writer_factory,
};

#[cfg(windows)]
pub use consent::permission_status;
#[cfg(windows)]
pub use devices::describe_native_modes as devices_describe_native_modes;
#[cfg(windows)]
pub use devices::{list_cameras, request_access};

#[cfg(windows)]
pub fn system_source() -> Box<dyn crate::types::CameraSource> {
    // A recording survives the camera being unplugged and plugged in again: the wrapper reopens
    // the same device with back-off (crate::reconnect, tested with fakes).
    Box::new(crate::reconnect::ReconnectingSource::new(
        Box::new(source::MfSource::default()),
        crate::reconnect::Backoff::default(),
    ))
}

#[cfg(windows)]
pub fn system_writer_factory() -> std::sync::Arc<dyn crate::writer::WriterFactory> {
    std::sync::Arc::new(writer::MfWriterFactory)
}
