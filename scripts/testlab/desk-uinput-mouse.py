#!/usr/bin/env python3
"""Runs ON the lab laptop as root (sudo): a virtual absolute mouse (like a VM's USB tablet) and keyboard through
/dev/uinput, for driving a native Wayland window of the app (GPUI ignores the touch device that desk-uinput.py makes).
Lab only, lab lock taken. Coordinates are screen pixels (1920x1080).

  sudo python3 desk-uinput-mouse.py click X Y             move there, press and release the left button
  sudo python3 desk-uinput-mouse.py move X Y              only move
  sudo python3 desk-uinput-mouse.py scroll X Y N          wheel at X,Y: N > 0 scrolls down, N < 0 up (notches)
  sudo python3 desk-uinput-mouse.py key Return Escape ... keys by name (table below)
  sudo python3 desk-uinput-mouse.py seq "click 100 100" "key Escape" "wait 1" ...   several steps, 0.4 s apart
"""
import fcntl
import os
import struct
import sys
import time

UI_SET_EVBIT = 0x40045564
UI_SET_KEYBIT = 0x40045565
UI_SET_RELBIT = 0x40045566
UI_SET_ABSBIT = 0x40045567
UI_DEV_CREATE = 0x5501
EV_SYN, EV_KEY, EV_REL, EV_ABS = 0, 1, 2, 3
ABS_X, ABS_Y = 0, 1
REL_WHEEL = 8
BTN_LEFT = 0x110
KEYS = {"Return": 28, "Tab": 15, "Escape": 1, "Down": 108, "Up": 103, "Left": 105, "Right": 106, "space": 57,
        "shift": 42, "ctrl": 29, "alt": 56, "super": 125, "r": 19, "v": 47, "m": 50}
W, H = 1920, 1080
AMAX = 32767


def emit(fd, t, c, v):
    os.write(fd, struct.pack("llHHi", 0, 0, t, c, v))


def syn(fd):
    emit(fd, EV_SYN, 0, 0)


def make():
    fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
    for ev in (EV_KEY, EV_ABS, EV_REL, EV_SYN):
        fcntl.ioctl(fd, UI_SET_EVBIT, ev)
    fcntl.ioctl(fd, UI_SET_KEYBIT, BTN_LEFT)
    for k in KEYS.values():
        fcntl.ioctl(fd, UI_SET_KEYBIT, k)
    fcntl.ioctl(fd, UI_SET_ABSBIT, ABS_X)
    fcntl.ioctl(fd, UI_SET_ABSBIT, ABS_Y)
    fcntl.ioctl(fd, UI_SET_RELBIT, REL_WHEEL)
    name = b"cm-lab-virtual-tablet".ljust(80, b"\0")
    absmax = [0] * 64
    absmax[ABS_X] = absmax[ABS_Y] = AMAX
    dev = name + struct.pack("HHHHi", 3, 0x1234, 0x5679, 1, 0)
    dev += struct.pack("64i", *absmax) + struct.pack("64i", *([0] * 64)) * 3
    os.write(fd, dev)
    fcntl.ioctl(fd, UI_DEV_CREATE)
    time.sleep(1.5)  # the shell needs a moment to see the new device
    return fd


def move(fd, x, y):
    emit(fd, EV_ABS, ABS_X, round(x * AMAX / (W - 1)))
    emit(fd, EV_ABS, ABS_Y, round(y * AMAX / (H - 1)))
    syn(fd)
    time.sleep(0.25)


def do(fd, step):
    p = step.split()
    if p[0] == "move":
        move(fd, int(p[1]), int(p[2]))
    elif p[0] == "click":
        move(fd, int(p[1]), int(p[2]))
        emit(fd, EV_KEY, BTN_LEFT, 1)
        syn(fd)
        time.sleep(0.1)
        emit(fd, EV_KEY, BTN_LEFT, 0)
        syn(fd)
    elif p[0] == "scroll":
        move(fd, int(p[1]), int(p[2]))
        n = int(p[3])
        for _ in range(abs(n)):
            emit(fd, EV_REL, REL_WHEEL, -1 if n > 0 else 1)
            syn(fd)
            time.sleep(0.05)
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
    if steps and steps[0] in ("click", "move", "scroll", "key"):
        steps = [" ".join(steps)]
    elif steps and steps[0] == "seq":
        steps = steps[1:]
    else:
        print(__doc__)
        return 2
    fd = make()
    for s in steps:
        do(fd, s)
        time.sleep(0.4)
    time.sleep(0.3)
    os.close(fd)
    return 0


if __name__ == "__main__":
    sys.exit(main())
