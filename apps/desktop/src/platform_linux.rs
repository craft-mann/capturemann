//! The Linux tray icon (CAP-2): a StatusNotifierItem served over D-Bus with `zbus`, no GTK.
//!
//! `tray-icon` needs GTK's main loop, which GPUI does not run. The StatusNotifierItem protocol
//! (`org.kde.StatusNotifierItem` plus the `com.canonical.dbusmenu` menu) is what KDE, the GNOME
//! "AppIndicator and KStatusNotifierItem" extension, XFCE, Cinnamon, MATE and others draw. It is a
//! small protocol, `zbus` is already in the tree (through `ashpd` and GPUI), so this is written by
//! hand instead of adding the `ksni` crate (Unlicense, which `deny.toml` does not allow, and a
//! second async runtime choice).
//!
//! The menu content is the pure model in `platform_linux_menu`; this file only exports it.
//! A click is sent to the same handler the macOS/Windows tray uses (`platform::set_menu_handler`),
//! so `Command::TrayMenu` routes it. With no `org.kde.StatusNotifierWatcher` on the session bus
//! (stock GNOME) `build_tray` fails with a message and the app shows its toast; if the watcher
//! restarts later (the shell reloads), the item registers again by itself.
#![cfg(target_os = "linux")]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zbus::blocking::{fdo::DBusProxy, Connection};
use zbus::zvariant::{OwnedValue, Type, Value};

use crate::platform::{menu_text, TrayState};
use crate::platform_linux_menu::{
    action_for, build_menu, props, rgba_to_argb, sni_status, Kind, MenuInputs, MenuNode, Prop,
};

const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/MenuBar";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const APP_ID: &str = "dev.capturemann.app";

type Handler = Box<dyn Fn(String) + Send + Sync>;
static HANDLER: Mutex<Option<Handler>> = Mutex::new(None);

/// Sets the function a menu click calls with the action id (the same ids as `tray-icon`'s).
pub fn set_menu_handler(f: impl Fn(String) + Send + Sync + 'static) {
    *HANDLER.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(f));
}

fn dispatch(key: String) {
    if let Some(h) = HANDLER.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        h(key);
    }
}

type Pixmap = (i32, i32, Vec<u8>);

struct Inner {
    inputs: MenuInputs,
    menu: Vec<MenuNode>,
    revision: u32,
    pixmaps: Vec<Pixmap>,
}

struct Shared(Mutex<Inner>);

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The icon of a state as ARGB32 pixmaps at a few sizes. Panels are dark by default, so the
/// kit's "on dark" icons are used (the same files as the Windows tray).
fn pixmaps_for(state: TrayState) -> Vec<Pixmap> {
    let bytes = crate::platform::windows_tray_ico(false, state);
    let Ok(img) = image::load_from_memory(bytes) else {
        return Vec::new();
    };
    let img = img.into_rgba8();
    [22u32, 32, 48]
        .into_iter()
        .map(|s| {
            let r = image::imageops::resize(&img, s, s, image::imageops::FilterType::Lanczos3);
            (s as i32, s as i32, rgba_to_argb(r.as_raw()))
        })
        .collect()
}

// ---- org.kde.StatusNotifierItem -------------------------------------------------------------

