//! GNOME's own screen-cast service, `org.gnome.Mutter.ScreenCast` on the session bus.
//!
//! Why it is used next to the desktop portal (experiment: `verification/L1-linux-0.3.0-recording-targets-experiment-*`):
//! the portal asks the person to pick a screen in a dialog the first time and cannot crop to a
//! rectangle; Mutter's service gives a monitor or a rectangle (`RecordArea`, cropped by the
//! compositor) as a PipeWire stream with no dialog, on both X11 and Wayland sessions of GNOME. It
//! is the same service GNOME's own screenshot and screen-recording tool uses, reachable by any
//! program of the logged-in user. Other desktops (KDE, wlroots) do not have it and use the
//! portal; the recording code picks by asking [`available`].

#[cfg(target_os = "linux")]
pub use imp::available;
#[cfg(target_os = "linux")]
pub use imp::{open, MutterStream};

/// Mutter's service exists on GNOME (Linux) only.
#[cfg(not(target_os = "linux"))]
pub fn available() -> bool {
    false
}

/// The cursor modes of `RecordMonitor`/`RecordArea` (`org.gnome.Mutter.ScreenCast.CursorMode`).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn cursor_mode(show_cursor: bool) -> u32 {
    // 0 hidden, 1 embedded in the picture, 2 as metadata of each buffer.
    u32::from(show_cursor)
}

#[cfg(target_os = "linux")]
mod imp {
    use std::collections::HashMap;
    use std::pin::Pin;
    use std::time::Duration;

    use ashpd::zbus::zvariant::{OwnedObjectPath, Value};
    use ashpd::zbus::{Connection, Message, Proxy};
    use cm_core::CmError;
    use futures_lite::future::{block_on, or, poll_once};
    use futures_lite::{Stream, StreamExt};

    use crate::record::pipewire::PwTarget;

    const SERVICE: &str = "org.gnome.Mutter.ScreenCast";
    const ROOT: &str = "/org/gnome/Mutter/ScreenCast";
    const SESSION_IFACE: &str = "org.gnome.Mutter.ScreenCast.Session";
    const STREAM_IFACE: &str = "org.gnome.Mutter.ScreenCast.Stream";
    /// How long Mutter may take to announce the stream.
    const STREAM_TIMEOUT: Duration = Duration::from_secs(5);

    /// Is Mutter's screen-cast service on the session bus? (A D-Bus property read; never starts a
    /// stream. Gives up after 2 s.)
    pub fn available() -> bool {
        let probe = async {
            let Ok(conn) = Connection::session().await else {
                return false;
            };
            let Ok(p) = Proxy::new(&conn, SERVICE, ROOT, SERVICE).await else {
                return false;
            };
            p.get_property::<i32>("Version").await.is_ok()
        };
        block_on(or(probe, async {
            async_io::Timer::after(Duration::from_secs(2)).await;
            false
        }))
    }

    type Signals = Pin<Box<dyn Stream<Item = Message> + Send>>;

    /// A running Mutter screen-cast session: the PipeWire node to read and the way to end it.
    pub struct MutterStream {
        /// The node in the default PipeWire instance (no portal file descriptor is needed).
        pub node: u32,
        conn: Connection,
        session: OwnedObjectPath,
        closed: Signals,
    }

    impl MutterStream {
        /// True once Mutter ended the session (the screen layout changed, the shell restarted).
        pub fn was_closed(&mut self) -> bool {
            // A `Closed` signal, or the end of the signal stream (the bus connection is gone).
            block_on(poll_once(self.closed.next())).is_some()
        }

        /// Ends the session; the compositor stops producing frames.
        pub fn stop(self) {
            let MutterStream { conn, session, .. } = self;
            let _ = block_on(conn.call_method(
                Some(SERVICE),
                session.as_ref(),
                Some(SESSION_IFACE),
                "Stop",
                &(),
            ));
        }
    }

