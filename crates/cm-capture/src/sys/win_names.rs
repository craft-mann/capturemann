//! The text rules for how Windows windows and displays are named in the picker. Pure functions, so they are
//! tested on every platform; the calls that read the names from Windows are in `win_window.rs` and
//! `win_display.rs`.

/// Whether a name is a file path or an executable name and not something to show a person
/// ("C:\Program Files\WindowsApps\...\Notepad.exe").
pub fn looks_like_path(name: &str) -> bool {
    let n = name.trim();
    n.contains('\\')
        || n.contains('/')
        || n.as_bytes().get(1) == Some(&b':')
        || n.to_ascii_lowercase().ends_with(".exe")
}

/// The file name of a path without its folder and `.exe`: `C:\x\Notepad.exe` gives `Notepad`.
pub fn exe_stem(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    if file.to_ascii_lowercase().ends_with(".exe") {
        file[..file.len() - 4].to_string()
    } else {
        file.to_string()
    }
}

/// Whether the program is the frame that Windows runs for every packaged (UWP) window. The window belongs to
/// this process; the app's own process owns a child window inside it.
pub fn is_frame_host(exe_path: &str) -> bool {
    exe_path
        .rsplit(['\\', '/'])
        .next()
        .is_some_and(|f| f.eq_ignore_ascii_case("ApplicationFrameHost.exe"))
}

/// The name the picker shows for a window's app: the first of the program's own descriptions
/// (`FileDescription`, `ProductName`) that is text a person can read, else the program's file name, else
/// the window title. Never a path.
pub fn choose_app_name(descriptions: &[&str], exe_path: &str, title: &str) -> String {
    for d in descriptions {
        let d = d.trim();
        if !d.is_empty() && !looks_like_path(d) {
            return d.to_string();
        }
    }
    let stem = exe_stem(exe_path);
    if !stem.trim().is_empty() {
        return stem;
    }
    let t = title.trim();
    if looks_like_path(t) {
        String::new()
    } else {
        t.to_string()
    }
}

/// The name of a display: the monitor's own name when Windows has one, else "Display N" with N from the
/// device name (`\\.\DISPLAY2` is "Display 2"), else the display's place in the list.
pub fn display_label(friendly: Option<&str>, device: &str, index: usize) -> String {
    if let Some(f) = friendly.map(str::trim).filter(|f| !f.is_empty()) {
        return f.to_string();
    }
    let digits: String = device
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    match digits.parse::<u32>() {
        Ok(n) if n > 0 => format!("Display {n}"),
        _ => format!("Display {}", index + 1),
    }
}

/// A rectangle `(left, top, right, bottom)` as `(x, y, width, height)`; an empty or inverted one gives `None`.
pub fn rect_to_xywh(left: i32, top: i32, right: i32, bottom: i32) -> Option<(i32, i32, u32, u32)> {
    if right <= left || bottom <= top {
        return None;
    }
    Some((left, top, (right - left) as u32, (bottom - top) as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_executable_names_are_not_app_names() {
        assert!(looks_like_path(
            r"C:\Program Files\WindowsApps\x\Notepad.exe"
        ));
        assert!(looks_like_path("Notepad.exe"));
        assert!(looks_like_path("/usr/bin/x"));
        assert!(!looks_like_path("Windows Terminal"));
        assert!(!looks_like_path("Notepad"));
    }

    #[test]
    fn the_stem_is_the_file_name_without_exe() {
        assert_eq!(exe_stem(r"C:\x\Notepad.exe"), "Notepad");
        assert_eq!(exe_stem(r"C:\x\Notepad.EXE"), "Notepad");
        assert_eq!(exe_stem("calc"), "calc");
        assert_eq!(exe_stem(""), "");
    }

    #[test]
    fn the_frame_host_is_found_by_file_name() {
        assert!(is_frame_host(
            r"C:\Windows\System32\ApplicationFrameHost.exe"
        ));
        assert!(is_frame_host(
            r"c:\windows\system32\applicationframehost.EXE"
        ));
        assert!(!is_frame_host(r"C:\Windows\notepad.exe"));
    }

    #[test]
    fn the_app_name_is_a_description_then_the_file_name_then_the_title() {
        let exe = r"C:\Program Files\WindowsApps\p\Notepad\Notepad.exe";
        assert_eq!(
            choose_app_name(&["Notepad", ""], exe, "a - Notepad"),
            "Notepad"
        );
        // A description that is a path is skipped.
        assert_eq!(
            choose_app_name(&[r"C:\Program Files\Wi", " "], exe, "t"),
            "Notepad"
        );
        // No description and no program: the title, unless it is a path.
        assert_eq!(choose_app_name(&[], "", " Settings "), "Settings");
        assert_eq!(choose_app_name(&[], "", r"C:\x\y.exe"), "");
    }

    #[test]
    fn a_display_without_a_name_is_called_display_n() {
        assert_eq!(
            display_label(Some("DELL U2720Q"), r"\\.\DISPLAY1", 0),
            "DELL U2720Q"
        );
        assert_eq!(display_label(None, r"\\.\DISPLAY2", 0), "Display 2");
        assert_eq!(display_label(Some("  "), r"\\.\DISPLAY12", 0), "Display 12");
        assert_eq!(display_label(None, "", 2), "Display 3");
        assert_eq!(display_label(None, r"\\.\DISPLAY0", 1), "Display 2");
    }

    #[test]
    fn a_rectangle_gives_the_same_size_everywhere() {
        // The size is right minus left: the listing and the saved picture both use this, at 150 % scale too.
        assert_eq!(
            rect_to_xywh(111, 104, 1537, 824),
            Some((111, 104, 1426, 720))
        );
        assert_eq!(rect_to_xywh(0, 0, 0, 10), None);
        assert_eq!(rect_to_xywh(5, 5, 4, 9), None);
    }
}
