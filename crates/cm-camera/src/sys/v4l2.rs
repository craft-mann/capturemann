//! Video4Linux2 through plain `ioctl` calls on a device node (`/dev/videoN`): querying the
//! driver, listing formats, sizes and frame intervals, setting a format and rate, and streaming
//! memory-mapped buffers. The struct layouts are the kernel's (`linux/videodev2.h`) for 64-bit
//! systems, with their sizes checked in the tests, so no bindgen or libv4l is needed. All `unsafe`
//! of the Linux camera backend is in this file; what to do with the answers is decided in
//! `crate::v4l_logic`.

use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::Path;
use std::ptr::NonNull;

const TYPE_VIDEO_CAPTURE: u32 = 1;
const MEMORY_MMAP: u32 = 1;
const FRMSIZE_DISCRETE: u32 = 1;
const FRMSIZE_STEPWISE: u32 = 3;
const FRMIVAL_DISCRETE: u32 = 1;
const FIELD_NONE: u32 = 1;
const BUF_FLAG_ERROR: u32 = 0x40;

const fn ioc(dir: u32, nr: u32, size: usize) -> libc::c_ulong {
    ((dir << 30) | ((size as u32) << 16) | ((b'V' as u32) << 8) | nr) as libc::c_ulong
}
const W: u32 = 1;
const R: u32 = 2;

#[repr(C)]
#[derive(Default)]
struct Capability {
    driver: [u8; 16],
    card: [u8; 32],
    bus_info: [u8; 32],
    version: u32,
    capabilities: u32,
    device_caps: u32,
    reserved: [u32; 3],
}

#[repr(C)]
#[derive(Default)]
struct FmtDesc {
    index: u32,
    type_: u32,
    flags: u32,
    description: [u8; 32],
    pixelformat: u32,
    mbus_code: u32,
    reserved: [u32; 3],
}

/// `struct v4l2_format`: the type, then a union that holds pointers and so starts at offset 8.
#[repr(C)]
struct Format {
    type_: u32,
    fmt: FormatUnion,
}

#[repr(C, align(8))]
struct FormatUnion {
    raw: [u8; 200],
}

impl Format {
    fn new() -> Self {
        Format {
            type_: TYPE_VIDEO_CAPTURE,
            fmt: FormatUnion { raw: [0; 200] },
        }
    }
    fn put(&mut self, offset: usize, v: u32) {
        self.fmt.raw[offset..offset + 4].copy_from_slice(&v.to_ne_bytes());
    }
    fn get(&self, offset: usize) -> u32 {
        let mut b = [0u8; 4];
        b.copy_from_slice(&self.fmt.raw[offset..offset + 4]);
        u32::from_ne_bytes(b)
    }
}

// Offsets inside `struct v4l2_pix_format`.
const PIX_WIDTH: usize = 0;
const PIX_HEIGHT: usize = 4;
const PIX_FORMAT: usize = 8;
const PIX_FIELD: usize = 12;
const PIX_BYTESPERLINE: usize = 16;

#[repr(C)]
#[derive(Default)]
struct RequestBuffers {
    count: u32,
    type_: u32,
    memory: u32,
    capabilities: u32,
    flags: u8,
    reserved: [u8; 3],
}

#[repr(C)]
#[derive(Default)]
struct Buffer {
    index: u32,
    type_: u32,
    bytesused: u32,
    flags: u32,
    field: u32,
    timestamp_sec: i64,
    timestamp_usec: i64,
    timecode: [u8; 16],
    sequence: u32,
    memory: u32,
    offset: u64,
    length: u32,
    reserved2: u32,
    request_fd: u32,
}

#[repr(C)]
struct StreamParm {
    type_: u32,
    // capability, capturemode, timeperframe {numerator, denominator}, extendedmode, readbuffers,
    // reserved[4], padded to the union's 200 bytes.
    parm: [u32; 50],
}

const PARM_TIMEPERFRAME: usize = 2;

