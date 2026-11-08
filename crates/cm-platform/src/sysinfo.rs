//! Facts about the machine the diagnostics report prints (DIAG-1): on Windows the OS edition and build
//! and the graphics adapters with their driver versions, which decide whether the GPU compositor and the
//! hardware video encoder work. Elsewhere [`lines`] is empty (macOS has its own probes in
//! the report). The formatting is plain functions so it is tested on every system; only the reading of
//! the registry and of DXGI is Windows code.

/// The OS line, for example `Windows 11 Pro 24H2, build 26200.1000`. The registry still says
/// "Windows 10 ..." in `ProductName` on Windows 11 (the name was never updated), so a build number of
/// 22000 or more turns it into 11.
pub fn format_os(
    product: &str,
    display_version: Option<&str>,
    build: &str,
    ubr: Option<u32>,
) -> String {
    let mut name = product.trim().to_owned();
    if build.trim().parse::<u32>().is_ok_and(|b| b >= 22_000) {
        name = name.replace("Windows 10", "Windows 11");
    }
    if name.is_empty() {
        name = "Windows".to_owned();
    }
    let mut out = name;
    if let Some(v) = display_version.map(str::trim).filter(|v| !v.is_empty()) {
        out.push(' ');
        out.push_str(v);
    }
    out.push_str(", build ");
    out.push_str(build.trim());
    if let Some(u) = ubr {
        out.push('.');
        out.push_str(&u.to_string());
    }
    out
}

/// A DXGI user-mode driver version (`LARGE_INTEGER` of four 16-bit parts) as `a.b.c.d`. Windows packs
/// the vendor's version this way; the part the vendor shows to people (for example NVIDIA's 551.23) is
/// in the last two parts, so the whole number is printed and nothing is guessed.
pub fn format_driver_version(v: i64) -> String {
    let v = v as u64;
    format!(
        "{}.{}.{}.{}",
        (v >> 48) & 0xffff,
        (v >> 32) & 0xffff,
        (v >> 16) & 0xffff,
        v & 0xffff
    )
}

/// One graphics adapter as a report line. `driver` is [`format_driver_version`]'s input when the
/// adapter answered; `software` marks the Microsoft Basic Render Driver (WARP), which means no GPU
/// reached the program.
pub fn format_gpu(
    description: &str,
    vendor_id: u32,
    dedicated_bytes: u64,
    driver: Option<i64>,
    software: bool,
) -> String {
    let mut out = format!("GPU {}", description.trim());
    if software {
        out.push_str(" (software renderer: no hardware GPU is in use)");
    }
    match driver {
        Some(v) => out.push_str(&format!(", driver {}", format_driver_version(v))),
        None => out.push_str(", driver version unknown"),
    }
    if dedicated_bytes > 0 {
        out.push_str(&format!(
            ", {} MB dedicated memory",
            dedicated_bytes / 1_000_000
        ));
    }
    out.push_str(&format!(", vendor id {vendor_id:#06x}"));
    out
}

/// Looks `vendor` and `device` (PCI ids) up in the text of a `pci.ids` file: the vendor's name and the
/// device's name. A vendor line starts at column 0 (`8086  Intel Corporation`), its devices follow
/// indented by one tab (`\t3ea0  WhiskeyLake-U GT2 [UHD Graphics 620]`); subdevice lines (two tabs) are
/// ignored. Pure, so it is tested on every system.
pub fn pci_names(pci_ids: &str, vendor: u16, device: u16) -> (Option<String>, Option<String>) {
    let (v, d) = (format!("{vendor:04x}"), format!("{device:04x}"));
    let mut vendor_name = None;
    let mut in_vendor = false;
    for line in pci_ids.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if !line.starts_with('\t') {
            in_vendor = line.get(..4).is_some_and(|id| id.eq_ignore_ascii_case(&v));
            if in_vendor {
                vendor_name = line.get(4..).map(|n| n.trim().to_owned());
            } else if vendor_name.is_some() {
                break;
            }
        } else if in_vendor && !line.starts_with("\t\t") {
            let t = line.trim_start_matches('\t');
            if t.get(..4).is_some_and(|id| id.eq_ignore_ascii_case(&d)) {
                return (vendor_name, t.get(4..).map(|n| n.trim().to_owned()));
            }
        }
    }
    (vendor_name, None)
}

/// One DRM graphics device (Linux) as a report line: the names from `pci.ids` when found, else the
/// ids, then the kernel driver (`i915`, `amdgpu`, `nvidia`, `nouveau`, `vmwgfx`...).
pub fn format_drm_gpu(
    vendor: u16,
    device: u16,
    vendor_name: Option<&str>,
    device_name: Option<&str>,
    driver: Option<&str>,
) -> String {
    let name = match (vendor_name, device_name) {
        (Some(v), Some(d)) => format!("{} {}", v.trim(), d.trim()),
        (Some(v), None) => format!("{} device {device:04x}", v.trim()),
        _ => format!("PCI {vendor:04x}:{device:04x}"),
    };
    format!(
        "GPU {name}, kernel driver {}, pci id {vendor:04x}:{device:04x}",
        driver.unwrap_or("unknown")
    )
}