struct Item(Arc<Shared>);

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    #[zbus(property)]
    fn category(&self) -> String {
        "ApplicationStatus".into()
    }
    #[zbus(property)]
    fn id(&self) -> String {
        APP_ID.into()
    }
    #[zbus(property)]
    fn title(&self) -> String {
        "CaptureMann".into() // i18n-ignore: the product name
    }
    #[zbus(property)]
    fn status(&self) -> String {
        sni_status(self.0.lock().inputs.icon).into()
    }
    #[zbus(property)]
    fn window_id(&self) -> u32 {
        0
    }
    #[zbus(property)]
    fn icon_name(&self) -> String {
        String::new()
    }
    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<Pixmap> {
        self.0.lock().pixmaps.clone()
    }
    #[zbus(property)]
    fn overlay_icon_name(&self) -> String {
        String::new()
    }
    #[zbus(property)]
    fn attention_icon_name(&self) -> String {
        String::new()
    }
    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> Vec<Pixmap> {
        Vec::new()
    }
    #[zbus(property)]
    fn tool_tip(&self) -> (String, Vec<Pixmap>, String, String) {
        (
            String::new(),
            Vec::new(),
            self.0.lock().inputs.tooltip.clone(),
            String::new(),
        )
    }
    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn menu(&self) -> zbus::zvariant::ObjectPath<'static> {
        zbus::zvariant::ObjectPath::from_static_str_unchecked(MENU_PATH)
    }

    /// Left click: the launcher (the menu is on the right click / the host's own gesture).
    fn activate(&self, _x: i32, _y: i32) {
        dispatch(crate::platform::ids::OPEN_LAUNCHER.to_owned());
    }
    fn secondary_activate(&self, _x: i32, _y: i32) {}
    fn context_menu(&self, _x: i32, _y: i32) {}
    fn scroll(&self, _delta: i32, _orientation: String) {}
}

// ---- com.canonical.dbusmenu -----------------------------------------------------------------

#[derive(Debug, Type, serde::Serialize, serde::Deserialize, Value, zbus::zvariant::OwnedValue)]
struct Layout {
    id: i32,
    props: HashMap<String, OwnedValue>,
    children: Vec<OwnedValue>,
}

fn prop_value(p: Prop) -> OwnedValue {
    let v: Value<'_> = match p {
        Prop::Str(s) => Value::from(s),
        Prop::Bool(b) => Value::from(b),
        Prop::Int(i) => Value::from(i),
    };
    v.try_to_owned()
        .expect("plain values carry no file descriptors")
}

fn prop_map(n: &MenuNode, names: &[String]) -> HashMap<String, OwnedValue> {
    props(n, names)
        .into_iter()
        .map(|(k, v)| (k.to_owned(), prop_value(v)))
        .collect()
}

fn layout_of(n: &MenuNode, depth: i32, names: &[String]) -> Layout {
    let children = if depth == 0 {
        Vec::new()
    } else {
        n.children
            .iter()
            .map(|c| {
                OwnedValue::try_from(Value::from(layout_of(c, depth - 1, names)))
                    .expect("a layout carries no file descriptors")
            })
            .collect()
    };
    let mut props = prop_map(n, names);
    if !n.children.is_empty() && names.is_empty() {
        props.insert(
            "children-display".into(),
            prop_value(Prop::Str("submenu".into())),
        );
    }
    Layout {
        id: n.id,
        props,
        children,
    }
}

struct DbusMenu(Arc<Shared>);

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl DbusMenu {
    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }
    #[zbus(property)]
    fn text_direction(&self) -> String {
        "ltr".into()
    }
    #[zbus(property)]
    fn status(&self) -> String {
        "normal".into()
    }
    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }

    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: Vec<String>,
    ) -> zbus::fdo::Result<(u32, Layout)> {
        let g = self.0.lock();
        let root = MenuNode {
            id: 0,
            key: String::new(),
            label: String::new(),
            kind: Kind::Submenu,
            enabled: true,
            children: g.menu.clone(),
        };
        let node = if parent_id == 0 {
            Some(&root)
        } else {
            crate::platform_linux_menu::find(&g.menu, parent_id)
        };
        let node = node.ok_or_else(|| zbus::fdo::Error::InvalidArgs("no such menu item".into()))?;
        let mut l = layout_of(node, recursion_depth, &property_names);
        if parent_id == 0 {
            l.props.clear();
            l.props.insert(
                "children-display".into(),
                prop_value(Prop::Str("submenu".into())),
            );
        }
        Ok((g.revision, l))
    }

    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        property_names: Vec<String>,
    ) -> Vec<(i32, HashMap<String, OwnedValue>)> {
        let g = self.0.lock();
        ids.into_iter()
            .filter_map(|id| {
                crate::platform_linux_menu::find(&g.menu, id)
                    .map(|n| (id, prop_map(n, &property_names)))
            })
            .collect()
    }

    fn get_property(&self, id: i32, name: String) -> zbus::fdo::Result<OwnedValue> {
        let g = self.0.lock();
        let n = crate::platform_linux_menu::find(&g.menu, id)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("no such menu item".into()))?;
        prop_map(n, std::slice::from_ref(&name))
            .remove(&name)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("no such property".into()))
    }

    fn event(&self, id: i32, event_id: String, _data: OwnedValue, _timestamp: u32) {
        if event_id == "clicked" {
            let key = action_for(&self.0.lock().menu, id);
            if let Some(key) = key {
                dispatch(key);
            }
        }
    }

    fn event_group(&self, events: Vec<(i32, String, OwnedValue, u32)>) -> Vec<i32> {
        let mut unknown = Vec::new();
        for (id, ev, data, ts) in events {
            if crate::platform_linux_menu::find(&self.0.lock().menu, id).is_none() {
                unknown.push(id);
            } else {
                self.event(id, ev, data, ts);
            }
        }
        unknown
    }

    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    fn about_to_show_group(&self, _ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        (Vec::new(), Vec::new())
    }
}

