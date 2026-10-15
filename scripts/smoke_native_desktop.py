"""Validate the native panel in an explicitly selected disposable desktop session."""
from __future__ import annotations

import argparse
import base64
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))
from vectorcraft_sci.protocol import ControlClient, ControlError
from vectorcraft_sci.engine import flatten
from smoke_desktop import example_png


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int)
    parser.add_argument("--exe", type=Path, help="Launch a dedicated preview window; existing windows are never closed")
    parser.add_argument("--edition", default="0.2.0-rc.1")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    process = None
    log = None
    if args.exe:
        if args.port is None:
            with socket.socket() as reservation:
                reservation.bind(("127.0.0.1", 0))
                args.port = reservation.getsockname()[1]
        environment = os.environ.copy()
        environment["VECTORCRAFT_CONFIG_DIR"] = str(output / "profile")
        environment.pop("VECTORCRAFT_NO_PREFS", None)
        log = (output / "desktop.log").open("ab")
        launch_args = [str(args.exe.resolve()), "--control", str(args.port), "--automation-read-root", str(output), "--automation-write-root", str(output)]
        process = subprocess.Popen(launch_args, env=environment, stdout=log, stderr=log, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        (output / "session.json").write_text(json.dumps({"pid": process.pid, "port": args.port}), encoding="utf-8")
    if args.port is None:
        parser.error("Use --exe or --port")
    client = ControlClient(args.port)
    for attempt in range(60):
        try:
            client.ui()
            break
        except ControlError:
            if process and process.poll() is not None:
                raise RuntimeError("Dedicated desktop exited before control became ready")
            time.sleep(.5)
    else:
        raise RuntimeError("Desktop control did not become ready")
    checks = []

    def check(name, condition):
        if not condition:
            raise AssertionError(name)
        checks.append(name)
        print("PASS", name, flush=True)

    def boxes(ids):
        nodes = flatten(client.inspect()["layers"])
        return {node_id: nodes[node_id]["bounds"] for node_id in ids}

    def same(first, second):
        return all(abs(first[i][k] - second[i][k]) < .05 for i in first for k in first[i])

    check("native panel command discovered", client.execute("ui.scientific", operation="inspect")["settings"]["columns"] >= 1)
    client.execute("file.new", title="科研组图原生预览", width=520, height=360)
    ids = []
    for i, (x, y, w, h) in enumerate([(45, 45, 160, 110), (330, 70, 110, 110), (70, 270, 150, 80), (360, 260, 160, 100)]):
        result = client.execute("file.place", name=f"Synthetic-{i+1}.png", dataBase64=base64.b64encode(example_png(w, h, i*20)).decode(), at=[x+w/2, y+h/2], link=False)
        ids.append(result["ids"][0])
    client.execute("select.set", ids=ids)
    original = boxes(ids)
    history = len(client.inspect()["history"])
    client.execute("ui.scientific", operation="grid", settings={"columns": 2, "gap_x": 18, "gap_y": 24})
    arranged = boxes(ids)
    check("native grid image bounds", abs(arranged[ids[1]]["x"]-arranged[ids[0]]["x"]-160-18)<.05 and abs(arranged[ids[2]]["y"]-arranged[ids[0]]["y"]-110-24)<.05)
    check("grid retains original objects and selection", client.inspect()["selection"] == ids)
    check("grid is one undo step", len(client.inspect()["history"]) == history+1)
    client.execute("edit.undo")
    check("grid undo restores original image bounds", same(original, boxes(ids)))
    client.execute("edit.redo")
    check("grid redo restores arranged bounds", same(arranged, boxes(ids)))
    client.execute("ui.scientific", operation="labels", settings={"columns": 2, "gap_x": 18, "gap_y": 24, "label_size": 18, "dx": 5, "dy": 5})
    labels = [n["id"] for n in flatten(client.inspect()["layers"]).values() if n.get("name", "").startswith("[SCI label]")]
    check("four editable native labels", len(labels)==4 and all(client.execute("document.node", id=i)["kind"]["type"]=="text" for i in labels))
    client.execute("ui.scientific", operation="labels", settings={"label_start": 3, "label_size": 18})
    updated = [n["id"] for n in flatten(client.inspect()["layers"]).values() if n.get("name", "").startswith("[SCI label]")]
    check("label updates preserve label IDs", labels == updated)
    check("label update changes actual native text", {client.execute("document.node", id=i)["kind"]["runs"][0]["text"] for i in labels} == {"C", "D", "E", "F"})
    before = client.inspect()
    try:
        client.execute("ui.scientific", operation="grid", settings={"columns": 0})
        raise AssertionError("invalid columns accepted")
    except ControlError:
        pass
    after = client.inspect()
    check("invalid settings leave document and history intact", before["layers"]==after["layers"] and before["history"]==after["history"])
    client.execute("ui.scientific", operation="labels", settings={"columns": 2, "gap_x": 18, "gap_y": 24, "label_size": 18})
    saved = output / "native-scientific-preview.vectorcraft"
    client.execute("document.save", path=str(saved), includeLinked=True)
    check("native document saved", saved.is_file() and saved.stat().st_size > 100)
    client.call("app.open", {"path": str(saved)})
    reopened = flatten(client.inspect()["layers"])
    check("save reopen preserves editable labels", len([n for n in reopened.values() if n.get("name", "").startswith("[SCI label]")])==4)
    client.execute("select.set", ids=ids)
    client.execute("ui.scientific", operation="labels")
    check("reopened associations update without duplicates", len([n for n in flatten(client.inspect()["layers"]).values() if n.get("name", "").startswith("[SCI label]")])==4)
    client.execute("document.save", path=str(saved), includeLinked=True)
    client.execute("app.language", lang="zh-hans")
    client.execute("window.panel", panel="science")
    check("science dock tab active", client.ui()["ui"]["dock_tab"] == "Science")
    client.execute("window.panel.float", panel="science")
    check("science panel floats", any("science" in group.get("panels", []) for group in client.ui()["ui"].get("floating_panels", [])))
    client.execute("window.panel.dock", panel="science")
    check("science panel docks", not any("science" in group.get("panels", []) for group in client.ui()["ui"].get("floating_panels", [])))
    if process:
        expected = client.execute("ui.scientific", operation="inspect")["settings"]
        try:
            client.execute("app.quit")
        except ControlError:
            pass  # A successful quit can close the connection before its response arrives.
        process.wait(timeout=15)
        process = subprocess.Popen(launch_args + [str(saved)], env=environment, stdout=log, stderr=log, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        (output / "session.json").write_text(json.dumps({"pid": process.pid, "port": args.port}), encoding="utf-8")
        for attempt in range(60):
            try:
                state = client.ui()
                if state.get("activeDocument") is not None:
                    break
            except ControlError:
                pass
            time.sleep(.5)
        else:
            raise RuntimeError("Restarted desktop did not open the saved example")
        check("restart preserves panel and parameters", state["ui"]["dock_tab"]=="Science" and client.execute("ui.scientific", operation="inspect")["settings"]==expected)
        client.execute("select.set", ids=ids)
    client.execute("document.export", path=str(output / "native-scientific-preview.png"), format="png", scale=1.5)
    time.sleep(1)
    client.call("ui.screenshot", {"path": str(output / "native-panel.png")})
    check("actual native window screenshot saved", (output / "native-panel.png").is_file())
    report = {"edition": args.edition, "checks": checks, "passed": len(checks), "scope": "native dock/grid/labels/undo/save-reopen; additional migration checks and public release are separate"}
    (output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
