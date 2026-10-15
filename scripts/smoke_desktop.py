"""End-to-end checks against an explicitly selected, disposable desktop session.

Creates only new documents and files beneath --output. Never closes existing documents.
"""
from __future__ import annotations
import argparse
import base64
from datetime import datetime,timezone
import hashlib
import json
from pathlib import Path
import struct
import sys
import zlib

sys.path.insert(0,str(Path(__file__).resolve().parents[1]/"src"))
from vectorcraft_sci.protocol import ControlClient
from vectorcraft_sci.engine import Toolbox,flatten
from vectorcraft_sci.layout import LayoutError


def example_png(width=160,height=110,tint=0):
    def chunk(kind,payload):
        data=kind+payload
        return struct.pack('!I',len(payload))+data+struct.pack('!I',zlib.crc32(data)&0xffffffff)
    raw=bytearray()
    for y in range(height):
        raw.append(0)
        for x in range(width):
            cell=((x-40)**2/240+(y-55)**2/150<1)or((x-100)**2/350+(y-35)**2/220<1)
            raw.extend((40+tint,150 if cell else 40,210 if cell else 80))
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('!2I5B',width,height,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(bytes(raw)))+chunk(b'IEND',b'')


def main():
    p=argparse.ArgumentParser();p.add_argument('--port',type=int,required=True);p.add_argument('--output',type=Path,required=True)
    args=p.parse_args();out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
    c=ControlClient(args.port);t=Toolbox(c);checks=[]
    def check(name,condition):
        if not condition:raise AssertionError(name)
        checks.append(name);print('PASS',name,flush=True)
    def snapshot(ids):
        nodes=flatten(c.inspect()['layers'])
        return {i:nodes[i]['bounds'] for i in ids}
    def choose(ids):c.execute('select.set',ids=ids)
    def close(a,b):return abs(a-b)<.08
    def near_boxes(a,b):return all(close(a[i][k],b[i][k]) for i in a for k in a[i])
    ui=c.ui();check('actual desktop control',isinstance(ui.get('documents'),list))
    c.execute('file.new',title='科研组图示例',width=440,height=350)
    ids=[]
    for i,(x,y,w,h) in enumerate([(45,45,160,110),(330,70,110,110),(70,270,150,80),(360,260,160,100)]):
        blob=example_png(int(w),int(h),i*20)
        r=c.execute('file.place',name=f'Example-{i+1}.png',dataBase64=base64.b64encode(blob).decode(),at=[x+w/2,y+h/2],link=False)
        ids.append(r['ids'][0])
    choose(ids);original=snapshot(ids)
    before_history=len(c.inspect()['history'])
    t.run('grid',{'columns':2,'gap_x':18,'gap_y':24,'order':'rows'})
    after=snapshot(ids)
    check('image grid layout',close(after[ids[1]]['x']-after[ids[0]]['x']-after[ids[0]]['width'],18) and close(after[ids[2]]['y']-after[ids[0]]['y']-max(after[i]['height'] for i in ids[:2]),24))
    check('one operation one undo group',len(c.inspect()['history'])==before_history+1)
    t.run('undo');check('undo restores exact bounds',near_boxes(original,snapshot(ids)))
    t.run('redo');check('redo restores grid',near_boxes(after,snapshot(ids)))
    t.run('resize',{'width':130,'keep_ratio':True});after=snapshot(ids)
    check('individual proportional image resize',all(close(b['width'],130)for b in after.values()))
    t.run('align',{'axis':'top'});check('align top',len({round(b['y'],2)for b in snapshot(ids).values()})==1)
    t.run('spacing',{'axis':'horizontal','gap':17});bs=sorted(snapshot(ids).values(),key=lambda b:b['x'])
    check('net horizontal gaps',all(close(b['x']-a['x']-a['width'],17) for a,b in zip(bs,bs[1:])))
    choose(ids[:2]);before=snapshot(ids[:2]);t.run('swap');after=snapshot(ids[:2])
    check('swap preserves sizes',close(after[ids[0]]['x'],before[ids[1]]['x']) and close(after[ids[0]]['width'],before[ids[0]]['width']))
    t.run('copy_position',{'mode':'absolute'});saved=snapshot(ids[:2]);c.execute('object.move',dx=45,dy=20)
    t.run('paste_position');check('absolute position paste',near_boxes(saved,snapshot(ids[:2])))
    t.run('copy_position',{'mode':'relative'});saved=snapshot(ids[:2]);c.execute('object.move',dx=35,dy=25)
    current=snapshot(ids[:2]);c.execute('select.set',ids=[ids[1]]);c.execute('object.move',dx=10,dy=10);choose(ids[:2]);t.run('paste_position');after=snapshot(ids[:2])
    check('relative reference stays fixed',close(current[ids[0]]['x'],after[ids[0]]['x']))
    check('relative offsets restored',close(after[ids[1]]['x']-after[ids[0]]['x'],saved[ids[1]]['x']-saved[ids[0]]['x']))
    choose(ids);t.run('grid',{'columns':2,'gap_x':18,'gap_y':24})
    t.run('labels',{'font':'Arial','size':18,'dx':5,'dy':5})
    doc=c.inspect();label_ids=[n['id']for n in flatten(doc['layers']).values() if n.get('name','').startswith('[SCI label]')]
    check('editable labels created',len(label_ids)==4 and all(c.execute('document.node',id=i)['kind']['type']=='text' for i in label_ids))
    t.run('labels',{'style':'a)','size':16,'dx':6,'dy':6})
    label_ids2=[n['id']for n in flatten(c.inspect()['layers']).values() if n.get('name','').startswith('[SCI label]')]
    check('label updates do not duplicate',label_ids==label_ids2)
    texts={c.execute('document.node',id=id)['kind']['runs'][0]['text'] for id in label_ids}
    check('label text updated',texts=={'a)','b)','c)','d)'})
    t.run('borders',{'weight':1,'color':'#243c64'});t.run('borders',{'weight':1.5,'color':'#243c64'})
    borders=[n for n in flatten(c.inspect()['layers']).values()if n.get('name','').startswith('[SCI border]')]
    check('border update does not duplicate',len(borders)==4)
    check('borders unfilled',all(n.get('fill') in ('none','None',None) for n in borders))
    savedoc=out/'scientific-layout.vectorcraft'
    c.execute('document.save',path=str(savedoc),includeLinked=True)
    check('native document saved',savedoc.is_file() and savedoc.stat().st_size>100)
    export=c.execute('document.export',path=str(out/'scientific-layout.png'),format='png',scale=1.5)
    check('real PNG exported',(out/'scientific-layout.png').is_file())
    r=c.call('app.open',{'path':str(savedoc)})
    reopened=c.inspect();nodes=flatten(reopened['layers'])
    check('native reopen retains editable images and text',len([n for n in nodes.values()if n.get('kind')=='Image'])==4 and len([n for n in nodes.values()if n.get('name','').startswith('[SCI label]')])==4)
    choose(ids);t.run('labels',{'style':'A','size':16,'dx':5,'dy':5})
    check('label association survives save reopen',len([n for n in flatten(c.inspect()['layers']).values() if n.get('name','').startswith('[SCI label]')])==4)
    # Separate document exercises combinations, clipping, rotation and rollback.
    c.execute('file.new',title='Toolbox edge cases',width=720,height=500)
    a=c.execute('shape.rectangle',x=30,y=40,width=80,height=60)['id'];b=c.execute('shape.ellipse',x=130,y=40,width=50,height=60)['id']
    choose([a,b]);g=c.execute('object.group')['id']
    image=c.execute('file.place',name='edge.png',dataBase64=base64.b64encode(example_png()).decode(),at=[360,150])['ids'][0]
    choose([image]);clip=c.execute('object.maskImage')['id']
    choose([g]);c.execute('object.rotate',angle=27)
    choose([g,clip]);before=snapshot([g,clip]);t.run('grid',{'columns':2,'gap_x':25});after=snapshot([g,clip])
    check('rotated group and clipped image layout',close(after[clip]['x']-after[g]['x']-after[g]['width'],25))
    t.run('undo');check('group clip undo restores bounds',near_boxes(before,snapshot([g,clip])))
    hist=len(c.inspect()['history'])
    try:t.run('resize',{'width':-4});raise AssertionError('invalid dimensions accepted')
    except LayoutError:check('invalid dimensions make no edits',len(c.inspect()['history'])==hist)
    old=t.snapshot();choose([g])
    try:t.run('align',{'axis':'left'},expected_selection=[g,clip]);raise AssertionError('stale selection accepted')
    except LayoutError:check('stale selection rejected',True)
    c.execute('object.setProps',ids=[g],locked=True)
    try:t.run('grid');raise AssertionError('empty selection accepted')
    except LayoutError:check('locked or empty selection rejected',True)
    # An explicit engine failure inside the operation must undo all its partial changes.
    choose([clip]);before=snapshot([clip])
    try:
        with t.transaction('deliberate rollback check',[clip]):
            c.execute('object.move',dx=123,dy=50)
            c.execute('command.doesNotExist')
    except Exception:pass
    check('explicit refusal rolls back partial operation',near_boxes(before,snapshot([clip])))
    c.execute('document.save',path=str(out/'edge-cases.vectorcraft'))
    # Leave the useful example visible, rather than the edge-case test document.
    c.call('app.open',{'path':str(savedoc)})
    choose(ids)
    report={'version':'0.1.0','tested_at_utc':datetime.now(timezone.utc).isoformat(),'vectorcraft':'0.10.0',
            'backend':'desktop control','checks_passed':checks,'count':len(checks),
            'example_sha256':hashlib.sha256(savedoc.read_bytes()).hexdigest(),
            'limitations':['No OS-level manual pointer test','No docked in-app panel','No automatic label following','Only this Windows host and VectorCraft 0.10.0 were tested']}
    (out/'smoke-report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
    print(f'PASS ALL {len(checks)} CHECKS')


if __name__=='__main__':main()
