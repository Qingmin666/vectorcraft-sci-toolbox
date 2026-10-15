"""Local companion panel. Standard library only; no cloud or model service."""
from __future__ import annotations

import argparse
import base64
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import sys
import threading
import time
import webbrowser

from . import __version__
from .engine import Toolbox
from .layout import LayoutError, integer
from .protocol import ControlClient, ControlError

WEB = Path(__file__).with_name("web")


def user_data_dir():
    if os.name == "nt":
        return Path(os.environ.get("LOCALAPPDATA",Path.home())) / "VectorCraftSciToolbox"
    return Path(os.environ.get("XDG_CONFIG_HOME",Path.home()/".config")) / "vectorcraft-sci-toolbox"


class Companion:
    def __init__(self, port=7979, data_dir=None):
        self.data_dir=Path(data_dir) if data_dir else user_data_dir()
        self.client=ControlClient(port)
        self.toolbox=Toolbox(self.client)
        self.lock=threading.RLock()
        self.process=None
        self.settings=self.load_settings()

    def load_settings(self):
        documents=Path.home()/"Documents"
        defaults={"executable":self.find_executable(),"root":str(documents if documents.is_dir() else Path.home()),"presets":{}}
        try:
            saved=json.loads((self.data_dir/"settings.json").read_text(encoding="utf-8"))
            if isinstance(saved,dict):
                defaults.update({k:v for k,v in saved.items() if k in defaults})
        except (OSError,ValueError):pass
        return defaults

    @staticmethod
    def find_executable():
        candidates=[Path("D:/StudyApp/VectorCraft/vectorcraft.exe"),
                    Path(os.environ.get("PROGRAMFILES","C:/Program Files"))/"VectorCraft/vectorcraft.exe"]
        return next((str(p) for p in candidates if p.is_file()),"")

    def save_settings(self,options):
        # Settings contain only paths and UI presets, never credentials or document data.
        for key in ("executable","root"):
            if key in options:
                value=str(options[key])
                if len(value)>4096:raise LayoutError("路径过长")
                self.settings[key]=value
        if "presets" in options:
            presets=options["presets"]
            if not isinstance(presets,dict) or len(presets)>30 or len(json.dumps(presets))>100000:
                raise LayoutError("预设数量或大小超出限制")
            self.settings["presets"]=presets
        self.data_dir.mkdir(parents=True,exist_ok=True)
        temporary=self.data_dir/"settings.tmp"
        temporary.write_text(json.dumps(self.settings,ensure_ascii=False,indent=2),encoding="utf-8")
        temporary.replace(self.data_dir/"settings.json")
        return {"message":"设置已保存"}

    def status(self):
        with self.lock,self.toolbox.lock:
            try:
                state=self.toolbox.snapshot()
            except (ControlError,OSError) as exc:
                state={"connected":False,"document":None,"selection":[],"error":str(exc),
                       "uncertain":self.toolbox.uncertain}
            return {"state":state,"settings":self.settings,"version":__version__,"port":self.client.port}

    def connect(self,port):
        port=integer(port,"控制端口",1,65535)
        candidate=ControlClient(port)
        # UI probe proves this is a desktop backend, never a headless session.
        candidate.ui()
        with self.lock,self.toolbox.lock:
            if self.toolbox.uncertain:raise LayoutError("先检查当前画稿并恢复编辑后，才能切换连接")
            self.client=candidate
            self.toolbox=Toolbox(candidate)
        return {"message":"已连接 VectorCraft 桌面窗口",**self.status()}

    def launch(self,options):
        with self.lock:
            if self.toolbox.uncertain:raise LayoutError("请先检查当前画稿，不能在结果不确定时切换窗口")
            if self.process and self.process.poll() is None:
                raise LayoutError("专用窗口已运行，请点击连接或切换到该窗口")
            executable=Path(options.get("executable",self.settings["executable"])).expanduser().resolve()
            if not executable.is_file() or executable.name.lower() not in ("vectorcraft.exe","vectorcraft"):
                raise LayoutError("请选择已安装的 vectorcraft.exe 主程序")
            root=Path(options.get("root",self.settings["root"])).expanduser().resolve()
            if not root.is_dir():raise LayoutError("工作目录不存在，请先建立目录或选择现有目录")
            port=integer(options.get("port",7979),"控制端口",1,65535)
            # Refuse to attach silently to an unrelated process on the requested port.
            try:
                with socket.create_connection(("127.0.0.1",port),.3):
                    raise LayoutError("该端口已被占用。若是你的 VectorCraft，请点击『连接现有窗口』；否则更换端口")
            except OSError:pass
            env=os.environ.copy()
            # Dedicated profile keeps user preferences/recovery separate from an ordinary app instance.
            env["VECTORCRAFT_CONFIG_DIR"]=str(self.data_dir/"vectorcraft-profile")
            kwargs={"env":env,"stdout":subprocess.DEVNULL,"stderr":subprocess.DEVNULL}
            if os.name=="nt":
                info=subprocess.STARTUPINFO();info.dwFlags|=subprocess.STARTF_USESHOWWINDOW;info.wShowWindow=0
                kwargs["startupinfo"]=info
            self.process=subprocess.Popen([str(executable),"--control",str(port),
                "--automation-read-root",str(root),"--automation-write-root",str(root)],**kwargs)
            candidate=ControlClient(port,timeout=3)
            for _ in range(40):
                if self.process.poll() is not None:raise LayoutError("VectorCraft 启动失败，请检查程序路径和工作目录")
                try:candidate.ui();break
                except ControlError:time.sleep(.25)
            else:raise LayoutError("窗口启动较慢，请稍后点击『连接现有窗口』；不会重复启动")
            # focus is an official application method, not OS-level input injection.
            candidate.call("ui.focus")
            self.client=ControlClient(port)
            self.toolbox=Toolbox(self.client)
            self.save_settings({"executable":str(executable),"root":str(root)})
            return {"message":"专用窗口已启动；请在其中打开或新建画稿",**self.status()}


