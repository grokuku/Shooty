"""Build the original Astre relay arena using ericw-tools on PATH."""
from pathlib import Path
import math
import shutil
import struct
import subprocess
import tempfile

def brush(x,y,z,X,Y,Z,tex):
    faces=[[(x,y,z),(x,Y,z),(x,y,Z)],[(X,y,z),(X,y,Z),(X,Y,z)],
           [(x,y,z),(x,y,Z),(X,y,z)],[(x,Y,z),(X,Y,z),(x,Y,Z)],
           [(x,y,z),(X,y,z),(x,Y,z)],[(x,y,Z),(x,Y,Z),(X,y,Z)]]
    return '{\n'+''.join(' '.join('( %d %d %d )'%p for p in f)+' '+tex+' 0 0 0 1 1\n' for f in faces)+'}\n'

def entity(name,origin,**keys):
    keys.update(classname=name,origin='%d %d %d'%origin)
    return '{\n'+''.join(f'"{k}" "{v}"\n' for k,v in keys.items())+'}\n'

root=Path(__file__).resolve().parent
with tempfile.TemporaryDirectory() as tmp:
    wd=Path(tmp)
    textures=['slate','copper','tile','beacon']
    blob=bytearray(12)
    directory=bytearray()
    for kind,name in enumerate(textures):
        pixels=bytearray()
        offset=[]
        for lod in range(4):
            offset.append(40+len(pixels))
            for v in range(64>>lod):
                for u in range(64>>lod):
                    x,y=u<<lod,v<<lod
                    seam=x%32<2 or y%32<2
                    grain=(x*17+y*13+x*y)%3
                    value=[6+grain+2*(y//16%2),96+grain+2*(x//8%2),5+grain+3*((x//32+y//32)%2),224+grain][kind]
                    pixels.append(3 if seam else value)
        header=struct.pack('<16sII4I',name.encode(),64,64,*offset)
        size=len(header)+len(pixels)
        directory+=struct.pack('<iiiBBBB16s',len(blob),size,size,68,0,0,0,name.encode())
        blob.extend(header+pixels)
    struct.pack_into('<4sii',blob,0,b'WAD2',4,len(blob))
    (wd/'relay.wad').write_bytes(blob+directory)
    level='{\n"classname" "worldspawn"\n"message" "Relais de Cendre"\n"wad" "relay.wad"\n'
    for box in [(-530,-680,-32,530,680,0,'tile'),(-530,-680,240,530,680,272,'slate'),
                (-544,-680,0,-512,680,240,'slate'),(512,-680,0,544,680,240,'slate'),
                (-512,-680,0,512,-640,240,'slate'),(-512,640,0,512,680,240,'slate')]:
        level+=brush(*box)
    for y in (-320,48,400):
        for x in (-264,200):
            level+=brush(x,y,0,x+64,y+72,240,'copper')
            level+=brush(x-8,y-6,0,x+72,y+78,22,'slate')
            level+=brush(x+12,y-4,74,x+52,y,150,'beacon')
        level+=brush(-264,y,186,264,y+72,226,'copper')
    for i in range(6):
        level+=brush(-130,318+i*32,0,130,350+i*32,(i+1)*12,'tile')
    level+=brush(-150,510,0,150,640,72,'copper')
    for x in (-98,74):
        level+=brush(x,600,72,x+24,636,206,'beacon')
    level+=brush(-98,600,206,98,636,222,'beacon')+'}\n'
    level+=entity('info_player_start',(0,-545,28),angle='90')
    for x,y in ((-355,-120),(345,230),(0,390)):
        level+=entity('monster_army',(x,y,90),angle='270')
    for x,y in ((90,-480),(-360,250)):
        level+=entity('item_shells',(x,y,28))
    level+=entity('item_health',(-340,-230,28))
    for y in (-496,-180,170,540):
        for x in (-330,0,330):
            level+=entity('light',(x,y,168),light='390')
    level+='{\n"classname" "trigger_changelevel"\n"map" "relay"\n'+brush(-68,610,72,68,636,192,'slate')+'}\n'
    (wd/'relay.map').write_text(level)
    for tool in ('qbsp','vis','light'):
        subprocess.run([tool,'relay.map' if tool=='qbsp' else 'relay.bsp'],cwd=wd,check=True)
    dest=root/'id1'/'maps'
    dest.mkdir(exist_ok=True)
    shutil.copy(wd/'relay.bsp',dest/'relay.bsp')

