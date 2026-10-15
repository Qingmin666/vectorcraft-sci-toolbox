"""Full native migration checks in an explicitly selected disposable control session."""
from __future__ import annotations
import argparse
import base64
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))
from vectorcraft_sci.protocol import ControlClient, ControlError
from vectorcraft_sci.engine import flatten
from vectorcraft_sci.layout import Box, align_plan, spacing_plan, resize_plan, grid_plan
from smoke_desktop import example_png


def main():
    parser=argparse.ArgumentParser(); parser.add_argument('--edition',default='0.2.0-rc.1'); parser.add_argument('--port',type=int,required=True); parser.add_argument('--output',type=Path,required=True); args=parser.parse_args()
    out=args.output.resolve(); out.mkdir(parents=True,exist_ok=True); c=ControlClient(args.port); checks=[]
    def check(name,condition):
        if not condition: raise AssertionError(name)
        checks.append(name); print('PASS',name,flush=True)
    def run(operation,settings=None,**extra):return c.execute('ui.scientific',operation=operation,settings=settings or {},**extra)
    def choose(ids):c.execute('select.set',ids=ids)
    def boxes(ids):
        nodes=flatten(c.inspect()['layers']);return [Box(i,**nodes[i]['bounds']) for i in ids]
    def same(actual,expected):
        lookup={b.id:b for b in actual}
        return all(all(abs(getattr(lookup[b.id],k)-getattr(b,k))<.08 for k in ('x','y','width','height')) for b in expected)
    def related(role):return [n['id'] for n in flatten(c.inspect()['layers']).values() if n.get('name','').startswith(f'[SCI {role}]')]
    def reject(name,action):
        before=c.inspect()
        try:action();raise AssertionError('operation unexpectedly accepted')
        except ControlError:pass
        after=c.inspect();check(name,before['layers']==after['layers'] and before['history']==after['history'])
    c.execute('file.new',title='原生科研功能验证',width=620,height=440)
    ids=[]
    for i,(x,y,w,h) in enumerate([(40,40,160,110),(310,65,110,110),(60,250,150,80),(350,260,160,100)]):
        ids.append(c.execute('file.place',name=f'Fixture-{i+1}.png',dataBase64=base64.b64encode(example_png(w,h,i*20)).decode(),at=[x+w/2,y+h/2],link=False)['ids'][0])
    choose(ids)
    before=boxes(ids); expected=resize_plan(before,130,None,True); run('resize',{'width':'130','height':'','keep_ratio':True})
    check('individual proportional resize',same(boxes(ids),expected));c.execute('edit.undo');check('resize undo',same(boxes(ids),before));c.execute('edit.redo')
    expected=resize_plan(boxes(ids),120,90,False);run('resize',{'width':'120','height':'90','keep_ratio':False});check('independent width and height',same(boxes(ids),expected))
    reject('ratio rejects two dimensions without edits',lambda:run('resize',{'width':'120','height':'90','keep_ratio':True}))
    before=boxes(ids);expected=grid_plan(resize_plan(before,100,None,True),3,13,19,'rows');run('grid',{'columns':3,'gap_x':13,'gap_y':19,'width':'100','height':'','keep_ratio':True,'order':'rows'});check('combined resize and grid',same(boxes(ids),expected))
    run('configure',{'width':'','height':''})
    for reference in ('selection','first'):
        for axis in ('left','center','right','top','middle','bottom'):
            before=boxes(ids);expected=align_plan(before,axis,reference);run('align',{'align_first':reference=='first'},axis=axis);check(f'align {reference} {axis}',same(boxes(ids),expected));c.execute('edit.undo')
    for axis in ('horizontal','vertical'):
        before=boxes(ids);expected=spacing_plan(before,axis,17);run('spacing',{'spacing':17},axis=axis);check(f'{axis} net spacing',same(boxes(ids),expected));c.execute('edit.undo')
    before=boxes(ids);run('copy_position',{'relative_position':False,'order':'selection'});c.execute('object.transform',ids=ids,matrix=[1,0,0,1,30,50]);run('paste_position');check('absolute position clipboard',same(boxes(ids),before))
    run('copy_position',{'relative_position':True,'order':'selection'});c.execute('object.transform',ids=ids,matrix=[1,0,0,1,20,25]);c.execute('object.transform',ids=[ids[1]],matrix=[1,0,0,1,30,0]);run('paste_position');expected=[Box(b.id,b.x+20,b.y+25,b.width,b.height) for b in before];check('relative clipboard preserves target reference',same(boxes(ids),expected))
    choose(ids[:2]);reject('clipboard count mismatch makes no edits',lambda:run('paste_position'))
    before=boxes(ids[:2]);run('swap');expected=[Box(before[0].id,before[1].x,before[1].y,before[0].width,before[0].height),Box(before[1].id,before[0].x,before[0].y,before[1].width,before[1].height)];check('swap preserves individual dimensions',same(boxes(ids[:2]),expected));c.execute('edit.undo');choose(ids)
    reject('swap rejects wrong selection count',lambda:run('swap'))
    run('grid',{'columns':2,'gap_x':18,'gap_y':35,'order':'rows'}); expected_ids=None
    for style,wanted in [('A',{'Z','AA','AB','AC'}),('a',{'z','aa','ab','ac'}),('A)',{'Z)','AA)','AB)','AC)'}),('a)',{'z)','aa)','ab)','ac)'}),('1',{'26','27','28','29'}),('1)',{'26)','27)','28)','29)'})]:
        run('labels',{'label_style':style,'label_start':26,'label_size':16,'label_color':'#f5f5f5','font':'Arial','label_anchor':'top-left','dx':4,'dy':4})
        label_ids=related('label');texts={c.execute('document.node',id=i)['kind']['runs'][0]['text'] for i in label_ids};check(f'label format {style}',texts==wanted)
        if expected_ids is None:expected_ids=sorted(label_ids)
        else:check(f'label format {style} retains IDs',sorted(label_ids)==expected_ids)
    for anchor in ('top-left','top-right','bottom-left','bottom-right'):
        run('labels',{'label_style':'A','label_start':1,'label_anchor':anchor,'dx':5,'dy':7})
        sources={b.id:b for b in boxes(ids)}; good=True
        for label_id in related('label'):
            detail=c.execute('document.node',id=label_id); data=dict(detail['attrs']['data']); source=sources[int(data['sci-target'])]; b=Box(label_id,**c.execute('document.node',id=label_id,summary=True)['bounds'])
            expected_x=source.right-b.width+5 if anchor.endswith('right') else source.x+5;expected_y=source.bottom-b.height+7 if anchor.startswith('bottom') else source.y+7
            good=good and abs(b.x-expected_x)<.08 and abs(b.y-expected_y)<.08
        check(f'label anchor {anchor}',good)
    style=c.execute('document.node',id=related('label')[0])['kind']['runs'][0]['style'];check('native label color and font size',abs(style['fill']['color']['r']-245/255)<.01 and style['size']==16)
    reject('invalid label color makes no edits',lambda:run('labels',{'label_color':'invalid'}))
    run('borders',{'border_weight':1,'border_color':'#243c64'}); borders=related('border');run('borders',{'border_weight':2});check('border updates retain IDs',sorted(borders)==sorted(related('border')) and len(borders)==4)
    check('native editable border paint',all(c.execute('document.node',id=i,summary=True).get('fill') in ('none','None',None) and abs(c.execute('document.node',id=i,summary=True)['strokeWidth']-2)<.01 for i in borders))
    run('preset_save',{'preset_name':'科研常用','columns':3,'gap_x':17,'label_style':'a)'})
    saved=run('inspect')['settings'];run('configure',{'columns':7,'gap_x':99});run('preset_load',{'preset_name':'科研常用'});check('named preset restores parameters',run('inspect')['settings']==saved)
    run('preset_save',{'preset_name':'临时预设'});run('preset_delete',{'preset_name':'临时预设'});check('named preset delete',run('inspect')['presets']==['科研常用'])
    run('configure',{'width':'','height':'','columns':2,'gap_x':18,'gap_y':35,'order':'rows','label_style':'A','label_start':1,'label_color':'#ffffff','label_anchor':'top-left','label_size':18,'dx':4,'dy':4});run('grid');run('labels');run('borders')
    native=out/'native-scientific-full.vectorcraft';c.execute('document.save',path=str(native),includeLinked=True);c.call('app.open',{'path':str(native)});choose(ids);run('labels');run('borders');check('saved associations update labels and borders',len(related('label'))==4 and len(related('border'))==4);c.execute('document.save',path=str(native),includeLinked=True)
    c.execute('document.export',path=str(out/'native-scientific-full.png'),format='png',scale=1.5)
    c.execute('file.new',title='原生科研边界验证',width=720,height=500)
    a=c.execute('shape.rectangle',x=30,y=40,width=80,height=60)['id'];b=c.execute('shape.ellipse',x=130,y=40,width=50,height=60)['id'];choose([a,b]);group=c.execute('object.group')['id']
    image=c.execute('file.place',name='clip.png',dataBase64=base64.b64encode(example_png()).decode(),at=[360,150],link=False)['ids'][0];choose([image]);clip=c.execute('object.maskImage')['id'];choose([group]);c.execute('object.rotate',angle=27);choose([group,clip]);before=boxes([group,clip]);expected=grid_plan(before,2,25,12,'rows');run('grid',{'columns':2,'gap_x':25,'gap_y':12,'width':'','height':''});check('rotated group and clipped image layout',same(boxes([group,clip]),expected));c.execute('edit.undo');check('group clip undo',same(boxes([group,clip]),before))
    reject('negative dimension makes no edits',lambda:run('resize',{'width':'-4'}));choose([group]);c.execute('object.setProps',ids=[group],locked=True);reject('locked or empty selection refused',lambda:run('grid',{'width':''}))
    c.execute('document.save',path=str(out/'native-edge-cases.vectorcraft'));c.call('app.open',{'path':str(native)});choose(ids);run('configure',{'width':'','height':'','columns':2,'gap_x':18,'gap_y':35,'label_style':'A','label_anchor':'top-left','label_color':'#ffffff','label_size':18});c.execute('window.panel',panel='science')
    report={'edition':args.edition,'checks':checks,'passed':len(checks),'scope':'native feature migration on one Windows host; publication is separate'};(out/'full-report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8');print('PASS ALL',len(checks))


if __name__=='__main__':main()
