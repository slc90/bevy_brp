"""Windows desktop MCP -> Custom Pointer -> real UI regression; never writes OS input."""

import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import time

from mcp_stdio import Mcp, listening


def process_alive(pid):
    if pid is None:
        return False
    result = subprocess.run(["pwsh", "-NoProfile", "-Command",
        f"$pointerFixtureProcess = Get-Process -Id {pid} -ErrorAction SilentlyContinue; "
        "if (-not $pointerFixtureProcess) { exit 0 }; "
        "if ($pointerFixtureProcess.ProcessName -ne 'pointer_test') { exit 2 }; exit 1"],
        capture_output=True, text=True, creationflags=subprocess.CREATE_NO_WINDOW)
    if result.returncode not in (0, 1):
        raise RuntimeError(f"Cannot inspect owned fixture PID {pid}: {result.stderr}")
    return result.returncode == 1


def cursor_position():
    point = wintypes.POINT()
    if not ctypes.windll.user32.GetCursorPos(ctypes.byref(point)):
        raise ctypes.WinError()
    return [point.x, point.y]


def foreground_pid():
    user32 = ctypes.windll.user32
    user32.GetForegroundWindow.restype = wintypes.HWND
    user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    pid = wintypes.DWORD()
    user32.GetWindowThreadProcessId(user32.GetForegroundWindow(), ctypes.byref(pid))
    return pid.value


def binary_sha256(path):
    with path.open("rb") as binary:
        return hashlib.file_digest(binary, "sha256").hexdigest()


