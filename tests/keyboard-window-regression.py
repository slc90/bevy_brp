"""通过本地 MCP stdio 验证 multi-window keyboard 的真实 runtime 路径。"""

import argparse
import hashlib
import json
from pathlib import Path
import queue
import shutil
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


def process_alive(pid=None):
    selector = f"-Id {pid}" if pid is not None else "-Name keyboard_windows"
    result = subprocess.run(
        ["pwsh", "-NoProfile", "-Command",
         f"$keyboardFixtureProcess = Get-Process {selector} -ErrorAction SilentlyContinue; "
         "if (-not $keyboardFixtureProcess) { exit 0 }; "
         "if ($keyboardFixtureProcess.ProcessName -ne 'keyboard_windows') { exit 2 }; exit 1"],
        capture_output=True, text=True, creationflags=subprocess.CREATE_NO_WINDOW)
    if result.returncode not in (0, 1):
        raise RuntimeError(f"Could not inspect fixture PID {pid}: {result.stderr}")
    return result.returncode == 1


def binary_sha256(path):
    with path.open("rb") as binary:
        return hashlib.file_digest(binary, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=15816)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 1 <= args.port <= 65535:
        parser.error("--port must be between 1 and 65535")
    repo = Path(__file__).resolve().parent.parent
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    if listening(args.port):
        raise RuntimeError(f"Port {args.port} is already in use")
    if process_alive():
        raise RuntimeError("A keyboard_windows fixture is already running; use an isolated session")
    mcp = Mcp(repo, output)
    pid = None
    fixture_log = None
    clean_shutdown = False
    try:
        mcp.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                  "clientInfo": {"name": "keyboard-window-regression", "version": "1"}})
        mcp.process.stdin.write('{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        mcp.process.stdin.flush()
        tools = mcp.request("tools/list", {})["tools"]
        for name in ["brp_extras_send_keys", "brp_extras_type_text"]:
            tool = next(tool for tool in tools if tool["name"] == name)
            assert "window" in tool["inputSchema"]["properties"]
            assert "window" not in tool["inputSchema"].get("required", [])
        (output / "tools-list.json").write_text(json.dumps(tools, indent=2), encoding="utf-8")
        launch = mcp.tool("brp_launch", {"target_name": "keyboard_windows", "search_order": "example",
                         "package_name": "bevy_brp_test_apps", "path": str(repo / "tests/test-app"), "port": args.port})
        pid = launch["result"][0]["pid"]
        fixture_log = Path(launch["result"][0]["log_file"])
        deadline = time.monotonic() + 30
        while not listening(args.port):
            if time.monotonic() >= deadline:
                raise TimeoutError("Fixture did not start listening")
            time.sleep(0.1)

        def call(tool_name, **values):
            return mcp.tool(tool_name, {"port": args.port, **values})

        def named(name):
            found = call("world_find_entities_by_name", name=name)["result"]
            assert len(found) == 1, (name, found)
            return found[0]["entity"]

        primary, secondary = named("PrimaryWindow"), named("SecondaryWindow")
        evidence_type = "keyboard_windows::WindowInputEvidence"
        global_type = "keyboard_windows::GlobalKeyboardEvidence"

        def evidence(entity, component=evidence_type):
            return call("world_get_components", entity=entity, components=[component])["result"]["components"][component]

        def wait_for(getter, predicate):
            deadline = time.monotonic() + 15
            while True:
                value = getter()
                if predicate(value):
                    return value
                if time.monotonic() >= deadline:
                    raise TimeoutError(f"State did not converge: {value}")
                time.sleep(0.05)

        def click(window):
            call("brp_extras_move_mouse", window=window, position=[120, 100])
            time.sleep(0.15)
            call("brp_extras_click_mouse", window=window, button="Left")
            wait_for(lambda: evidence(window), lambda state: state["pointer_clicks"] > 0)

        click(secondary)
        call("brp_extras_send_keys", window=secondary, keys=["KeyA"], duration_ms=0)
        wait_for(lambda: evidence(secondary), lambda state: state["text"] == "a" and state["presses"] == state["releases"])
        assert evidence(primary)["text"] == ""
        call("brp_extras_type_text", window=secondary, text="Az!")
        secondary_text = wait_for(lambda: evidence(secondary), lambda state: state["text"] == "aAz!" and state["presses"] == state["releases"])
        assert evidence(primary)["text"] == ""
        call("brp_extras_screenshot", camera=named("SecondaryCamera"), path=str(output / "secondary.png"))
        click(primary)
        # 默认 keyboard 必须保持 PrimaryWindow，不能跟随最后移动过的 secondary cursor。
        call("brp_extras_move_mouse", window=secondary, position=[120, 100])
        call("brp_extras_send_keys", keys=["KeyP"], duration_ms=0)
        call("brp_extras_type_text", text="ok")
        primary_text = wait_for(lambda: evidence(primary), lambda state: state["text"] == "pok" and state["presses"] == state["releases"])
        assert evidence(secondary)["text"] == "aAz!"
        call("brp_extras_screenshot", camera=named("PrimaryCamera"), path=str(output / "primary.png"))

        for name, payload in [("brp_extras_send_keys", {"keys": ["KeyA"]}),
                              ("brp_extras_type_text", {"text": "bad"})]:
            invalid = mcp.tool(name, {"port": args.port, "window": 0, **payload}, error=True)
            assert invalid["error_info"]["code"] == -32602
            assert invalid["error_info"]["port"] == args.port

        before_hold_secondary = evidence(secondary)
        before_hold_primary = evidence(primary)
        call("brp_extras_send_keys", window=secondary, keys=["ControlLeft"], duration_ms=60000)
        wait_for(lambda: evidence(primary, global_type), lambda state: state["ctrl_pressed"])
        held_keys_secondary = evidence(secondary)
        assert held_keys_secondary["presses"] == before_hold_secondary["presses"] + 1
        assert evidence(primary)["presses"] == before_hold_primary["presses"]
        call("brp_extras_type_text", window=secondary, text="B" * 10000)
        typing_started = wait_for(lambda: evidence(secondary), lambda state:
                                  state["presses"] >= held_keys_secondary["presses"] + 2
                                  and state["text"].startswith(secondary_text["text"] + "B"))
        modifiers_before_close = wait_for(lambda: evidence(primary, global_type), lambda state:
                                          state["ctrl_pressed"] and state["shift_pressed"]
                                          and state["active_operations"] == 2)
        call("brp_execute", method="fixture/close_window", params={"window": secondary})
        wait_for(lambda: call("world_find_entities_by_name", name="SecondaryWindow")["result"], lambda rows: not rows)
        final = wait_for(lambda: evidence(primary, global_type), lambda state: not state["ctrl_pressed"] and not state["shift_pressed"] and state["active_operations"] == 0)
        assert evidence(primary)["text"] == "pok"
        for name, payload in [("brp_extras_send_keys", {"keys": ["ShiftLeft"]}),
                              ("brp_extras_type_text", {"text": "stale"})]:
            stale = mcp.tool(name, {"port": args.port, "window": secondary, **payload}, error=True)
            assert stale["error_info"]["code"] == -32602

        result = call("brp_shutdown", app_name="keyboard_windows")
        clean_shutdown = result["metadata"]["shutdown_method"] == "clean_shutdown"
        assert clean_shutdown
        wait_for(lambda: listening(args.port), lambda state: not state)
        wait_for(lambda: process_alive(pid), lambda state: not state)
        summary = {"primary": primary, "secondary": secondary, "primary_input": primary_text,
                   "secondary_input": secondary_text, "after_target_close": final,
                   "typing_started": typing_started, "modifiers_before_close": modifiers_before_close,
                   "clean_shutdown": clean_shutdown, "port_released": True,
                   "binary_sha256": {name: binary_sha256(repo / "target/debug" / name)
                                     for name in ["bevy_brp_mcp.exe", "examples/keyboard_windows.exe"]},
                   "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
                   "working_tree": subprocess.check_output(["git", "status", "--short"], cwd=repo, text=True)}
    finally:
        try:
            if pid is not None and process_alive(pid):
                try:
                    mcp.tool("brp_shutdown", {"app_name": "keyboard_windows", "port": args.port})
                except Exception as error:
                    print(f"Fixture shutdown failed during cleanup: {error}", flush=True)
                if process_alive(pid):
                    # 仅终止本轮 launcher 返回的 PID，不处理用户已有进程。
                    result = subprocess.run(["taskkill", "/PID", str(pid), "/F"], capture_output=True, text=True)
                    if result.returncode != 0 and process_alive(pid):
                        raise RuntimeError(f"Could not stop fixture PID {pid}: {result.stderr}")
                if process_alive(pid):
                    raise RuntimeError(f"Fixture PID {pid} was not released")
        finally:
            mcp.close()
            if fixture_log is not None:
                shutil.copyfile(fixture_log, output / "fixture-app.log")
                if not process_alive(pid):
                    fixture_log.unlink()
        if listening(args.port):
            raise RuntimeError(f"Port {args.port} was not released")
    summary["mcp_exited"] = mcp.process.poll() is not None
    summary["mcp_exit_code"] = mcp.process.returncode
    if mcp.process.returncode != 0:
        raise RuntimeError(f"MCP did not exit cleanly: {mcp.process.returncode}")
    (output / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print("Multi-window MCP to BRP keyboard regression passed.", flush=True)


if __name__ == "__main__":
    main()