#[repr(C)]
#[derive(Default)]
struct FrmSizeEnum {
    index: u32,
    pixel_format: u32,
    type_: u32,
    // discrete: width, height; stepwise: min_w, max_w, step_w, min_h, max_h, step_h.
    size: [u32; 6],
    reserved: [u32; 2],
}

#[repr(C)]
#[derive(Default)]
struct FrmIvalEnum {
    index: u32,
    pixel_format: u32,
    width: u32,
    height: u32,
    type_: u32,
    // discrete: numerator, denominator; stepwise: min, max, step fractions.
    interval: [u32; 6],
    reserved: [u32; 2],
}

const VIDIOC_QUERYCAP: libc::c_ulong = ioc(R, 0, std::mem::size_of::<Capability>());
const VIDIOC_ENUM_FMT: libc::c_ulong = ioc(R | W, 2, std::mem::size_of::<FmtDesc>());
const VIDIOC_S_FMT: libc::c_ulong = ioc(R | W, 5, std::mem::size_of::<Format>());
const VIDIOC_REQBUFS: libc::c_ulong = ioc(R | W, 8, std::mem::size_of::<RequestBuffers>());
const VIDIOC_QUERYBUF: libc::c_ulong = ioc(R | W, 9, std::mem::size_of::<Buffer>());
const VIDIOC_QBUF: libc::c_ulong = ioc(R | W, 15, std::mem::size_of::<Buffer>());
const VIDIOC_DQBUF: libc::c_ulong = ioc(R | W, 17, std::mem::size_of::<Buffer>());
const VIDIOC_STREAMON: libc::c_ulong = ioc(W, 18, std::mem::size_of::<i32>());
const VIDIOC_STREAMOFF: libc::c_ulong = ioc(W, 19, std::mem::size_of::<i32>());
const VIDIOC_S_PARM: libc::c_ulong = ioc(R | W, 22, std::mem::size_of::<StreamParm>());
const VIDIOC_ENUM_FRAMESIZES: libc::c_ulong = ioc(R | W, 74, std::mem::size_of::<FrmSizeEnum>());
const VIDIOC_ENUM_FRAMEINTERVALS: libc::c_ulong =
    ioc(R | W, 75, std::mem::size_of::<FrmIvalEnum>());