/// The report lines this system adds to "System": OS build, then one line per graphics adapter.
/// Empty off Windows.
pub fn lines() -> Vec<String> {
    sys::lines()
}

#[cfg(windows)]
mod sys {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIDevice, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
    };
    use windows::Win32::System::Registry::{
        RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
    };

    const KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

    fn reg_string(name: &str) -> Option<String> {
        let (key, name) = (HSTRING::from(KEY), HSTRING::from(name));
        let mut bytes = 0u32;
        // SAFETY: both names are NUL-terminated wide strings; a null data pointer only asks for the
        // size, which is written to `bytes`.
        let size = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                &key,
                &name,
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut bytes),
            )
        };
        if size != ERROR_SUCCESS || bytes == 0 {
            return None;
        }
        let mut buf = vec![0u16; (bytes as usize).div_ceil(2)];
        // SAFETY: `buf` holds `bytes` bytes, as the first call said; the length is updated.
        let got = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                &key,
                &name,
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut bytes),
            )
        };
        if got != ERROR_SUCCESS {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    }

    fn reg_dword(name: &str) -> Option<u32> {
        let (key, name) = (HSTRING::from(KEY), HSTRING::from(name));
        let mut value = 0u32;
        let mut bytes = std::mem::size_of::<u32>() as u32;
        // SAFETY: `value` is a u32 and `bytes` says so; the names are NUL-terminated wide strings.
        let got = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                &key,
                &name,
                RRF_RT_REG_DWORD,
                None,
                Some((&mut value as *mut u32).cast()),
                Some(&mut bytes),
            )
        };
        (got == ERROR_SUCCESS).then_some(value)
    }

    fn os_line() -> Option<String> {
        let build = reg_string("CurrentBuildNumber")?;
        let product = reg_string("ProductName").unwrap_or_default();
        // `DisplayVersion` ("24H2") exists from 20H2; before that `ReleaseId` ("1909") is the closest.
        let version = reg_string("DisplayVersion").or_else(|| reg_string("ReleaseId"));
        Some(super::format_os(
            &product,
            version.as_deref(),
            &build,
            reg_dword("UBR"),
        ))
    }

    fn gpu_lines() -> Vec<String> {
        // SAFETY: creating a factory has no preconditions; an error (no DXGI) gives no lines.
        let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
            return vec!["GPU unknown (DXGI is not available)".to_owned()];
        };
        let mut out = Vec::new();
        let mut i = 0;
        // SAFETY: EnumAdapters1 returns an error (DXGI_ERROR_NOT_FOUND) past the last adapter.
        while let Ok(adapter) = unsafe { factory.EnumAdapters1(i) } {
            i += 1;
            // SAFETY: the description is written into a struct we own.
            let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
                continue;
            };
            let len = desc
                .Description
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(desc.Description.len());
            let name = String::from_utf16_lossy(&desc.Description[..len]);
            // SAFETY: the interface id is a valid GUID; the call returns the user-mode driver version.
            let driver = unsafe { adapter.CheckInterfaceSupport(&IDXGIDevice::IID) }.ok();
            let software = desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0;
            out.push(super::format_gpu(
                &name,
                desc.VendorId,
                desc.DedicatedVideoMemory as u64,
                driver,
                software,
            ));
        }
        out
    }

    pub fn lines() -> Vec<String> {
        let mut out = Vec::new();
        out.extend(os_line());
        out.extend(gpu_lines());
        out
    }
}

#[cfg(target_os = "linux")]
mod sys {
    use super::{format_drm_gpu, pci_names};

    fn hex(path: &std::path::Path) -> Option<u16> {
        let t = std::fs::read_to_string(path).ok()?;
        u16::from_str_radix(t.trim().trim_start_matches("0x"), 16).ok()
    }

