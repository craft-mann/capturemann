//! Pure DPI arithmetic for the Windows cursor sprite (no OS calls, so it is tested on every OS).
//!
//! The cursor bitmap that `GetCursorInfo` hands out is loaded at the *system* DPI, while Windows
//! draws the cursor on screen at the DPI of the *monitor* it is on. The recorded frame is in
//! physical px (`CursorTarget.scale` = 1.0), so the sprite must be drawn `monitor / system` times
//! its bitmap size. The renderer divides by `ImageEntry.scale` (image px per physical px), so the
//! value stored with the image is `system / monitor`.

/// The reference DPI of 100 % scaling.
pub const BASE_DPI: u32 = 96;

/// Image px per physical px for a cursor bitmap loaded at `system_dpi` and shown on a monitor at
/// `monitor_dpi`, rounded to 3 decimals (the image id hashes milli-units).
///
/// * monitor DPI equals system DPI (including both at 100 %): 1.0, the bitmap is drawn as is;
/// * a DPI of 0 (a failed query) counts as 96, so a failure never produces 0 or infinity.
pub fn sprite_density(system_dpi: u32, monitor_dpi: u32) -> f64 {
    let sys = f64::from(if system_dpi == 0 {
        BASE_DPI
    } else {
        system_dpi
    });
    let mon = f64::from(if monitor_dpi == 0 {
        BASE_DPI
    } else {
        monitor_dpi
    });
    ((sys / mon) * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn density_table() {
        // (system dpi, monitor dpi, expected image px per physical px)
        let table = [
            (96, 96, 1.0),    // 100 % everywhere: unchanged
            (96, 120, 0.8),   // 125 % monitor
            (96, 144, 0.667), // 150 % monitor, system 100 %
            (96, 192, 0.5),   // 200 % monitor, system 100 %: drawn twice the bitmap size
            (96, 288, 0.333), // 300 % monitor
            (144, 144, 1.0),  // single 150 % monitor: bitmap already physical size
            (192, 96, 2.0),   // 100 % second monitor, system 200 %: drawn at half
            (120, 192, 0.625),
            (0, 192, 0.5), // failed system query counts as 96
            (96, 0, 1.0),  // failed monitor query counts as 96
        ];
        for (sys, mon, want) in table {
            let got = sprite_density(sys, mon);
            assert!((got - want).abs() < 1e-9, "{sys}/{mon}: {got} != {want}");
        }
    }

    #[test]
    fn a_cursor_on_a_second_monitor_gets_that_monitors_density() {
        // System 100 %, monitor A 100 %, monitor B 200 %: the same bitmap, two densities.
        let on_a = sprite_density(96, 96);
        let on_b = sprite_density(96, 192);
        assert_eq!(on_a, 1.0);
        assert_eq!(on_b, 0.5);
        // Drawn size in physical px of a 32 px bitmap: 32 on A, 64 on B.
        assert_eq!((32.0 / on_a).round(), 32.0);
        assert_eq!((32.0 / on_b).round(), 64.0);
    }
}