fn ioctl<T>(fd: RawFd, request: libc::c_ulong, arg: &mut T) -> io::Result<()> {
    loop {
        // SAFETY: `arg` is a live, exclusively borrowed struct of the type `request` was built
        // for (its size is in the request number); the kernel reads and writes within it.
        let r = unsafe { libc::ioctl(fd, request as _, std::ptr::from_mut(arg)) };
        if r >= 0 {
            return Ok(());
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::Interrupted {
            return Err(e);
        }
    }
}

fn text(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

/// What a driver says about a node.
#[derive(Debug, Clone)]
pub struct Caps {
    pub card: String,
    pub bus_info: String,
    pub capabilities: u32,
    pub device_caps: u32,
}

/// What the driver settled on for a format request.
#[derive(Debug, Clone, Copy)]
pub struct Negotiated {
    pub width: u32,
    pub height: u32,
    pub fourcc: u32,
    pub bytes_per_line: u32,
}

/// An open video node.
pub struct Device {
    fd: OwnedFd,
}

impl Device {
    /// Opens `path` for reading frames, without blocking.
    pub fn open(path: &Path) -> io::Result<Device> {
        let c = CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        // SAFETY: `c` is a NUL-terminated path; the flags are plain constants.
        let fd = unsafe {
            libc::open(
                c.as_ptr(),
                libc::O_RDWR | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `fd` was just opened and is owned by nobody else.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        Ok(Device { fd })
    }

    fn raw(&self) -> RawFd {
        self.fd.as_raw_fd()
    }

    pub fn caps(&self) -> io::Result<Caps> {
        let mut c = Capability::default();
        ioctl(self.raw(), VIDIOC_QUERYCAP, &mut c)?;
        Ok(Caps {
            card: text(&c.card),
            bus_info: text(&c.bus_info),
            capabilities: c.capabilities,
            device_caps: c.device_caps,
        })
    }

    /// The pixel formats of the node (four-character codes).
    pub fn formats(&self) -> Vec<u32> {
        let mut out = Vec::new();
        for index in 0..64 {
            let mut d = FmtDesc {
                index,
                type_: TYPE_VIDEO_CAPTURE,
                ..FmtDesc::default()
            };
            if ioctl(self.raw(), VIDIOC_ENUM_FMT, &mut d).is_err() {
                break;
            }
            out.push(d.pixelformat);
        }
        out
    }

    /// The discrete sizes of `fourcc`, or, for a stepwise range, `(min, max, step)` of it.
    pub fn sizes(&self, fourcc: u32) -> Sizes {
        let mut discrete = Vec::new();
        for index in 0..256 {
            let mut s = FrmSizeEnum {
                index,
                pixel_format: fourcc,
                ..FrmSizeEnum::default()
            };
            if ioctl(self.raw(), VIDIOC_ENUM_FRAMESIZES, &mut s).is_err() {
                break;
            }
            match s.type_ {
                FRMSIZE_DISCRETE => discrete.push((s.size[0], s.size[1])),
                // Continuous (2) and stepwise (3) report one range, for index 0 only.
                t => {
                    let step = if t == FRMSIZE_STEPWISE {
                        (s.size[2], s.size[5])
                    } else {
                        (1, 1)
                    };
                    return Sizes::Range {
                        min: (s.size[0], s.size[3]),
                        max: (s.size[1], s.size[4]),
                        step,
                    };
                }
            }
        }
        Sizes::Discrete(discrete)
    }

    /// The frame rates offered for a size (empty when the driver lists none, or only a range,
    /// which is reported as its two ends).
    pub fn fps_list(&self, fourcc: u32, width: u32, height: u32) -> Vec<f64> {
        let mut out = Vec::new();
        for index in 0..256 {
            let mut v = FrmIvalEnum {
                index,
                pixel_format: fourcc,
                width,
                height,
                ..FrmIvalEnum::default()
            };
            if ioctl(self.raw(), VIDIOC_ENUM_FRAMEINTERVALS, &mut v).is_err() {
                break;
            }
            if v.type_ == FRMIVAL_DISCRETE {
                out.extend(crate::v4l_logic::fps_of(v.interval[0], v.interval[1]));
            } else {
                out.extend(crate::v4l_logic::fps_of(v.interval[0], v.interval[1]));
                out.extend(crate::v4l_logic::fps_of(v.interval[2], v.interval[3]));
                break;
            }
        }
        out
    }

    pub fn set_format(&self, fourcc: u32, width: u32, height: u32) -> io::Result<Negotiated> {
        let mut f = Format::new();
        f.put(PIX_WIDTH, width);
        f.put(PIX_HEIGHT, height);
        f.put(PIX_FORMAT, fourcc);
        f.put(PIX_FIELD, FIELD_NONE);
        ioctl(self.raw(), VIDIOC_S_FMT, &mut f)?;
        Ok(Negotiated {
            width: f.get(PIX_WIDTH),
            height: f.get(PIX_HEIGHT),
            fourcc: f.get(PIX_FORMAT),
            bytes_per_line: f.get(PIX_BYTESPERLINE),
        })
    }

    /// Asks for `fps` frames per second; returns the rate the driver accepted.
    pub fn set_fps(&self, fps: u32) -> io::Result<f64> {
        let mut p = StreamParm {
            type_: TYPE_VIDEO_CAPTURE,
            parm: [0; 50],
        };
        p.parm[PARM_TIMEPERFRAME] = 1;
        p.parm[PARM_TIMEPERFRAME + 1] = fps.max(1);
        ioctl(self.raw(), VIDIOC_S_PARM, &mut p)?;
        Ok(
            crate::v4l_logic::fps_of(p.parm[PARM_TIMEPERFRAME], p.parm[PARM_TIMEPERFRAME + 1])
                .unwrap_or(f64::from(fps)),
        )
    }

    /// Maps `count` buffers and queues them. `EBUSY` here means another program is capturing.
    pub fn start(self, count: u32) -> io::Result<Stream> {
        let mut rb = RequestBuffers {
            count,
            type_: TYPE_VIDEO_CAPTURE,
            memory: MEMORY_MMAP,
            ..RequestBuffers::default()
        };
        ioctl(self.raw(), VIDIOC_REQBUFS, &mut rb)?;
        if rb.count < 2 {
            return Err(io::Error::other("the driver gave fewer than two buffers"));
        }
        let mut stream = Stream {
            dev: self,
            maps: Vec::new(),
            on: false,
        };
        for index in 0..rb.count {
            let mut b = Buffer {
                index,
                type_: TYPE_VIDEO_CAPTURE,
                memory: MEMORY_MMAP,
                ..Buffer::default()
            };
            ioctl(stream.dev.raw(), VIDIOC_QUERYBUF, &mut b)?;
            // SAFETY: maps `length` bytes of the driver's buffer at its `offset`, shared with the
            // driver; unmapped in `Drop`.
            let p = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    b.length as usize,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    stream.dev.raw(),
                    b.offset as libc::off_t,
                )
            };
            if p == libc::MAP_FAILED {
                return Err(io::Error::last_os_error());
            }
            let ptr = NonNull::new(p.cast::<u8>()).ok_or_else(io::Error::last_os_error)?;
            stream.maps.push((ptr, b.length as usize));
            ioctl(stream.dev.raw(), VIDIOC_QBUF, &mut b)?;
        }
        let mut t = TYPE_VIDEO_CAPTURE as i32;
        ioctl(stream.dev.raw(), VIDIOC_STREAMON, &mut t)?;
        stream.on = true;
        Ok(stream)
    }
}

/// The sizes a format offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sizes {
    Discrete(Vec<(u32, u32)>),
    Range {
        min: (u32, u32),
        max: (u32, u32),
        step: (u32, u32),
    },
}

/// One picture taken off the queue; give its index back with [`Stream::requeue`].
#[derive(Debug, Clone, Copy)]
pub struct Dequeued {
    pub index: u32,
    pub bytes_used: usize,
    pub timestamp_sec: i64,
    pub timestamp_usec: i64,
    pub flags: u32,
    /// The driver flagged the picture as damaged.
    pub error: bool,
}

/// What waiting for a picture found.
pub enum Next {
    Frame(Dequeued),
    /// Nothing within the timeout.
    Idle,
    /// The device is gone (unplugged, or the driver reset).
    Gone,
}

/// A running stream with its mapped buffers.
pub struct Stream {
    dev: Device,
    maps: Vec<(NonNull<u8>, usize)>,
    on: bool,
}

// SAFETY: the mappings are private to the stream and only touched through `&self`/`&mut self`.
unsafe impl Send for Stream {}

impl Stream {
    /// Waits up to `timeout_ms` for the next picture.
    pub fn next(&mut self, timeout_ms: i32) -> io::Result<Next> {
        let mut p = libc::pollfd {
            fd: self.dev.raw(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid pollfd, count 1.
        let n = unsafe { libc::poll(&mut p, 1, timeout_ms) };
        if n < 0 {
            let e = io::Error::last_os_error();
            return if e.kind() == io::ErrorKind::Interrupted {
                Ok(Next::Idle)
            } else {
                Err(e)
            };
        }
        if p.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Ok(Next::Gone);
        }
        if n == 0 || p.revents & libc::POLLIN == 0 {
            return Ok(Next::Idle);
        }
        let mut b = Buffer {
            type_: TYPE_VIDEO_CAPTURE,
            memory: MEMORY_MMAP,
            ..Buffer::default()
        };
        match ioctl(self.dev.raw(), VIDIOC_DQBUF, &mut b) {
            Ok(()) => Ok(Next::Frame(Dequeued {
                index: b.index,
                bytes_used: b.bytesused as usize,
                timestamp_sec: b.timestamp_sec,
                timestamp_usec: b.timestamp_usec,
                flags: b.flags,
                error: b.flags & BUF_FLAG_ERROR != 0,
            })),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(Next::Idle),
            Err(e)
                if matches!(
                    e.raw_os_error(),
                    Some(libc::ENODEV | libc::EIO | libc::EPIPE)
                ) =>
            {
                Ok(Next::Gone)
            }
            Err(e) => Err(e),
        }
    }

    /// The bytes of a dequeued picture. Valid until it is requeued.
    pub fn data(&self, d: &Dequeued) -> &[u8] {
        let (ptr, len) = self.maps[d.index as usize];
        // SAFETY: the mapping is `len` bytes and lives as long as the stream; the kernel does not
        // write a buffer between its dequeue and requeue.
        unsafe { std::slice::from_raw_parts(ptr.as_ptr(), d.bytes_used.min(len)) }
    }

    /// Hands the buffer back to the driver.
    pub fn requeue(&mut self, d: &Dequeued) -> io::Result<()> {
        let mut b = Buffer {
            index: d.index,
            type_: TYPE_VIDEO_CAPTURE,
            memory: MEMORY_MMAP,
            ..Buffer::default()
        };
        ioctl(self.dev.raw(), VIDIOC_QBUF, &mut b)
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        if self.on {
            let mut t = TYPE_VIDEO_CAPTURE as i32;
            let _ = ioctl(self.dev.raw(), VIDIOC_STREAMOFF, &mut t);
        }
        for (ptr, len) in self.maps.drain(..) {
            // SAFETY: each pair is a mapping made in `start` and not unmapped before.
            unsafe {
                libc::munmap(ptr.as_ptr().cast(), len);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The kernel's sizes for the structs on 64-bit systems (`linux/videodev2.h`).
    #[cfg(target_pointer_width = "64")]
    #[test]
    fn struct_sizes_match_the_kernel() {
        assert_eq!(std::mem::size_of::<Capability>(), 104);
        assert_eq!(std::mem::size_of::<FmtDesc>(), 64);
        assert_eq!(std::mem::size_of::<Format>(), 208);
        assert_eq!(std::mem::size_of::<RequestBuffers>(), 20);
        assert_eq!(std::mem::size_of::<Buffer>(), 88);
        assert_eq!(std::mem::size_of::<StreamParm>(), 204);
        assert_eq!(std::mem::size_of::<FrmSizeEnum>(), 44);
        assert_eq!(std::mem::size_of::<FrmIvalEnum>(), 52);
    }

    /// Request numbers as the kernel headers define them on x86-64 and arm64.
    #[test]
    fn request_numbers_match_the_kernel() {
        assert_eq!(VIDIOC_QUERYCAP, 0x8068_5600);
        assert_eq!(VIDIOC_ENUM_FMT, 0xc040_5602);
        assert_eq!(VIDIOC_S_FMT, 0xc0d0_5605);
        assert_eq!(VIDIOC_REQBUFS, 0xc014_5608);
        assert_eq!(VIDIOC_QUERYBUF, 0xc058_5609);
        assert_eq!(VIDIOC_QBUF, 0xc058_560f);
        assert_eq!(VIDIOC_DQBUF, 0xc058_5611);
        assert_eq!(VIDIOC_STREAMON, 0x4004_5612);
        assert_eq!(VIDIOC_STREAMOFF, 0x4004_5613);
        assert_eq!(VIDIOC_S_PARM, 0xc0cc_5616);
        assert_eq!(VIDIOC_ENUM_FRAMESIZES, 0xc02c_564a);
        assert_eq!(VIDIOC_ENUM_FRAMEINTERVALS, 0xc034_564b);
    }

    #[test]
    fn a_missing_node_is_an_error_not_a_panic() {
        assert!(Device::open(Path::new("/dev/video-does-not-exist")).is_err());
    }
}
