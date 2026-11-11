import struct, sys
for p in sys.argv[1:]:
    f = open(p, 'rb')
    d = f.read(4096)
    pe = struct.unpack_from('<I', d, 0x3C)[0]
    sub = struct.unpack_from('<H', d, pe + 24 + 68)[0]
    print(p.split('/')[-1], 'subsystem', {2: 'GUI', 3: 'CONSOLE'}.get(sub, sub))