    fn pci_ids() -> String {
        ["/usr/share/hwdata/pci.ids", "/usr/share/misc/pci.ids"]
            .iter()
            .find_map(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default()
    }

    /// The OS release and one line per GPU the kernel exposes (`/sys/class/drm/card*`, render nodes
    /// and connectors are skipped), each with its kernel driver.
    pub fn lines() -> Vec<String> {
        let mut out = Vec::new();
        if let Ok(t) = std::fs::read_to_string("/etc/os-release") {
            let pretty = t
                .lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                .map(|v| v.trim_matches('"').to_owned());
            if let Some(p) = pretty {
                out.push(format!("OS {p}"));
            }
        }
        if let Ok(k) = std::fs::read_to_string("/proc/sys/kernel/osrelease") {
            out.push(format!("kernel {}", k.trim()));
        }
        let mut cards: Vec<_> = std::fs::read_dir("/sys/class/drm")
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                    n.starts_with("card") && n[4..].bytes().all(|b| b.is_ascii_digit())
                })
            })
            .collect();
        cards.sort();
        let ids = pci_ids();
        for c in cards {
            let dev = c.join("device");
            let (Some(v), Some(d)) = (hex(&dev.join("vendor")), hex(&dev.join("device"))) else {
                continue;
            };
            let driver = std::fs::read_link(dev.join("driver"))
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
            let (vn, dn) = pci_names(&ids, v, d);
            out.push(format_drm_gpu(
                v,
                d,
                vn.as_deref(),
                dn.as_deref(),
                driver.as_deref(),
            ));
        }
        out
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod sys {
    pub fn lines() -> Vec<String> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DIAG-1: a GPU line on Linux names the device from `pci.ids` and the kernel driver.
    #[test]
    fn diag1_pci_ids_name_the_gpu() {
        let ids = "# header\n8086  Intel Corporation\n\t3ea0  WhiskeyLake-U GT2 [UHD Graphics 620]\n\t\t1028 0000  sub\n10de  NVIDIA Corporation\n\t1c82  GP107 [GeForce GTX 1050 Ti]\n";
        let (v, d) = pci_names(ids, 0x8086, 0x3ea0);
        assert_eq!(v.as_deref(), Some("Intel Corporation"));
        assert_eq!(d.as_deref(), Some("WhiskeyLake-U GT2 [UHD Graphics 620]"));
        assert_eq!(
            pci_names(ids, 0x10de, 0x1c82).1.as_deref(),
            Some("GP107 [GeForce GTX 1050 Ti]")
        );
        // A known vendor with an unknown device, and an unknown vendor.
        assert_eq!(
            pci_names(ids, 0x8086, 0x9999),
            (Some("Intel Corporation".into()), None)
        );
        assert_eq!(pci_names(ids, 0x1234, 0x1111), (None, None));
        assert_eq!(
            format_drm_gpu(0x8086, 0x3ea0, v.as_deref(), d.as_deref(), Some("i915")),
            "GPU Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620], kernel driver i915, pci id 8086:3ea0"
        );
        assert_eq!(
            format_drm_gpu(0x1234, 0x1111, None, None, None),
            "GPU PCI 1234:1111, kernel driver unknown, pci id 1234:1111"
        );
    }

    #[test]
    fn windows_11_is_named_by_its_build_not_by_the_registry_product_name() {
        assert_eq!(
            format_os("Windows 10 Pro", Some("24H2"), "26200", Some(1000)),
            "Windows 11 Pro 24H2, build 26200.1000"
        );
        // A real Windows 10 keeps its name.
        assert_eq!(
            format_os("Windows 10 Home", Some("22H2"), "19045", Some(5247)),
            "Windows 10 Home 22H2, build 19045.5247"
        );
    }

    #[test]
    fn missing_registry_values_leave_a_shorter_but_valid_line() {
        assert_eq!(format_os("", None, "19041", None), "Windows, build 19041");
        assert_eq!(
            format_os("Windows 10 Pro", Some(" "), "19041", None),
            "Windows 10 Pro, build 19041"
        );
    }

    #[test]
    fn the_driver_version_unpacks_four_sixteen_bit_parts() {
        let v = (31i64 << 48) | (101i64 << 16) | 2127;
        assert_eq!(format_driver_version(v), "31.0.101.2127");
        assert_eq!(format_driver_version(0), "0.0.0.0");
        // A negative LARGE_INTEGER (top bit set) is read as unsigned.
        assert_eq!(format_driver_version(i64::MIN | 1), "32768.0.0.1");
    }

    #[test]
    fn a_gpu_line_says_what_is_known_and_what_is_not() {
        let v = (31i64 << 48) | (101i64 << 16) | 2127;
        assert_eq!(
            format_gpu("Intel(R) UHD Graphics 620", 0x8086, 134_217_728, Some(v), false),
            "GPU Intel(R) UHD Graphics 620, driver 31.0.101.2127, 134 MB dedicated memory, vendor id 0x8086"
        );
        let warp = format_gpu("Microsoft Basic Render Driver", 0x1414, 0, None, true);
        assert!(warp.contains("software renderer"), "{warp}");
        assert!(warp.contains("driver version unknown"), "{warp}");
        assert!(!warp.contains("dedicated memory"), "{warp}");
    }

    #[test]
    fn off_windows_the_system_adds_no_lines() {
        if cfg!(not(windows)) {
            assert!(lines().is_empty());
        }
    }
}
