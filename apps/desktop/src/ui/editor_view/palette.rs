//! The grouped tool palette on the left of the stage (0.3.3 design spec section 2, board N6):
//! eight 40 px buttons, each showing the tool used last in its group, a corner mark on groups
//! with more than one tool, a group menu (corner click, press-and-hold or right-click) and the
//! blue "new" dot. The rules live in `crate::editor::palette`; this only draws them.

use super::*;
use crate::editor::palette::{menu_step, Group};

/// Group button size (design spec: 40 x 40, icon 18).
const GROUP_BTN: f32 = 40.;
/// The pill's padding (6) plus border (1) on each side.
const RAIL_EDGE: f32 = 7.;
const RAIL_GAP: f32 = 2.;
/// A divider is 1 px high with 3 px above and below.
const DIVIDER_H: f32 = 7.;
const MENU_W: f32 = 220.;
const MENU_ITEM_H: f32 = 34.;
/// Press-and-hold opens the group menu after this long.
const HOLD_MS: u64 = 450;
/// The design spec's info blue (dots, outlines): `#5B8CFF`.
pub(super) const INFO_BLUE: Color = Color(0x5B8C_FFFF);

/// The open group menu: which group and which item the arrow keys are on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct GroupMenu {
    pub group: Group,
    pub cursor: usize,
}

/// The tooltip of a group button: `Callout [K] in Text` (the group is named only for groups
/// with more than one tool).
struct PaletteTip {
    label: SharedString,
    keys: Vec<String>,
    group: Option<SharedString>,
}

impl Render for PaletteTip {
    fn render(&mut self, _w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .max_w(px(tooltip::MAX_W))
            .px(px(8.))
            .py(px(4.))
            .rounded(px(scale::RADIUS_SM))
            .bg(rgba_of(t.raised))
            .border_1()
            .border_color(rgba_of(t.line))
            .text_color(rgba_of(t.text))
            .text_size(px(type_scale::CAPTION))
            .child(shrink_text(self.label.clone()))
            .child(keycaps(&t, &self.keys))
            .when_some(self.group.clone(), |d, g| {
                d.child(
                    div()
                        .text_color(rgba_of(t.text_3))
                        .child(shrink_text(tr_ctx!(
                            "tool tooltip",
                            "in {group}",
                            group = g
                        ))),
                )
            })
    }
}

fn palette_tip(
    tool: Tool,
    group: Group,
) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    let label = SharedString::from(tool.label());
    let keys = vec![tool.key().to_ascii_uppercase().to_string()];
    let group = group.has_menu().then(|| SharedString::from(group.label()));
    move |_, cx| {
        let (label, keys, group) = (label.clone(), keys.clone(), group.clone());
        cx.new(|_| PaletteTip { label, keys, group }).into()
    }
}

/// Height of the pill for `n` visible buttons and `d` dividers.
fn rail_height(n: usize, d: usize) -> f32 {
    2.0 * RAIL_EDGE
        + n as f32 * GROUP_BTN
        + d as f32 * DIVIDER_H
        + (n + d).saturating_sub(1) as f32 * RAIL_GAP
}

/// Distance from the top of the pill to the top of `g`'s button.
fn button_top(g: Group) -> f32 {
    let mut y = RAIL_EDGE;
    for other in Group::visible_groups() {
        if other.starts_section() {
            y += DIVIDER_H + RAIL_GAP;
        }
        if other == g {
            return y;
        }
        y += GROUP_BTN + RAIL_GAP;
    }
    y
}