def make_server(companion: Companion, port=0):
    token=secrets.token_urlsafe(32)
    class Handler(BaseHTTPRequestHandler):
        def log_message(self,*args):pass

        def valid_host(self):
            return self.headers.get("Host")==f"127.0.0.1:{self.server.server_port}"

        def respond(self,status,data,content_type="application/json; charset=utf-8"):
            if not isinstance(data,bytes):data=json.dumps(data,ensure_ascii=False).encode()
            self.send_response(status)
            self.send_header("Content-Type",content_type)
            self.send_header("Content-Length",str(len(data)))
            self.send_header("Cache-Control","no-store")
            self.send_header("X-Content-Type-Options","nosniff")
            self.send_header("Referrer-Policy","no-referrer")
            self.send_header("Content-Security-Policy","default-src 'self'; img-src 'self' data: blob:; style-src 'self'; script-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'")
            self.end_headers()
            self.wfile.write(data)

        def authorized(self):
            return self.valid_host() and secrets.compare_digest(self.headers.get("X-Toolbox-Token",""),token)

        def do_GET(self):
            if not self.valid_host():return self.respond(403,{"error":"访问地址不正确"})
            if self.path in ("/","/index.html"):
                html=(WEB/"index.html").read_text(encoding="utf-8").replace("__SESSION_TOKEN__",token)
                return self.respond(200,html.encode(),"text/html; charset=utf-8")
            if self.path in ("/app.js","/style.css"):
                path=WEB/self.path[1:]
                return self.respond(200,path.read_bytes(),"text/javascript; charset=utf-8" if path.suffix==".js" else "text/css; charset=utf-8")
            if not self.authorized():return self.respond(403,{"error":"请从本工具箱的页面访问"})
            try:
                if self.path=="/api/state":return self.respond(200,companion.status())
                if self.path=="/api/preview":
                    with companion.lock,companion.toolbox.lock:
                        data=companion.client.call("ui.render",{"data":True,"scale":.8})
                    raw=base64.b64decode(data["pngBase64"])
                    return self.respond(200,raw,"image/png")
                return self.respond(404,{"error":"不存在的页面"})
            except (ControlError,LayoutError,KeyError,ValueError) as exc:
                return self.respond(400,{"error":str(exc)})

        def do_POST(self):
            origin=self.headers.get("Origin")
            expected=f"http://127.0.0.1:{self.server.server_port}"
            if not self.authorized() or (origin is not None and origin!=expected):
                return self.respond(403,{"error":"请求来源不正确"})
            try:
                size=int(self.headers.get("Content-Length","0"))
                if not 0<size<=256000:raise LayoutError("请求大小无效")
                payload=json.loads(self.rfile.read(size))
                if not isinstance(payload,dict):raise LayoutError("请求格式无效")
                with companion.lock:
                    if self.path=="/api/connect":result=companion.connect(payload.get("port",7979))
                    elif self.path=="/api/launch":result=companion.launch(payload)
                    elif self.path=="/api/settings":result=companion.save_settings(payload)
                    elif self.path=="/api/action":
                        result=companion.toolbox.run(payload.get("operation"),payload.get("options"),
                            payload.get("revision"),payload.get("documentIndex"),payload.get("selection"),payload.get("title"))
                    elif self.path=="/api/shutdown":
                        self.respond(200,{"message":"工具箱已关闭；VectorCraft 画稿保持打开"})
                        threading.Thread(target=self.server.shutdown,daemon=True).start()
                        return
                    else:return self.respond(404,{"error":"不存在的操作"})
                return self.respond(200,result)
            except (ControlError,LayoutError,ValueError,TypeError,KeyError,OSError) as exc:
                return self.respond(400,{"error":str(exc)})
            except Exception:
                return self.respond(500,{"error":"操作未正常结束，请刷新状态并检查画稿后再继续"})

    server=ThreadingHTTPServer(("127.0.0.1",port),Handler)
    server.daemon_threads=True
    return server


def main():
    parser=argparse.ArgumentParser(description="VectorCraft 科研组图工具箱")
    parser.add_argument("--port",type=int,default=0,help="面板端口，默认自动分配")
    parser.add_argument("--control-port",type=int,default=7979)
    parser.add_argument("--no-browser",action="store_true")
    parser.add_argument("--data-dir",type=Path,help="独立设置目录（用于测试或便携部署）")
    args=parser.parse_args()
    server=make_server(Companion(args.control_port,args.data_dir),args.port)
    url=f"http://127.0.0.1:{server.server_port}/"
    print(f"VectorCraft Sci Toolbox {__version__}: {url}",flush=True)
    if not args.no_browser:webbrowser.open(url)
    try:server.serve_forever(poll_interval=.3)
    except KeyboardInterrupt:pass
    finally:server.server_close()


if __name__=="__main__":main()
