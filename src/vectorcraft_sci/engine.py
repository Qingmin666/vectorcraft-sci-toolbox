"""Validated operations built exclusively from documented VectorCraft commands."""
from __future__ import annotations

from contextlib import contextmanager
from dataclasses import asdict
import re
import threading

from .layout import Box, LayoutError, number, integer, union, ordered, resize_plan, grid_plan, align_plan, spacing_plan, matrix, label_text
from .protocol import ControlClient, ControlError


def flatten(nodes, blocked=False, ancestors=()):
    result={}
    for node in nodes:
        hidden=blocked or node.get("locked",False) or not node.get("visible",True) or node.get("template",False)
        n=dict(node);n["blocked"]=hidden;n["ancestors"]=ancestors
        result[n["id"]]=n
        result.update(flatten(n.get("children",[]),hidden,(*ancestors,n["id"])))
    return result


class Toolbox:
    def __init__(self, client: ControlClient):
        self.client=client
        self.lock=threading.RLock()
        self.clipboard=None
        self.uncertain=False

    def snapshot(self):
        ui=self.client.ui()
        if ui.get("activeDocument") is None:
            return {"connected":True,"desktop":True,"document":None,"selection":[],"revision":None,
                    "uncertain":self.uncertain,"clipboard":self.clipboard is not None}
        doc=self.client.inspect()
        tree=flatten(doc.get("layers",[]))
        selection=[]
        for id in doc.get("selection",[]):
            node=tree.get(id,{})
            selection.append({"id":id,"name":node.get("name",""),"kind":node.get("kind",""),
                              "bounds":node.get("bounds"),"blocked":node.get("blocked",False)})
        return {"connected":True,"desktop":True,"document":doc.get("title"),"path":doc.get("path"),
                "revision":doc.get("revision"),"selection":selection,"dirty":doc.get("dirty"),
                "history":doc.get("history",[])[-5:],"uncertain":self.uncertain,
                "clipboard":self.clipboard is not None,"documentIndex":ui.get("activeDocument")}

    def selection(self, doc):
        if doc.get("isolation") is not None:
            raise LayoutError("请先退出隔离编辑，再运行工具箱")
        nodes=flatten(doc.get("layers",[]))
        ids=doc.get("selection",[])
        if not ids: raise LayoutError("请先在 VectorCraft 专用窗口中选择对象")
        if len(ids)>200: raise LayoutError("单次最多处理 200 个对象")
        boxes=[]
        for id in ids:
            n=nodes.get(id)
            if n is None or n.get("blocked") or n.get("kind")=="Layer":
                raise LayoutError("选择中包含图层、隐藏或锁定对象，请重新选择")
            if any(a in ids for a in n["ancestors"]):
                raise LayoutError("不能同时处理组合和它内部的对象")
            b=n.get("bounds")
            if not b or not all(k in b for k in ("x","y","width","height")):
                raise LayoutError("对象没有可用边界，请排除空对象")
            boxes.append(Box(id,**{k:float(b[k]) for k in ("x","y","width","height")},
                             kind=n.get("kind",""),name=n.get("name","")))
        return boxes,nodes

    @contextmanager
    def transaction(self,label,selection):
        self.client.execute("edit.undoGroup.begin",label=label)
        try:
            yield
        except Exception as exc:
            # Only an explicit engine refusal is known not to have changed the rejected step.
            uncertain=isinstance(exc,ControlError) and any(s in str(exc) for s in ("可能","响应编号"))
            if uncertain:
                self.uncertain=True
                raise ControlError("连接中断，结果不确定。请检查 VectorCraft 画稿与撤销记录；工具箱已暂停编辑。") from exc
            try:
                self.client.execute("edit.undoGroup.end",cancel=True)
                self.client.execute("select.set",ids=selection)
            except Exception:
                self.uncertain=True
                raise ControlError("操作失败且无法确认回滚结果，请检查画稿并重新连接") from exc
            raise
        else:
            try:
                self.client.execute("select.set",ids=selection)
                self.client.execute("edit.undoGroup.end")
            except Exception as exc:
                self.uncertain=True
                raise ControlError("编辑已执行，但无法确认撤销分组关闭。请检查画稿；工具箱已暂停编辑。") from exc

    def transform(self,before,after,scale_strokes=False):
        lookup={b.id:b for b in before}
        for b in after:
            self.client.execute("object.transform",ids=[b.id],matrix=matrix(lookup[b.id],b),
                                strokes=scale_strokes,corners=False,patterns=True)

    def _box(self,id):
        node=flatten(self.client.inspect().get("layers",[])).get(id)
        if not node or not node.get("bounds"): raise LayoutError("无法读取对象边界")
        return Box(id,**node["bounds"],kind=node.get("kind",""),name=node.get("name",""))

    def _place_label(self,id,b,anchor,dx,dy):
        current=self._box(id)
        if anchor not in ("top-left","top-right","bottom-left","bottom-right"):
            raise LayoutError("未知标签锚点")
        x=b.right-current.width if "right" in anchor else b.x
        y=b.bottom-current.height if "bottom" in anchor else b.y
        dest=Box(id,x+dx,y+dy,current.width,current.height)
        self.transform([current],[dest])

    def run(self,operation,options=None,expected_revision=None,expected_document=None,expected_selection=None,expected_title=None):
        options=options or {}
        with self.lock:
            if operation=="acknowledge":
                # The user explicitly checked the uncertain edit; close any pending group.
                self.client.execute("edit.undoGroup.end")
                self.uncertain=False
                return {"message":"已恢复编辑，请重新选择对象","state":self.snapshot()}
            if self.uncertain: raise LayoutError("请先检查画稿，再点击『已检查画稿，恢复编辑』")
            ui=self.client.ui()
            if ui.get("activeDocument") is None: raise LayoutError("请先在 VectorCraft 中新建或打开文档")
            doc=self.client.inspect()
            if expected_document is not None and expected_document!=ui.get("activeDocument"):
                raise LayoutError("活动文档已切换，请刷新选择后再操作")
            if expected_revision is not None and expected_revision!=doc.get("revision"):
                raise LayoutError("画稿已改变，请先刷新选择后再操作")
            if expected_title is not None and expected_title!=doc.get("title"):
                raise LayoutError("活动画稿已改变，请刷新选择后再操作")
            if expected_selection is not None and expected_selection!=doc.get("selection",[]):
                raise LayoutError("所选对象已改变，请刷新选择后再操作")
            if operation in ("undo","redo"):
                self.client.execute("edit."+operation)
                return {"message":"已撤销" if operation=="undo" else "已重做","state":self.snapshot()}
            boxes,nodes=self.selection(doc)
            ids=[b.id for b in boxes]
            if operation=="copy_position":
                mode=options.get("mode","absolute")
                if mode not in ("absolute","relative"): raise LayoutError("未知位置复制方式")
                boxes=ordered(boxes,options.get("order","selection"))
                if mode=="relative" and len(boxes)<2: raise LayoutError("相对位置复制：先选参考图，再选要记录的对象（至少两个）")
                self.clipboard={"mode":mode,"boxes":[asdict(b) for b in boxes]}
                return {"message":f"已记录 {len(boxes)} 个对象的{('相对' if mode=='relative' else '绝对')}位置","state":self.snapshot()}
            plan=None
            if operation=="grid":
                source=boxes
                if options.get("width") or options.get("height"):
                    source=resize_plan(boxes,options.get("width"),options.get("height"),options.get("keep_ratio",True))
                plan=grid_plan(source,options.get("columns",3),options.get("gap_x",12),options.get("gap_y",12),options.get("order","rows"))
            elif operation=="resize":
                plan=resize_plan(boxes,options.get("width"),options.get("height"),options.get("keep_ratio",True))
            elif operation=="align":
                plan=align_plan(boxes,options.get("axis","left"),options.get("anchor","selection"))
            elif operation=="spacing":
                plan=spacing_plan(boxes,options.get("axis","horizontal"),options.get("gap",12))
            elif operation=="swap":
                if len(boxes)!=2: raise LayoutError("交换位置需要正好选择两个对象")
                a,b=boxes
                plan=[Box(a.id,b.x,b.y,a.width,a.height),Box(b.id,a.x,a.y,b.width,b.height)]
            elif operation=="paste_position":
                if not self.clipboard: raise LayoutError("请先复制位置")
                saved=[Box(**b) for b in self.clipboard["boxes"]]
                if len(saved)!=len(boxes): raise LayoutError(f"请选择 {len(saved)} 个目标对象，数量应与复制时一致")
                boxes=ordered(boxes,options.get("order","selection"))
                dx=dy=0
                if self.clipboard["mode"]=="relative":
                    dx,dy=boxes[0].x-saved[0].x,boxes[0].y-saved[0].y
                plan=[Box(b.id,s.x+dx,s.y+dy,b.width,b.height) for b,s in zip(boxes,saved)]
            elif operation not in ("labels","borders"):
                raise LayoutError("未知工具箱操作")
            if plan is not None:
                before={b.id:b for b in boxes}
                unchanged=all(all(abs(getattr(before[b.id],key)-getattr(b,key))<1e-8
                                  for key in ("x","y","width","height")) for b in plan)
                if unchanged:
                    return {"message":"对象已符合当前设置，未产生编辑或撤销记录","state":self.snapshot()}
            # Validate parameters before opening a group or modifying objects.
            if operation=="labels":
                boxes=ordered(boxes,options.get("order","rows"))
                size=number(options.get("size",14),"字号",1,300)
                start=integer(options.get("start",1),"起始编号",1,9999)
                style=options.get("style","A");label_text(start+len(boxes)-1,style)
                dx=number(options.get("dx",4),"横向偏移");dy=number(options.get("dy",4),"纵向偏移")
                color=options.get("color","#000000")
                if not re.fullmatch(r"#[0-9a-fA-F]{6}",color): raise LayoutError("颜色应为六位十六进制色值")
                anchor=options.get("anchor","top-left")
                if anchor not in ("top-left","top-right","bottom-left","bottom-right"):raise LayoutError("未知标签锚点")
                font=str(options.get("font","Arial")).strip()
                if not font or len(font)>120:raise LayoutError("请输入有效字体名称")
                if any(b.name.startswith("[SCI ") for b in boxes):raise LayoutError("请只选择原图或原始对象，不要同时选择工具箱标签或边框")
                existing={}
                for n in nodes.values():
                    if n.get("name","").startswith("[SCI label]"):
                        data=dict(self.client.execute("document.node",id=n["id"]).get("attrs",{}).get("data",{}))
                        target=data.get("sci-target")
                        if target:
                            if n.get("blocked"):raise LayoutError("已有工具箱标签被锁定或隐藏，请先解锁并显示")
                            existing[str(target)]=n["id"]
            if operation=="borders":
                weight=number(options.get("weight",1),"边框线宽",.01,100)
                color=options.get("color","#000000")
                if not re.fullmatch(r"#[0-9a-fA-F]{6}",color): raise LayoutError("边框颜色无效")
                if any(b.name.startswith("[SCI ") for b in boxes):raise LayoutError("请只选择原始对象")
                border_ids={}
                for n in nodes.values():
                    if n.get("name","").startswith("[SCI border]"):
                        data=dict(self.client.execute("document.node",id=n["id"]).get("attrs",{}).get("data",{}))
                        if data.get("sci-target"):
                            if n.get("blocked"):raise LayoutError("已有工具箱边框被锁定或隐藏，请先解锁并显示")
                            border_ids[str(data["sci-target"])]=n["id"]
            labels={"grid":"科研网格排图","resize":"统一对象尺寸","align":"科研对齐","spacing":"科研等间距",
                    "swap":"交换对象位置","paste_position":"粘贴对象位置","labels":"添加或更新子图标签","borders":"添加或更新图片边框"}
            with self.transaction(labels[operation],ids):
                if plan is not None:
                    self.transform(boxes,plan,options.get("scale_strokes",False))
                elif operation=="labels":
                    for i,b in enumerate(boxes):
                        text=label_text(start+i,style)
                        id=existing.get(str(b.id))
                        if id is None:
                            id=self.client.execute("text.create",x=b.x,y=b.y,text=text,size=size,font=font,color=color)["id"]
                            self.client.execute("object.setProps",ids=[id],name=f"[SCI label] {b.id}",data={"sci-target":str(b.id),"sci-role":"label"})
                        else:
                            self.client.execute("text.setText",ids=[id],text=text)
                        self.client.execute("text.setStyle",ids=[id],font=font,size=size,fill=color)
                        self._place_label(id,b,anchor,dx,dy)
                elif operation=="borders":
                    for b in boxes:
                        id=border_ids.get(str(b.id))
                        if id is None:
                            id=self.client.execute("shape.rectangle",x=b.x,y=b.y,width=b.width,height=b.height)["id"]
                            self.client.execute("object.setProps",ids=[id],name=f"[SCI border] {b.id}",data={"sci-target":str(b.id),"sci-role":"border"})
                        else:
                            current=self._box(id)
                            self.transform([current],[Box(id,b.x,b.y,b.width,b.height)])
                        self.client.execute("paint.setFill",ids=[id],none=True)
                        self.client.execute("paint.setStroke",ids=[id],color=color)
                        self.client.execute("stroke.set",ids=[id],weight=weight,align="inside")
            return {"message":f"{labels[operation]}完成：{len(boxes)} 个对象，可一次撤销","state":self.snapshot()}
