#!/usr/bin/env python3
"""Runs ON the lab laptop as root (sudo): a virtual touch-like pointer and keyboard through /dev/uinput, for driving
GNOME's own dialogs on Wayland, where xdotool does not reach them (lab only, lab lock taken).

  sudo python3 desk-uinput.py tap X Y            tap at screen pixel X,Y (1920x1080)
  sudo python3 desk-uinput.py key Return [Tab..]  press keys by name (a small table below)
  sudo python3 desk-uinput.py seq "tap 100 100" "key Return" ...   several steps with 0.6 s between
"""
import fcntl
import os
import struct
import sys
import time

UI_SET_EVBIT = 0x40045564
UI_SET_KEYBIT = 0x40045565
UI_SET_ABSBIT = 0x40045567
UI_SET_PROPBIT = 0x4004556E
UI_DEV_CREATE = 0x5501
UI_DEV_DESTROY = 0x5502
EV_SYN, EV_KEY, EV_ABS = 0, 1, 3
ABS_X, ABS_Y = 0, 1
BTN_TOUCH = 0x14A
KEYS = {"Return": 28, "Tab": 15, "Escape": 1, "Down": 108, "Up": 103, "Left": 105, "Right": 106, "space": 57,
        "Print": 99, "shift": 42, "ctrl": 29, "alt": 56, "super": 125}
W, H = 1920, 1080


def emit(fd, t, c, v):
    os.write(fd, struct.pack("llHHi", 0, 0, t, c, v))


def syn(fd):
    emit(fd, EV_SYN, 0, 0)


def make():
    fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
    fcntl.ioctl(fd, UI_SET_EVBIT, EV_KEY)
    fcntl.ioctl(fd, UI_SET_EVBIT, EV_ABS)
    fcntl.ioctl(fd, UI_SET_EVBIT, EV_SYN)
    fcntl.ioctl(fd, UI_SET_KEYBIT, BTN_TOUCH)
    for k in KEYS.values():
        fcntl.ioctl(fd, UI_SET_KEYBIT, k)
    fcntl.ioctl(fd, UI_SET_ABSBIT, ABS_X)
    fcntl.ioctl(fd, UI_SET_ABSBIT, ABS_Y)
    fcntl.ioctl(fd, UI_SET_PROPBIT, 1)  # INPUT_PROP_DIRECT: a touch screen
    name = b"cm-lab-virtual-input".ljust(80, b"\0")
    absmax = [0] * 64
    absmax[ABS_X], absmax[ABS_Y] = W - 1, H - 1
    dev = name + struct.pack("HHHHi", 3, 0x1234, 0x5678, 1, 0)
    dev += struct.pack("64i", *absmax) + struct.pack("64i", *([0] * 64)) * 3
    os.write(fd, dev)
    fcntl.ioctl(fd, UI_DEV_CREATE)
    time.sleep(1.5)  # the shell needs a moment to see the new device
    return fd


def do(fd, step):
    p = step.split()
    if p[0] == "tap":
        x, y = int(p[1]), int(p[2])
        emit(fd, EV_ABS, ABS_X, x)
        emit(fd, EV_ABS, ABS_Y, y)
        syn(fd)
        time.sleep(0.15)
        emit(fd, EV_KEY, BTN_TOUCH, 1)
        syn(fd)
        time.sleep(0.12)
        emit(fd, EV_KEY, BTN_TOUCH, 0)
        syn(fd)
    elif p[0] == "key":
        for name in p[1:]:
            code = KEYS[name]
            emit(fd, EV_KEY, code, 1)
            syn(fd)
            time.sleep(0.08)
            emit(fd, EV_KEY, code, 0)
            syn(fd)
            time.sleep(0.25)
    elif p[0] == "wait":
        time.sleep(float(p[1]))


def main():
    steps = sys.argv[1:]
    if steps and steps[0] in ("tap", "key"):
        steps = [" ".join(steps)]
    elif steps and steps[0] == "seq":
        steps = steps[1:]
    fd = make()
    try:
        for s in steps:
            do(fd, s)
            time.sleep(0.6)
    finally:
        fcntl.ioctl(fd, UI_DEV_DESTROY)
        os.close(fd)


main()
