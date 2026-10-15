"""Pure layout calculations in VectorCraft's point coordinates (y down)."""
from __future__ import annotations

from dataclasses import dataclass
import math


class LayoutError(ValueError):
    pass


def number(value, name: str, low=-100000, high=100000) -> float:
    try:
        result = float(value)
    except (ValueError, TypeError):
        raise LayoutError(f"{name}必须为数字") from None
    if not math.isfinite(result) or not low <= result <= high:
        raise LayoutError(f"{name}必须介于 {low:g} 和 {high:g} 之间")
    return result


def integer(value, name, low, high):
    result = number(value, name, low, high)
    if result != int(result):
        raise LayoutError(f"{name}必须为整数")
    return int(result)


@dataclass(frozen=True)
class Box:
    id: int
    x: float
    y: float
    width: float
    height: float
    kind: str = ""
    name: str = ""

    @property
    def right(self):
        return self.x + self.width

    @property
    def bottom(self):
        return self.y + self.height


def union(boxes):
    if not boxes:
        raise LayoutError("请先在 VectorCraft 中选择对象")
    x, y = min(b.x for b in boxes), min(b.y for b in boxes)
    return Box(-1, x, y, max(b.right for b in boxes)-x,
               max(b.bottom for b in boxes)-y)


def ordered(boxes, order="rows"):
    if order == "selection":
        return list(boxes)
    if order == "horizontal":
        return sorted(boxes, key=lambda b: (b.x, b.y))
    if order == "vertical":
        return sorted(boxes, key=lambda b: (b.y, b.x))
    if order != "rows":
        raise LayoutError("未知排序方式")
    # Cluster by centre y with a scale derived from each pair's smaller height.
    rows = []
    for box in sorted(boxes, key=lambda b: (b.y + b.height/2, b.x)):
        cy = box.y + box.height/2
        row = next((r for r in rows if abs(cy-r[0]) <= .45*min(box.height,r[1])), None)
        if row is None:
            rows.append([cy, box.height, [box]])
        else:
            row[2].append(box)
    return [b for row in rows for b in sorted(row[2], key=lambda b: b.x)]


def resize_plan(boxes, width=None, height=None, keep_ratio=True):
    width = None if width in (None, "") else number(width, "宽度", .01, 100000)
    height = None if height in (None, "") else number(height, "高度", .01, 100000)
    if width is None and height is None:
        raise LayoutError("至少填写宽度或高度")
    if keep_ratio and width is not None and height is not None:
        raise LayoutError("保持比例时只填写宽度或高度其中一项")
    out = []
    for b in boxes:
        if b.width <= 1e-7 or b.height <= 1e-7:
            raise LayoutError("零宽或零高对象不能缩放")
        w, h = width or b.width, height or b.height
        if keep_ratio:
            if width is not None:
                h = b.height * w/b.width
            else:
                w = b.width * h/b.height
        out.append(Box(b.id, b.x, b.y, w, h, b.kind, b.name))
    return out


def grid_plan(boxes, columns=3, gap_x=12, gap_y=12, order="rows"):
    columns = integer(columns, "列数", 1, 100)
    gap_x, gap_y = number(gap_x, "列间距", 0), number(gap_y, "行间距", 0)
    items = ordered(boxes, order)
    origin = union(items)
    cols = min(columns, len(items))
    widths = [max(b.width for b in items[i::cols]) for i in range(cols)]
    heights = [max(b.height for b in items[i:i+cols]) for i in range(0,len(items),cols)]
    xs = [origin.x + sum(widths[:i]) + gap_x*i for i in range(cols)]
    ys = [origin.y + sum(heights[:i]) + gap_y*i for i in range(len(heights))]
    return [Box(b.id, xs[i%cols], ys[i//cols], b.width, b.height, b.kind, b.name)
            for i,b in enumerate(items)]


def align_plan(boxes, axis, anchor="selection"):
    if axis not in ("left","center","right","top","middle","bottom"):
        raise LayoutError("未知对齐方向")
    ref = boxes[0] if anchor == "first" else union(boxes)
    out=[]
    for b in boxes:
        x,y=b.x,b.y
        if axis == "left": x=ref.x
        if axis == "center": x=ref.x+(ref.width-b.width)/2
        if axis == "right": x=ref.right-b.width
        if axis == "top": y=ref.y
        if axis == "middle": y=ref.y+(ref.height-b.height)/2
        if axis == "bottom": y=ref.bottom-b.height
        out.append(Box(b.id,x,y,b.width,b.height,b.kind,b.name))
    return out


def spacing_plan(boxes, axis, gap):
    if len(boxes)<2: raise LayoutError("等间距需要至少两个对象")
    if axis not in ("horizontal","vertical"): raise LayoutError("未知分布方向")
    gap=number(gap,"间距",0)
    items=ordered(boxes,axis)
    cursor=items[0].x if axis=="horizontal" else items[0].y
    out=[]
    for b in items:
        out.append(Box(b.id,cursor if axis=="horizontal" else b.x,
                       b.y if axis=="horizontal" else cursor,b.width,b.height,b.kind,b.name))
        cursor+=(b.width if axis=="horizontal" else b.height)+gap
    return out


def matrix(source: Box, target: Box):
    if source.width <= 1e-7 or source.height <= 1e-7:
        if source.width != target.width or source.height != target.height:
            raise LayoutError("零宽或零高对象不能缩放")
        return [1,0,0,1,target.x-source.x,target.y-source.y]
    sx,sy=target.width/source.width,target.height/source.height
    return [sx,0,0,sy,target.x-sx*source.x,target.y-sy*source.y]


def label_text(index, style):
    index=integer(index,"编号",1,9999)
    if style in ("A","a","A)","a)"):
        n=index; letters=""
        while n:
            n,r=divmod(n-1,26);letters=chr(65+r)+letters
        if style.startswith("a"): letters=letters.lower()
        return letters + (")" if style.endswith(")") else "")
    if style in ("1","1)"): return str(index)+(")" if style.endswith(")") else "")
    raise LayoutError("未知编号格式")
