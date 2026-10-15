"""Bounded, non-retrying client for the official loopback control protocol."""
from __future__ import annotations

import json
import socket
import threading


class ControlError(RuntimeError):
    """A command was refused or its completion is uncertain."""


class ControlClient:
    def __init__(self, port: int = 7979, timeout: float = 12):
        if not 1 <= int(port) <= 65535:
            raise ValueError("端口必须介于 1 和 65535 之间")
        self.port, self.timeout = int(port), timeout
        self._lock = threading.RLock()
        self._sequence = 0

    def call(self, method: str, params: dict | None = None):
        # Each request has a fresh connection. Never replay a possibly applied edit.
        with self._lock:
            self._sequence += 1
            request_id = self._sequence
            payload = json.dumps({"id": request_id, "method": method,
                                  "params": params or {}}, ensure_ascii=False).encode() + b"\n"
            if len(payload) > 4 * 1024 * 1024:
                raise ControlError("请求过大，尚未发送")
            sent = False
            try:
                with socket.create_connection(("127.0.0.1", self.port), self.timeout) as sock:
                    sock.settimeout(self.timeout)
                    sock.sendall(payload)
                    sent = True
                    with sock.makefile("rb") as stream:
                        line = stream.readline(64 * 1024 * 1024 + 1)
                    if not line or len(line) > 64 * 1024 * 1024 or not line.endswith(b"\n"):
                        raise ControlError("响应为空或超出限制；操作可能已完成，请检查画稿后再操作")
                    reply = json.loads(line)
            except (OSError, ValueError) as exc:
                suffix = "操作可能已完成，请检查画稿；不会自动重试" if sent else "尚未发送操作，请连接专用窗口"
                raise ControlError(f"无法取得 VectorCraft 响应：{suffix}") from exc
            if reply.get("id") != request_id:
                raise ControlError("响应编号不匹配；请检查画稿，不要重复执行")
            if not reply.get("ok"):
                raise ControlError(str(reply.get("error", "VectorCraft 拒绝了操作")))
            return reply.get("result")

    def execute(self, command: str, **params):
        return self.call("engine.execute", {"command": command, "params": params})

    def inspect(self):
        return self.call("document.inspect")

    def ui(self):
        return self.call("ui.inspect")