// ---- the handle the app holds ---------------------------------------------------------------

/// The Linux counterpart of `platform::NativeTray`: the same methods, so the app does not care.
pub struct TrayHandle {
    shared: Arc<Shared>,
    conn: Connection,
}

fn watcher_present(conn: &Connection) -> Result<bool, String> {
    let bus = DBusProxy::new(conn).map_err(|e| e.to_string())?;
    let name: zbus::names::BusName<'_> = WATCHER
        .try_into()
        .map_err(|e: zbus::names::Error| e.to_string())?;
    bus.name_has_owner(name).map_err(|e| e.to_string())
}

fn register(conn: &Connection, service: &str) -> Result<(), String> {
    conn.call_method(
        Some(WATCHER),
        WATCHER_PATH,
        Some(WATCHER),
        "RegisterStatusNotifierItem",
        &(service,),
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Exports the item and the menu and registers with the watcher. Fails with a readable message
/// when there is no session bus or no StatusNotifierWatcher.
pub fn build_tray() -> Result<TrayHandle, String> {
    let inputs = MenuInputs::default();
    let menu = build_menu(&inputs);
    let shared = Arc::new(Shared(Mutex::new(Inner {
        pixmaps: pixmaps_for(inputs.icon),
        menu,
        revision: 1,
        inputs,
    })));
    let service = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
    let conn = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.name(service.as_str()))
        .and_then(|b| b.serve_at(ITEM_PATH, Item(shared.clone())))
        .and_then(|b| b.serve_at(MENU_PATH, DbusMenu(shared.clone())))
        .and_then(|b| b.build())
        .map_err(|e| format!("no D-Bus session bus for the tray icon: {e}"))?;
    if !watcher_present(&conn)? {
        return Err(cm_i18n::tr!(
            "There is no StatusNotifier host on this desktop. On GNOME turn on the \u{201C}AppIndicator and KStatusNotifierItem Support\u{201D} extension; KDE, XFCE, Cinnamon and MATE need nothing. Use the launcher window meanwhile."
        ));
    }
    register(&conn, &service)?;
    // The watcher can restart (the shell reloads its extensions): register again when it returns.
    let c2 = conn.clone();
    let s2 = service.clone();
    let spawned = std::thread::Builder::new()
        .name("sni-watch".into())
        .spawn(move || watch_for_watcher(&c2, &s2));
    if let Err(e) = spawned {
        log::warn!("tray: could not start the watcher monitor: {e}");
    }
    Ok(TrayHandle { shared, conn })
}

fn watch_for_watcher(conn: &Connection, service: &str) {
    let Ok(bus) = DBusProxy::new(conn) else {
        return;
    };
    let Ok(changes) = bus.receive_name_owner_changed() else {
        return;
    };
    for sig in changes {
        let Ok(args) = sig.args() else { continue };
        if args.name().as_str() == WATCHER && args.new_owner().is_some() {
            match register(conn, service) {
                Ok(()) => log::info!("tray: registered again with the restarted watcher"),
                Err(e) => log::warn!("tray: could not register again: {e}"),
            }
        }
    }
}

impl TrayHandle {
    fn emit(&self, path: &str, interface: &str, signal: &str, body: impl serde::Serialize + Type) {
        if let Err(e) = self
            .conn
            .emit_signal(None::<&str>, path, interface, signal, &body)
        {
            log::warn!("tray: could not send {signal}: {e}");
        }
    }

    /// Applies a change to the inputs and tells the host what changed.
    fn update(&self, f: impl FnOnce(&mut MenuInputs)) {
        let (icon, tooltip, status, layout, revision) = {
            let mut g = self.shared.lock();
            let old = g.inputs.clone();
            let mut new = old.clone();
            f(&mut new);
            if new == old {
                return;
            }
            let menu = build_menu(&new);
            let layout = menu != g.menu;
            if layout {
                g.menu = menu;
                g.revision += 1;
            }
            let icon = new.icon != old.icon;
            if icon {
                g.pixmaps = pixmaps_for(new.icon);
            }
            let tooltip = new.tooltip != old.tooltip;
            g.inputs = new;
            (icon, tooltip, icon, layout, g.revision)
        };
        const ITEM: &str = "org.kde.StatusNotifierItem";
        if icon {
            self.emit(ITEM_PATH, ITEM, "NewIcon", ());
        }
        if status {
            let s = sni_status(self.shared.lock().inputs.icon);
            self.emit(ITEM_PATH, ITEM, "NewStatus", (s,));
        }
        if tooltip {
            self.emit(ITEM_PATH, ITEM, "NewToolTip", ());
        }
        if layout {
            self.emit(
                MENU_PATH,
                "com.canonical.dbusmenu",
                "LayoutUpdated",
                (revision, 0i32),
            );
        }
    }

    /// Switches the icon.
    pub fn set_state(&self, state: TrayState) {
        self.update(|i| i.icon = state);
    }

    /// Applies the recording state (REC-5.AC7): icon, tooltip, and the Record / Stop / Pause items.
    pub fn apply_plan(&self, plan: &crate::recording::tray::TrayPlan) {
        self.update(|i| {
            i.icon = plan.state;
            i.recording = plan.state.take_on_screen();
            i.tooltip = plan.tooltip.clone();
            i.record_enabled = plan.record_enabled;
            i.stop_enabled = plan.stop_enabled;
            i.pause_enabled = plan.pause_enabled;
            i.pause_label = plan.pause_label.clone();
        });
    }

    /// Puts the capture delay check mark on `secs` (CAP-5).
    pub fn set_delay(&self, secs: u32) {
        self.update(|i| i.delay_s = secs);
    }

    /// A live language switch: the menu is rebuilt from `menu_text`, so only the recents are
    /// passed on. A language change alone does not alter the inputs, so force a new layout.
    pub fn relabel(&self, recents: &[(String, String)]) {
        let revision = {
            let mut g = self.shared.lock();
            g.inputs.recents = recents.to_vec();
            g.inputs.pause_label = if g.inputs.pause_enabled && g.inputs.recording {
                g.inputs.pause_label.clone()
            } else {
                menu_text(crate::platform::ids::REC_PAUSE)
            };
            g.menu = build_menu(&g.inputs);
            g.revision += 1;
            g.revision
        };
        self.emit(
            MENU_PATH,
            "com.canonical.dbusmenu",
            "LayoutUpdated",
            (revision, 0i32),
        );
    }

    /// Replaces the "Recent captures" entries (last 5, CAP-2).
    pub fn set_recents(&self, recents: &[(String, String)]) {
        self.update(|i| i.recents = recents.to_vec());
    }
}