    fn rejected(what: &str, e: impl std::fmt::Display) -> CmError {
        CmError::platform_rejected(
            format!("The GNOME screen-cast service refused the request ({what}): {e}"),
            Some("mutter_screencast".into()),
        )
    }

    /// Starts a session for `target` (a [`PwTarget::Monitor`] or [`PwTarget::Area`]).
    pub fn open(target: &PwTarget, show_cursor: bool) -> Result<MutterStream, CmError> {
        block_on(open_async(target, show_cursor))
    }

    async fn open_async(target: &PwTarget, show_cursor: bool) -> Result<MutterStream, CmError> {
        let conn = Connection::session()
            .await
            .map_err(|e| rejected("connect", e))?;
        let none: HashMap<&str, Value<'_>> = HashMap::new();
        let reply = conn
            .call_method(
                Some(SERVICE),
                ROOT,
                Some(SERVICE),
                "CreateSession",
                &(none,),
            )
            .await
            .map_err(|e| rejected("create session", e))?;
        let session: OwnedObjectPath = reply
            .body()
            .deserialize()
            .map_err(|e| rejected("create session", e))?;
        let mut props: HashMap<&str, Value<'_>> = HashMap::new();
        props.insert("cursor-mode", Value::from(super::cursor_mode(show_cursor)));
        let stream_path: OwnedObjectPath = match target {
            PwTarget::Monitor { connector, .. } => conn
                .call_method(
                    Some(SERVICE),
                    session.as_ref(),
                    Some(SESSION_IFACE),
                    "RecordMonitor",
                    &(connector.as_str(), props),
                )
                .await
                .map_err(|e| rejected("record monitor", e))?
                .body()
                .deserialize()
                .map_err(|e| rejected("record monitor", e))?,
            PwTarget::Area {
                x,
                y,
                width,
                height,
            } => conn
                .call_method(
                    Some(SERVICE),
                    session.as_ref(),
                    Some(SESSION_IFACE),
                    "RecordArea",
                    &(*x, *y, *width as i32, *height as i32, props),
                )
                .await
                .map_err(|e| rejected("record area", e))?
                .body()
                .deserialize()
                .map_err(|e| rejected("record area", e))?,
            other => {
                return Err(CmError::internal(format!(
                    "the GNOME screen-cast service cannot record {other:?}"
                )))
            }
        };
        // Subscribe before Start: the node id arrives in a signal right after it.
        let stream_proxy = Proxy::new(&conn, SERVICE, stream_path.as_ref(), STREAM_IFACE)
            .await
            .map_err(|e| rejected("stream", e))?;
        let mut added = stream_proxy
            .receive_signal("PipeWireStreamAdded")
            .await
            .map_err(|e| rejected("stream signal", e))?;
        let session_proxy = Proxy::new(&conn, SERVICE, session.as_ref(), SESSION_IFACE)
            .await
            .map_err(|e| rejected("session", e))?;
        let closed = session_proxy
            .receive_signal("Closed")
            .await
            .map_err(|e| rejected("session signal", e))?;
        conn.call_method(
            Some(SERVICE),
            session.as_ref(),
            Some(SESSION_IFACE),
            "Start",
            &(),
        )
        .await
        .map_err(|e| rejected("start", e))?;
        let msg = or(async { added.next().await }, async {
            async_io::Timer::after(STREAM_TIMEOUT).await;
            None
        })
        .await
        .ok_or_else(|| {
            CmError::platform_rejected(
                "The GNOME screen-cast service started but sent no video stream within 5 s.",
                Some("mutter_no_stream".into()),
            )
        })?;
        let node: u32 = msg
            .body()
            .deserialize()
            .map_err(|e| rejected("stream node", e))?;
        Ok(MutterStream {
            node,
            conn,
            session,
            closed: Box::pin(closed),
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_cursor_is_in_the_picture_or_hidden() {
        assert_eq!(super::cursor_mode(true), 1);
        assert_eq!(super::cursor_mode(false), 0);
    }
}
