"""RC2 integration checks using only synthetic art in an owned native session."""
from __future__ import annotations
import argparse, base64, json, sys, time
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'src'))
from vectorcraft_sci.protocol import ControlClient, ControlError
from vectorcraft_sci.engine import flatten
from smoke_desktop import example_png

def main():
    p=argparse.ArgumentParser();p.add_argument('--port',type=int,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
    out=a.output.resolve();out.mkdir(parents=True,exist_ok=True);c=ControlClient(a.port);checks=[]
    def check(name,ok):
        if not ok:raise AssertionError(name)
        checks.append(name);print('PASS',name,flush=True)
    def run(op,settings=None,**kw):return c.execute('ui.scientific',operation=op,settings=settings or {},**kw)
    def choose(ids):c.execute('select.set',ids=ids)
    def detail(i):return c.execute('document.node',id=i)
    def attrs(i):return dict((detail(i).get('attrs') or {}).get('data') or [])
    def nodes():return flatten(c.inspect()['layers'])
    def role(r):return [i for i in nodes() if attrs(i).get('sci-role')==r]
    def box(i):return c.execute('document.node',id=i,summary=True)['bounds']
    def reject(name,f):
        before=c.inspect()
        try:f()
        except ControlError:pass
        else:raise AssertionError(name+' accepted')
        after=c.inspect();check(name,after['layers']==before['layers'] and after['history']==before['history'])
    def image(x=90,y=90):
        return c.execute('file.place',name='Synthetic.png',dataBase64=base64.b64encode(example_png(160,100)).decode(),at=[x,y],link=False)['ids'][0]
    c.execute('file.new',title='RC2 科研完整联测',width=900,height=650)
    source=image();choose([source]);before=c.inspect();lib=run('palette_inspect')
    check('23 traceable palettes',len(lib['palettes'])==23)
    run('palette_group_save',name='QA');run('palette_save',group='QA',name='Test',colors=['#123456','#ABCDEF'])
    check('custom palette persists in native state',any(p['name']=='Test' for p in run('palette_inspect')['palettes']))
    run('palette_save',group='QA',name='Edited',oldGroup='QA',oldName='Test',colors=['#F00','#0F0'])
    check('palette editing normalizes HEX',next(p for p in run('palette_inspect')['palettes'] if p['name']=='Edited')['colors']==['#FF0000','#00FF00'])
    reject('palette fill rejects raster without editing',lambda:run('palette_apply',color='#123456'))
    run('palette_group_save',oldName='QA',name='QA renamed');run('palette_delete',group='QA renamed',name='Edited');run('palette_group_delete',name='QA renamed')
    check('palette CRUD leaves drawing and undo untouched',c.inspect()['layers']==before['layers'] and c.inspect()['history']==before['history'])
    shape=c.execute('shape.rectangle',x=260,y=40,width=100,height=65)['id'];choose([shape]);run('palette_apply',color='#123456')
    check('explicit palette fill targets path',c.execute('document.node',id=shape,summary=True)['fill'].lower()=='#123456')
    c.execute('edit.undo');choose([source]);run('copy_size');s=run('inspect')['settings'];check('copy individual dimensions',abs(float(s['width'])-160)<.1 and abs(float(s['height'])-100)<.1)
    run('configure',{'width':'','height':''});run('labels',{'label_style':'(A)','label_bold':True,'label_color':'#ffffff'})
    labels=role('label');check('parenthesized bold native labels',len(labels)==1 and detail(labels[0])['kind']['runs'][0]['text']=='(A)')
    choose(labels);run('renumber_labels',{'label_style':'(a)','label_start':3});check('renumber selected native text',detail(labels[0])['kind']['runs'][0]['text']=='(c)')
    choose([source]);run('borders',{'border_dash':4,'border_group':True});check('grouped dashed border created',len(role('border'))==1 and len(role('border-wrapper'))==1)
    run('labels',{'label_style':'A','label_start':1});check('labels resolve annotated source',len(role('label'))==1)
    choose([source]);run('scale_apply',{'scale':{'fov_width':'160','fov_height':'100','length':20,'unit':'um','auto_group':False}})
    check('explicit physical calibration persisted',json.loads(attrs(source)['sci-fov'])=={'width':160.0,'height':100.0})
    check('editable scale group created',len(role('scale'))==1)
    reject('oversized scale fails atomically',lambda:run('scale_apply',{'scale':{'length':2000}}))
    run('scale_apply',{'scale':{'length':.02,'unit':'mm'}});check('unit-equivalent scale updates without duplication',len(role('scale'))==1)
    run('scale_load');check('stored FOV loads into UI',float(run('inspect')['settings']['scale']['fov_width'])==160)
    reject('PNG metadata never guesses calibration',lambda:run('scale_read_metadata'))
    run('zoom_apply',{'zoom':{'regions':[{'name':'A','x':.2,'y':.2,'width':.3,'height':.3,'color':'#ff6b35','scale_length':'5','scale_unit':'um'},{'name':'B','x':.6,'y':.5,'width':.2,'height':.3,'color':'#00bfff','rotation':10}],'magnification':2,'auto_update':True,'inherit_scale':True}})
    records=json.loads(attrs(source)['sci-zoom-records']);check('two editable ROI outputs and markers',len(records)==2 and all(r['output'] in nodes() and r['marker'] in nodes() for r in records))
    output_ids=[r['output'] for r in records]
    r=records[0];fov=json.loads(attrs(r['output'])['sci-fov']);check('ROI inherits cropped physical FOV',abs(fov['width']-48)<.1 and abs(fov['height']-30)<.1)
    run('zoom_load');settings=run('inspect')['settings']['zoom'];settings['regions'][0]['x']=.25
    run('zoom_apply',{'zoom':settings});check('ROI edit retains output identities',[r['output'] for r in json.loads(attrs(source)['sci-zoom-records'])]==output_ids)
    before=c.inspect()['layers'];c.execute('object.transform',ids=[source],matrix=[1,0,0,1,8,6]);time.sleep(.4)
    check('automatic ROI sync follows source movement',abs(json.loads(attrs(source)['sci-zoom-records'])[0]['source_frame'][0]-records[0]['source_frame'][0]-8)<.1)
    c.execute('edit.undo');time.sleep(.25);check('one undo restores source and linked ROI',c.inspect()['layers']==before);c.execute('edit.redo');time.sleep(.25)
    choose([source]);run('pseudocolor_apply',{'pseudocolor':{'lut':'red','keep_original':False}});pseudo=c.inspect()['selection'][0]
    check('pseudocolor preserves original pixel node',source in nodes() and pseudo!=source)
    check('pseudocolor transfers ROI association',len(json.loads(attrs(pseudo)['sci-zoom-records']))==2 and 'sci-zoom-records' not in attrs(source))
    choose([pseudo]);run('zoom_sync');run('scale_load');check('pseudocolor preserves physical calibration',float(run('inspect')['settings']['scale']['fov_width'])==160)
    run('pseudocolor_apply',{'pseudocolor':{'lut':'green'}});check('pseudocolor recolor retains group ID',c.inspect()['selection']==[pseudo])
    native=out/'native-rc2-combined.vectorcraft';c.execute('document.save',path=str(native),includeLinked=True);c.call('app.open',{'path':str(native)});choose([pseudo]);run('zoom_sync')
    check('save reopen retains linked ROI identity',[r['output'] for r in json.loads(attrs(pseudo)['sci-zoom-records'])]==output_ids)
    c.execute('document.export',path=str(out/'native-rc2-combined.png'),format='png',scale=1)
    check('combined native file and PNG export exist',native.is_file() and (out/'native-rc2-combined.png').stat().st_size>1000)
    c.execute('file.new',title='RC2 Screen 通道联测',width=700,height=400);a1=image(110,90);a2=image(300,90);choose([a1,a2]);rows=run('channels_read')
    for row,lut in zip(rows,['red','green']):row['lut']=lut
    run('channels_set',channels=rows);run('channels_merge');merged=c.inspect()['selection'][0]
    check('Screen merge keeps original images',a1 in nodes() and a2 in nodes() and merged not in [a1,a2])
    c.execute('edit.undo');choose([a1,a2]);run('channels_read');c.execute('object.transform',ids=[a1],matrix=[1,0,0,1,1,0]);reject('stale channel list refuses edits',lambda:run('channels_merge'))
    c.execute('document.save',path=str(out/'native-rc2-channels.vectorcraft'),includeLinked=True)
    from PIL import Image, TiffImagePlugin
    c.execute('file.new',title='RC2 TIFF 标定联测',width=500,height=400)
    tiff=out/'synthetic-calibrated.tif';tags=TiffImagePlugin.ImageFileDirectory_v2();tags[270]='ImageJ=1.54\nunit=um';tags[282]=2.;tags[283]=4.
    Image.new('RGB',(160,100),(100,130,160)).save(tiff,tiffinfo=tags)
    linked=c.execute('file.place',path=str(tiff),at=[160,100],link=True)['ids'][0];choose([linked]);run('scale_read_metadata');ts=run('inspect')['settings']['scale']
    check('real linked TIFF reads ImageJ calibration',abs(float(ts['fov_width'])-80)<.01 and abs(float(ts['fov_height'])-25)<.01)
    plain=out/'synthetic-dpi-only.tif';Image.new('RGB',(160,100),(100,130,160)).save(plain,dpi=(300,300))
    dpi=c.execute('file.place',path=str(plain),at=[300,200],link=True)['ids'][0];choose([dpi]);reject('real linked TIFF DPI is not microscopy calibration',lambda:run('scale_read_metadata'))
    report={'edition':'0.2.0-rc.2','passed':len(checks),'checks':checks,'scope':'synthetic owned native desktop session'}
    (out/'complete-report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8');print('PASS ALL',len(checks))

if __name__=='__main__':main()
