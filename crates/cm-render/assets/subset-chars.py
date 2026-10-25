import sys
cps=set()
def codec(enc, rows):
    for r in rows:
        for c in range(0x21,0x7f):
            try: s=bytes([r,c]).decode(enc)
            except Exception: continue
            if len(s)==1: cps.add(ord(s))
# GB2312 (EUC-CN lead bytes 0xA1-0xF7)
for hi in range(0xA1,0xF8):
    for lo in range(0xA1,0xFF):
        try: s=bytes([hi,lo]).decode('gb2312'); cps.add(ord(s))
        except Exception: pass
for hi in list(range(0x81,0xA0))+list(range(0xE0,0xEB)):
    for lo in range(0x40,0xFD):
        try: s=bytes([hi,lo]).decode('shift_jis'); cps.add(ord(s))
        except Exception: pass
for a,b in [(0x3000,0x30ff),(0x31f0,0x31ff),(0xff00,0xffef),(0x2000,0x206f),(0x2190,0x21ff),(0x25a0,0x25ff),(0x2600,0x26ff),(0x20,0x7e),(0xa0,0xff)]:
    cps.update(range(a,b+1))
print(" ".join("U+%04X"%c for c in sorted(cps)))