impl EditorView {
    /// The palette pill: one button per visible group, dividers before Shapes, Emphasis, Redact
    /// and Crop. The selected group is the filled tile (accent fill, button-text glyph).
    pub(super) fn tool_rail(&self, t: &Theme, s: &Snap, cx: &mut Context<Self>) -> AnyElement {
        let mut rail = Self::pill_surface(t)
            .id("editor-tools")
            .role(Role::Toolbar)
            .aria_label(tr!("Annotation tools"))
            .aria_orientation(gpui::Orientation::Vertical)
            .w(px(TOOL_RAIL_W))
            .max_h_full()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(RAIL_GAP))
            .overflow_y_scroll();
        for (i, g) in Group::visible_groups().enumerate() {
            if g.starts_section() {
                rail = rail.child(
                    div()
                        .w(px(GROUP_BTN - 12.))
                        .h(px(1.))
                        .my(px(3.))
                        .flex_none()
                        .bg(rgba_of(t.line)),
                );
            }
            rail = rail.child(self.group_button(t, s, i, g, cx));
        }
        rail.into_any_element()
    }

    fn group_button(
        &self,
        t: &Theme,
        s: &Snap,
        i: usize,
        g: Group,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let on = s.tool.group() == g;
        // The face is the active tool while the group is active, else the remembered one.
        let face = if on { s.tool } else { s.faces.get(g) };
        let has_menu = g.has_menu();
        let name = if has_menu {
            tr!("{group}: {tool}", group = g.label(), tool = face.label())
        } else {
            face.label()
        };
        let key = face.key().to_ascii_uppercase().to_string();
        let hover = rgba_of(t.raised);
        let fg = if on { t.primary_fg } else { t.text_2 };
        let mut b = div()
            .id(ElementId::from(("tool-group", i)))
            .role(Role::Button)
            .aria_label(SharedString::from(name))
            .aria_description(SharedString::from(tr!("Shortcut {key}", key = key)))
            .aria_keyshortcuts(SharedString::from(key))
            .aria_toggled(toggled(on))
            .focusable()
            .tab_stop(true)
            .relative()
            .size(px(GROUP_BTN))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(11.))
            .border_1()
            .border_color(gpui::transparent_black())
            .cursor_pointer()
            .when(on, |d| d.bg(rgba_of(accent(t))))
            .when(!on, |d| d.hover(move |s| s.bg(hover)))
            .focus_visible(|s| s.border_color(rgba_of(t.text)))
            .tooltip(palette_tip(face, g))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                // A press-and-hold that opened the menu must not also pick the tool.
                if std::mem::take(&mut this.hold_fired) {
                    return;
                }
                this.pick_tool(face, window, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| this.hold = None),
            )
            .child(icon(face.icon(), 18., fg));
        if has_menu {
            b = b
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, _, cx| this.start_hold(g, cx)),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                        this.open_group_menu(g, cx);
                        cx.stop_propagation();
                    }),
                )
                .child(
                    // The corner mark is also a click target for the menu.
                    div()
                        .id(ElementId::from(("tool-group-corner", i)))
                        .absolute()
                        .right(px(2.))
                        .bottom(px(2.))
                        .size(px(14.))
                        .flex()
                        .items_end()
                        .justify_end()
                        .pr(px(2.))
                        .pb(px(2.))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                this.open_group_menu(g, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .child(icon("cm-group-corner", 6., fg)),
                );
        }
        if g.has_new_tool() {
            b = b.child(
                div()
                    .absolute()
                    .top(px(5.))
                    .right(px(5.))
                    .size(px(6.))
                    .rounded_full()
                    .bg(rgba_of(INFO_BLUE)),
            );
        }
        b.into_any_element()
    }

    /// Makes `tool` the active tool and puts focus back on the canvas.
    pub(super) fn pick_tool(&mut self, tool: Tool, window: &mut Window, cx: &mut Context<Self>) {
        self.group_menu = None;
        if let Some(d) = &self.doc {
            d.update(cx, |d, cx| d.set_tool(tool, cx));
        }
        window.focus(&self.canvas_focus, cx);
        cx.notify();
    }

    fn start_hold(&mut self, g: Group, cx: &mut Context<Self>) {
        self.hold_seq += 1;
        let token = self.hold_seq;
        self.hold = Some(token);
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(HOLD_MS))
                .await;
            this.update(cx, |this, cx| {
                if this.hold == Some(token) {
                    this.hold = None;
                    this.hold_fired = true;
                    this.open_group_menu(g, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Opens the menu of `g` with the cursor on the tool the button shows.
    pub(super) fn open_group_menu(&mut self, g: Group, cx: &mut Context<Self>) {
        if !g.has_menu() {
            return;
        }
        let current = self
            .doc
            .as_ref()
            .map(|d| {
                let st = &d.read(cx).state;
                if st.tool.group() == g {
                    st.tool
                } else {
                    st.faces.get(g)
                }
            })
            .unwrap_or_else(|| g.default_face());
        let cursor = g.tools().iter().position(|t| *t == current).unwrap_or(0);
        self.group_menu = Some(GroupMenu { group: g, cursor });
        cx.notify();
    }

    /// Keys while the group menu is open: arrows move, Enter or Space picks, Esc closes.
    /// Returns whether the key was consumed.
    pub(super) fn menu_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(m) = self.group_menu else {
            return false;
        };
        match key {
            "down" | "right" => {
                self.group_menu = Some(GroupMenu {
                    cursor: menu_step(m.group, m.cursor, true),
                    ..m
                });
            }
            "up" | "left" => {
                self.group_menu = Some(GroupMenu {
                    cursor: menu_step(m.group, m.cursor, false),
                    ..m
                });
            }
            "enter" | "space" => {
                if let Some(tool) = m.group.tools().get(m.cursor).copied() {
                    self.pick_tool(tool, window, cx);
                }
            }
            "escape" => self.group_menu = None,
            _ => return false,
        }
        cx.notify();
        true
    }

    /// The group menu (radio items) beside the palette, drawn as a deferred popover.
    pub(super) fn group_menu_view(
        &self,
        t: &Theme,
        m: GroupMenu,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let b = self.canvas_bounds.get();
        let groups = Group::visible_groups().count();
        let dividers = Group::visible_groups()
            .filter(|g| g.starts_section())
            .count();
        let rail_h = rail_height(groups, dividers);
        let avail = f32::from(b.size.height) - 2.0 * PILL_MARGIN;
        let rail_top = PILL_MARGIN + ((avail - rail_h) / 2.0).max(0.0);
        let at = point(
            b.origin.x + px(PILL_MARGIN + TOOL_RAIL_W + 8.),
            b.origin.y + px(rail_top + button_top(m.group) - 6.),
        );
        let entity = _cx.entity();
        let items: Vec<AnyElement> = m
            .group
            .tools()
            .iter()
            .enumerate()
            .map(|(i, tool)| {
                let tool = *tool;
                let current = i == m.cursor;
                let hover = rgba_of(t.surface_2);
                let e = entity.clone();
                div()
                    .id(ElementId::from(("tool-menu-item", i)))
                    .role(Role::RadioButton)
                    .aria_label(SharedString::from(tool.label()))
                    .aria_keyshortcuts(SharedString::from(
                        tool.key().to_ascii_uppercase().to_string(),
                    ))
                    .aria_toggled(toggled(current))
                    .focusable()
                    .h(px(MENU_ITEM_H))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(9.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .text_color(rgba_of(t.text))
                    .when(current, |d| d.bg(rgba_of(t.raised)))
                    .when(!current, |d| d.hover(move |s| s.bg(hover)))
                    .on_click(move |_, window, cx| {
                        e.update(cx, |this, cx| this.pick_tool(tool, window, cx));
                    })
                    .child(icon(tool.icon(), 16., t.text_2))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .child(shrink_text(tool.label())),
                    )
                    .child(keycaps(t, &[tool.key().to_ascii_uppercase().to_string()]))
                    .into_any_element()
            })
            .collect();
        let body = div()
            .id("tool-group-menu")
            .occlude()
            .role(Role::RadioGroup)
            .aria_label(SharedString::from(m.group.label()))
            .w(px(MENU_W))
            .p(px(6.))
            .flex()
            .flex_col()
            .gap(px(1.))
            .rounded(px(13.))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.surface_2))
            .shadow_lg()
            .on_mouse_down_out(_cx.listener(|this, _, _, cx| {
                this.group_menu = None;
                cx.notify();
            }))
            .child(
                div()
                    .px(px(8.))
                    .pt(px(4.))
                    .pb(px(6.))
                    .text_size(px(11.))
                    .text_color(rgba_of(t.mist))
                    .child(SharedString::from(m.group.label())),
            )
            .children(items);
        deferred(
            anchored()
                .position(at)
                .anchor(gpui::Anchor::TopLeft)
                .snap_to_window_with_margin(px(8.))
                .child(body),
        )
        .with_priority(11)
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Board N6: about 380 px tall with 8 buttons and 4 dividers.
    #[test]
    fn rail_height_is_about_380() {
        let h = rail_height(8, 4);
        assert!((360.0..=420.0).contains(&h), "{h}");
    }

    /// The menu is placed from the same arithmetic as the pill: the first button starts at the
    /// pill's edge and later ones follow gaps and dividers.
    #[test]
    fn button_tops_follow_gaps_and_dividers() {
        assert_eq!(button_top(Group::Select), RAIL_EDGE);
        let shapes = RAIL_EDGE + GROUP_BTN + RAIL_GAP + DIVIDER_H + RAIL_GAP;
        assert_eq!(button_top(Group::Shapes), shapes);
        let last = Group::visible_groups().last().unwrap();
        assert!(
            button_top(last) + GROUP_BTN + RAIL_EDGE
                <= rail_height(
                    Group::visible_groups().count(),
                    Group::visible_groups()
                        .filter(|g| g.starts_section())
                        .count()
                ) + 0.01
        );
    }
}
