"""Make a TyrQuake compatibility overlay for the LibreQuake v0.09 lite data."""
from pathlib import Path
import struct

base=Path(__file__).resolve().parent/'id1'
entries={}
for archive in ('pak0.pak','pak1.pak'):
    with (base/archive).open('rb') as f:
        magic,offset,size=struct.unpack('<4sii',f.read(12))
        assert magic==b'PACK'
        f.seek(offset)
        for raw,offset,size in struct.iter_unpack('<56sii',f.read(size)):
            name=raw.rstrip(b'\0').decode('ascii')
            if name=='gfx/colormap.lmp' and size==16384:
                f.seek(offset)
                entries[name]=f.read(size)+bytes([32])
            elif name.startswith('maps/b_') and name.endswith('.bsp'):
                f.seek(offset)
                data=bytearray(f.read(size))
                if struct.unpack_from('<i',data)[0]!=29:
                    continue
                _,vis_size=struct.unpack_from('<ii',data,4+4*8)
                leaves,length=struct.unpack_from('<ii',data,4+10*8)
                if vis_size==0:
                    for pos in range(leaves,leaves+length,28):
                        struct.pack_into('<i',data,pos+4,-1)
                    entries[name]=data
# Start directly in the relay instead of entering the pack's attract loop.
entries['quake.rc']=b'exec default.cfg\nexec config.cfg\nstuffcmds\n'
blob=bytearray(12)
directory=bytearray()
for name,data in entries.items():
    directory.extend(struct.pack('<56sii',name.encode(),len(blob),len(data)))
    blob.extend(data)
struct.pack_into('<4sii',blob,0,b'PACK',len(blob),len(directory))
(base/'pak2.pak').write_bytes(blob+directory)