def wait_for(getter, predicate, seconds=15):
    deadline = time.monotonic() + seconds
    while True:
        value = getter()
        if predicate(value):
            return value
        if time.monotonic() >= deadline:
            raise TimeoutError(f"State did not converge: {value}")
        time.sleep(0.025)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=15828)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 1024 <= args.port <= 65534:
        parser.error("port must be between 1024 and 65534")
    repo = Path(__file__).resolve().parent.parent
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    if listening(args.port):
        raise RuntimeError(f"Port {args.port} is already occupied")
    mcp = Mcp(repo, output)
    pid = None
    fixture_log = None
    summary = None
    try:
        mcp.request("initialize", {"protocolVersion":"2025-11-25", "capabilities":{},
            "clientInfo":{"name":"pointer-regression", "version":"1"}})
        mcp.process.stdin.write('{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        mcp.process.stdin.flush()
        tools = mcp.request("tools/list", {})["tools"]
        assert len(tools) in (47, 49)
        assert not any("pointer_control" in tool["name"] for tool in tools)
        for name in ["brp_extras_click_mouse", "brp_extras_double_click_mouse", "brp_extras_drag_mouse", "brp_extras_send_mouse_button"]:
            tool = next(tool for tool in tools if tool["name"] == name)
            assert [variant["const"] for variant in tool["inputSchema"]["properties"]["button"]["oneOf"]] == ["Left","Right","Middle"]
        (output / "tools-list.json").write_text(json.dumps(tools, indent=2), encoding="utf-8")
        launch_cursor = cursor_position()
        row = 50 if launch_cursor[1] > 450 else 600
        launch = mcp.tool("brp_launch", {"target_name":"pointer_test", "search_order":"example",
            "package_name":"bevy_brp_test_apps", "path":"tests/test-app", "port":args.port,
            "env":{"BRP_POINTER_FIXTURE_Y":str(row)}})
        pid = launch["result"][0]["pid"]
        fixture_log = Path(launch["result"][0]["log_file"])
        wait_for(lambda: listening(args.port), bool, 30)

        def call(tool_name, **params):
            return mcp.tool(tool_name, {"port":args.port, **params})

        def control(action="status"):
            return call("brp_execute", method="brp_extras/pointer_control", params={"action":action})["result"]

        def state():
            return call("brp_execute", method="fixture/state", params={})["result"]

        def named(name):
            found = call("world_find_entities_by_name", name=name)["result"]
            assert len(found) == 1, (name, found)
            return found[0]["entity"]

        def finished():
            return wait_for(control, lambda s: not s["busy"] and not s["pressed_buttons"])

        def release():
            control("release")
            return wait_for(control, lambda s: s["phase"] == "inactive")

        def click(window, button="Left"):
            call("brp_extras_click_mouse", window=window, button=button)
            finished()

        discovered = call("rpc_discover")["result"]["methods"]
        assert any(method["name"] == "brp_extras/pointer_control" for method in discovered)
        assert call("brp_list_agent_tools")["result"]["tools"] == []
        initial = control()
        assert initial["phase"] == "inactive" and initial["pointer_id"] is None
        assert control("release") == initial
        primary, secondary = named("PrimaryWindow"), named("SecondaryWindow")
        primary_camera, secondary_camera = named("PrimaryCamera"), named("SecondaryCamera")
        time.sleep(0.5)
        baseline = state()
        cursor_before, focus_before = cursor_position(), foreground_pid()
        (output / "desktop-baseline.json").write_text(json.dumps({"cursor":cursor_before,"foreground_pid":focus_before,"fixture_pid":pid}),encoding="utf-8")
        assert focus_before != pid, "Fixture unexpectedly owns OS foreground focus"

        call("brp_extras_move_mouse", window=primary, position=[70,45])
        finished()
        click(primary)
        first = state()
        assert first["primary"]["activations"] == 1
        for button in ["Right", "Middle"]:
            click(primary, button)
        assert state()["primary"]["activations"] == 3
        call("brp_extras_send_mouse_button", window=primary, button="Left", duration_ms=0)
        finished()
        assert state()["primary"]["activations"] == 4
        for delay, expected in [(0, [1,2]), (500, [1,1])]:
            time.sleep(0.5)
            before = len(state()["primary"]["click_counts"])
            call("brp_extras_double_click_mouse", window=primary, button="Left", delay_ms=delay)
            finished()
            assert state()["primary"]["click_counts"][before:] == expected
        call("brp_extras_move_mouse", window=secondary, position=[70,45])
        click(secondary)
        assert state()["secondary"]["activations"] == 1

        call("brp_extras_drag_mouse", window=primary, button="Left", start=[60,125], end=[330,125], frames=1)
        finished()
        dragged = state()
        assert dragged["primary"]["drag_starts"] == 1
        assert dragged["primary"]["drag_ends"] == 1
        assert dragged["primary"]["drops"] == 1
        assert dragged["primary"]["drag_x"] == 290
        call("brp_extras_move_mouse", window=primary, position=[70,250])
        call("brp_extras_scroll_mouse", window=primary, x=0, y=-1, unit="Line")
        finished()
        line = state()
        call("brp_extras_scroll_mouse", window=primary, x=0, y=-10, unit="Pixel")
        finished()
        scrolled = state()
        assert line["primary"]["scroll_y"] == 20
        assert scrolled["primary"]["scroll_y"] == 30
        assert scrolled["primary"]["scroll_units"] == ["Line","Pixel"]
        call("brp_extras_screenshot", camera=primary_camera, path=str(output / "primary.png"))
        capture_frame = state()["updates"]
        # Screenshot watching cleanup follows terminal delivery; wait for actual host cycles.
        wait_for(state, lambda s: s["active_operations"] == 0 and s["updates"] >= capture_frame + 3)
        call("brp_extras_screenshot", camera=secondary_camera, path=str(output / "secondary.png"))
        wait_for(state, lambda s: s["active_operations"] == 0)

        call("brp_extras_move_mouse", window=primary, position=[70,45])
        call("brp_extras_send_mouse_button", window=primary, button="Left", duration_ms=60000)
        wait_for(state, lambda s: s["primary"]["button_pressed"])
        before_cancel = state()
        call("brp_extras_move_mouse", window=primary, position=[450,340])
        wait_for(state, lambda s: s["primary"]["outs"] > before_cancel["primary"]["outs"])
        inactive = release()
        cancelled = state()
        assert not cancelled["primary"]["button_pressed"]
        assert cancelled["primary"]["cancels"] == before_cancel["primary"]["cancels"] + 1
        assert cancelled["primary"]["activations"] == before_cancel["primary"]["activations"]
        assert control("release") == inactive
        old_id = inactive["pointer_id"]
        for _ in range(2):
            call("brp_extras_move_mouse", window=primary, position=[70,45])
            click(primary)
            assert release()["pointer_id"] == old_id
        for params in [[], ["release"], {"action":{"release":None}}, {"action":"status","extra":0}]:
            bad = mcp.tool("brp_execute", {"port":args.port,"method":"brp_extras/pointer_control","params":params}, error=True)
            assert bad["error_info"]["code"] == -32602
            assert bad["error_info"]["data"]["method"] == "brp_extras/pointer_control"
        bad = mcp.tool("brp_extras_move_mouse", {"port":args.port,"window":0,"position":[0,0]}, error=True)
        assert bad["error_info"]["code"] == -32602

        # Keep an idle hover active, then verify bounded wake tail instead of continuous frames.
        call("brp_extras_move_mouse", window=primary, position=[70,45])
        idle = finished()
        assert idle["phase"] == "active"
        time.sleep(0.5)
        idle_before = state()
        time.sleep(0.5)
        idle_after = state()
        assert idle_after["updates"] - idle_before["updates"] <= 8
        assert idle_after["active_operations"] == 0
        assert idle_after["raw"] == baseline["raw"], (baseline["raw"], idle_after["raw"])
        cursor_after = cursor_position()
        (output / "desktop-after.json").write_text(json.dumps({"cursor":cursor_after,"foreground_pid":foreground_pid()}),encoding="utf-8")
        assert cursor_after == cursor_before, (cursor_before, cursor_after)
        assert foreground_pid() == focus_before
        assert not idle_after["primary"]["focused"] and not idle_after["secondary"]["focused"]
        assert idle_after["primary"]["native_cursor"] is None and idle_after["secondary"]["native_cursor"] is None

        call("brp_extras_send_mouse_button", window=secondary, button="Left", duration_ms=60000)
        wait_for(control, lambda s: s["pressed_buttons"] == ["Left"])
        call("brp_execute", method="fixture/close_window", params={"window":secondary})
        failed = wait_for(control, lambda s: s["phase"] == "inactive")
        assert failed["last_error"]["method"] == "brp_extras/send_mouse_button"
        assert failed["last_error"]["window"] == secondary
        assert not failed["busy"]
        stale = mcp.tool("brp_extras_click_mouse", {"port":args.port,"window":secondary,"button":"Left"}, error=True)
        assert stale["error_info"]["code"] == -32602
        summary = {"tools":len(tools),"cursor_before":cursor_before,"cursor_after":cursor_position(),
            "foreground_pid":focus_before,"first_click":first,"dragged":dragged,"scrolled":scrolled,
            "cancelled":cancelled,"idle_before":idle_before,"idle_after":idle_after,"window_failure":failed,
            "native_window_regions":"not supported; excluded", "revision":subprocess.check_output(["git","rev-parse","HEAD"],cwd=repo,text=True).strip(),
            "binary_sha256":{name:binary_sha256(repo / "target/debug" / name)
                for name in ["bevy_brp_mcp.exe","examples/pointer_test.exe"]}}
    finally:
        try:
            if process_alive(pid):
                try:
                    mcp.tool("brp_execute", {"port":args.port,"method":"brp_extras/pointer_control","params":{"action":"release"}})
                    stopped = mcp.tool("brp_shutdown", {"port":args.port,"app_name":"pointer_test"})
                    clean = stopped["metadata"]["shutdown_method"] == "clean_shutdown"
                    wait_for(lambda: process_alive(pid), lambda alive: not alive)
                    if summary is not None:
                        summary["clean_shutdown"] = clean
                        assert clean
                except Exception as error:
                    print(f"Fixture normal cleanup failed: {error}", flush=True)
                    if process_alive(pid):
                        killed = subprocess.run(["taskkill","/PID",str(pid),"/F"],capture_output=True,text=True)
                        if killed.returncode != 0 and process_alive(pid):
                            raise RuntimeError(f"Could not stop owned fixture PID {pid}: {killed.stderr}")
                    raise
        finally:
            mcp.close()
            if fixture_log is not None:
                shutil.copyfile(fixture_log, output / "fixture-app.log")
                if not process_alive(pid):
                    fixture_log.unlink()
        assert not listening(args.port)
        assert not process_alive(pid)
        assert mcp.process.returncode == 0
    summary.update({"port_released":True,"fixture_exited":True,"mcp_exited":True})
    (output / "summary.json").write_text(json.dumps(summary,indent=2),encoding="utf-8")
    print("Pointer desktop MCP to BRP regression passed; cursor unchanged; idle and cleanup verified.",flush=True)


if __name__ == "__main__":
    main()
