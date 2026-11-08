#!/usr/bin/env python3
"""Experiment (release 0.3.0 task 4): does GNOME's org.gnome.Mutter.ScreenCast give frames to a plain
session-bus client without a dialog? Modes: monitor | area X Y W H | window ID.  Usage:
  rec-mutter-exp.py <mode> [args] --out FILE.png [--cursor embedded|hidden|metadata]
Starts a session, prints the PipeWire node id, grabs one frame with a pipewiresrc pipeline, stops."""
import sys, subprocess
from gi.repository import Gio, GLib

args = sys.argv[1:]
out = args[args.index("--out") + 1] if "--out" in args else "/tmp/rec-exp.png"
cursor = {"hidden": 0, "embedded": 1, "metadata": 2}[args[args.index("--cursor") + 1]] if "--cursor" in args else 1
mode = args[0]
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)


def call(path, iface, method, params):
    return bus.call_sync("org.gnome.Mutter.ScreenCast", path, iface, method, params, None,
                         Gio.DBusCallFlags.NONE, 5000, None)


sess = call("/org/gnome/Mutter/ScreenCast", "org.gnome.Mutter.ScreenCast", "CreateSession",
            GLib.Variant("(a{sv})", ({},))).unpack()[0]
print("session", sess)
props = {"cursor-mode": GLib.Variant("u", cursor), "is-recording": GLib.Variant("b", True)}
I = "org.gnome.Mutter.ScreenCast.Session"
if mode == "monitor":
    stream = call(sess, I, "RecordMonitor", GLib.Variant("(sa{sv})", ("", props))).unpack()[0]
elif mode == "area":
    x, y, w, h = map(int, args[1:5])
    stream = call(sess, I, "RecordArea", GLib.Variant("(iiiia{sv})", (x, y, w, h, props))).unpack()[0]
elif mode == "window":
    props["window-id"] = GLib.Variant("t", int(args[1]))
    stream = call(sess, I, "RecordWindow", GLib.Variant("(a{sv})", (props,))).unpack()[0]
print("stream", stream)
loop = GLib.MainLoop()
node = []


def on_sig(conn, sender, path, iface, name, params, ud):
    if name == "PipeWireStreamAdded":
        node.append(params.unpack()[0])
        loop.quit()


bus.signal_subscribe("org.gnome.Mutter.ScreenCast", "org.gnome.Mutter.ScreenCast.Stream", "PipeWireStreamAdded",
                     stream, None, Gio.DBusSignalFlags.NONE, on_sig, None)
call(sess, I, "Start", None)
GLib.timeout_add(5000, loop.quit)
loop.run()
print("node", node)
if node:
    r = subprocess.run(["gst-launch-1.0", "-q", "pipewiresrc", "path=%d" % node[0], "num-buffers=3", "!",
                        "videoconvert", "!", "pngenc", "snapshot=true", "!", "filesink", "location=" + out],
                       capture_output=True, text=True, timeout=30)
    print("pipeline rc", r.returncode, r.stderr[-300:])
call(sess, I, "Stop", None)
