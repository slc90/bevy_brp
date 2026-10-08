"""Shared local MCP stdio transport for desktop regression fixtures."""

import json
import queue
import socket
import subprocess
import threading
import time


class Mcp:
    def __init__(self, repo, output):
        self.err = (output / "mcp-stderr.log").open("w", encoding="utf-8")
        self.process = subprocess.Popen(
            [str(repo / "target/debug/bevy_brp_mcp.exe")], cwd=repo,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.err,
            text=True, encoding="utf-8", creationflags=subprocess.CREATE_NO_WINDOW)
        self.lines = queue.Queue()
        self.serial = 0
        self.transcript = (output / "interaction.jsonl").open("w", encoding="utf-8")
        threading.Thread(target=self.read, daemon=True).start()

    def read(self):
        for line in self.process.stdout:
            self.lines.put(line)
        self.lines.put(None)

    def request(self, method, params):
        self.serial += 1
        payload = {"jsonrpc": "2.0", "id": self.serial, "method": method, "params": params}
        self.transcript.write(json.dumps({"request": payload}) + "\n")
        self.transcript.flush()
        self.process.stdin.write(json.dumps(payload) + "\n")
        self.process.stdin.flush()
        # brp_launch 的首次 workspace feature 合并可能触发完整图形依赖编译。
        deadline = time.monotonic() + (900 if params.get("name") == "brp_launch" else 120)
        while True:
            line = self.lines.get(timeout=max(0.01, deadline - time.monotonic()))
            if line is None:
                raise RuntimeError("MCP stdout closed")
            response = json.loads(line)
            self.transcript.write(json.dumps({"response": response}) + "\n")
            self.transcript.flush()
            if response.get("id") == self.serial:
                if "error" in response:
                    raise RuntimeError(response["error"])
                return response["result"]

    def tool(self, name, args, error=False):
        value = self.request("tools/call", {"name": name, "arguments": args})
        if bool(value.get("isError", False)) != error:
            raise RuntimeError(f"Unexpected {name} result: {value}")
        return value["structuredContent"]

    def close(self):
        try:
            try:
                self.process.stdin.close()
            except BrokenPipeError as error:
                print(f"MCP stdin closed during cleanup: {error}", flush=True)
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                result = subprocess.run(["taskkill", "/PID", str(self.process.pid), "/T", "/F"], capture_output=True, text=True)
                if result.returncode != 0 and self.process.poll() is None:
                    raise RuntimeError(f"Could not stop MCP process tree: {result.stderr}")
                self.process.wait(timeout=5)
        finally:
            try:
                self.transcript.close()
            finally:
                self.err.close()


def listening(port):
    with socket.socket() as client:
        client.settimeout(0.2)
        return client.connect_ex(("127.0.0.1", port)) == 0
