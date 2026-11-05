#!/usr/bin/env python3
"""A/V sync of a bundle recorded with win-av-sync.ps1 (screen + system audio): flash frame against beep onset.

  av-sync-analyze.py <bundle.cmrec> [av-log.csv]

Needs ffmpeg and ffprobe on PATH. Prints the first white frame's time minus the beep onset in system.wav (negative:
the picture is first) and, with the stimulus log, the trigger-to-frame and trigger-to-onset latencies on the session
clock. Frame times are quantized to the recording's frame grid (33 ms at 30 fps): their mean is about 17 ms later
than the true appearance, so read the means, not single events.
"""
import json, statistics as st, struct, subprocess, sys
d = sys.argv[1]
m = json.load(open(f'{d}/manifest.json'))
T = m['tracks']; R = 120000.0
scr, sysd = T['screen'], T['system']
print('screen start/file_start', scr['start'], scr.get('file_start'), 'system', {k: sysd[k] for k in sysd if k in ('start','end','file_start','rate','channels')})
# video: mean luma per frame via 1x1 gray
fr = subprocess.run(['ffprobe','-v','error','-select_streams','v:0','-show_entries','frame=pts_time','-of','csv=p=0',f'{d}/screen.mp4'],capture_output=True,text=True).stdout.split()
pts = [float(x.strip(',')) for x in fr if x.strip(',')]
raw = subprocess.run(['ffmpeg','-v','error','-i',f'{d}/screen.mp4','-vf','scale=1:1:flags=area,format=gray','-f','rawvideo','-'],capture_output=True).stdout
print('frames', len(pts), len(raw), 'first pts', pts[:3])
vs = 0.0  # mp4 pts are session time already (first pts = file_start)
flashes = []
prev = 0
for i, b in enumerate(raw):
    if b > 128 and prev <= 128: flashes.append(vs + pts[i])
    prev = b
# audio
sr = sysd['rate']; ch = sysd['channels']
a = subprocess.run(['ffmpeg','-v','error','-i',f'{d}/system.wav','-f','f32le','-ac','1','-'],capture_output=True).stdout
n = len(a)//4; x = struct.unpack('<%df' % n, a[:n*4])
as_ = sysd.get('file_start', sysd['start'])/R
print('audio samples', n, 'dur', n/sr, 'start', as_)
peak = max(abs(v) for v in x) or 1
thr = 0.1*peak
onsets = []; quiet = 0
for i, v in enumerate(x):
    if abs(v) > thr:
        if quiet > sr*0.5: onsets.append(as_ + i/sr)
        quiet = 0
    else: quiet += 1
print('flashes', len(flashes), 'onsets', len(onsets), 'peak', peak)
diffs = []
for f in flashes:
    best = min(onsets, key=lambda o: abs(o-f), default=None)
    if best is not None and abs(best-f) < 0.5: diffs.append((f-best)*1000)
import statistics as st
print('flash minus beep onset (ms): n=%d mean %.1f median %.1f sd %.1f min %.1f max %.1f' % (len(diffs), st.mean(diffs), st.median(diffs), st.pstdev(diffs), min(diffs), max(diffs)))
print([round(v,1) for v in diffs])
# absolute check against the stimulus log (QPC ns, same host clock as the session origin)
if len(sys.argv) > 2:
    sess = json.load(open(f'{d}/session.json'))
    o = sess['clock']['origin_ns']
    rows = [l.strip().split(',') for l in open(sys.argv[2]).read().split('\n')[:] if l.strip() and not l.startswith('ERR')]
    q = [int(r[5]) for r in rows]
    vl, al = [], []
    for qq in q:
        t = (qq - o)/1e9   # session seconds of the trigger
        f = min(flashes, key=lambda x: abs(x - t - 0.02), default=None)
        b = min(onsets, key=lambda x: abs(x - t), default=None)
        if f is not None and abs(f-t) < 0.6: vl.append((f - t)*1000)
        if b is not None and abs(b-t) < 0.6: al.append((b - t)*1000)
    print('trigger->first white frame pts (ms): n=%d mean %.1f sd %.1f min %.1f max %.1f' % (len(vl), st.mean(vl), st.pstdev(vl), min(vl), max(vl)))
    print('trigger->beep onset in system.wav (ms): n=%d mean %.1f sd %.1f min %.1f max %.1f' % (len(al), st.mean(al), st.pstdev(al), min(al), max(al)))
