"""Real MCP -> BRP screenshot geometry, composited pixels and cleanup regression.

Requires Python Pillow and the built Windows MCP and extras_plugin fixture.
"""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time

from PIL import Image
from mcp_stdio import Mcp, listening


def wait_until(predicate, seconds=30):
    deadline = time.monotonic() + seconds
    while not predicate():
        if time.monotonic() >= deadline:
            raise TimeoutError("Screenshot fixture did not converge")
        time.sleep(0.05)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=15934)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 1024 <= args.port <= 65534:
        parser.error("port must be between 1024 and 65534")
    repo = Path(__file__).resolve().parent.parent
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    if listening(args.port):
        raise RuntimeError("Screenshot port is occupied")
    mcp = Mcp(repo, output)
    pid = None
    captures = {}

    def call(tool_name, error=False, **params):
        return mcp.tool(tool_name, {"port": args.port, **params}, error=error)

    def capture(name, expected_rect, pixels):
        path = output / (name + ".png")
        deadline = time.monotonic() + 30
        while True:
            value = call("brp_extras_screenshot", name=name, padding=0, path=str(path))
            result = value["result"]
            rect = result["rect"]
            actual = tuple(rect[key] for key in ("x", "y", "width", "height"))
            assert actual == expected_rect, (name, actual, expected_rect)
            with Image.open(path) as image:
                assert image.size == expected_rect[2:], (name, image.size)
                rgb = image.convert("RGB")
                observed = {point: rgb.getpixel(point) for point in pixels}
            if observed == pixels:
                break
            assert time.monotonic() < deadline, (name, observed, pixels)
            # Shader compilation can leave initial retained frames clear; inspect actual pixels.
            time.sleep(0.1)
        captures[name] = result

    try:
        mcp.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": {"name": "screenshot-regression", "version": "1"}})
        mcp.process.stdin.write('{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        mcp.process.stdin.flush()
        launch = call("brp_launch", target_name="extras_plugin", package_name="bevy_brp_test_apps",
            search_order="example", path="tests/test-app")
        pid = launch["result"][0]["pid"]
        wait_until(lambda: listening(args.port))
        for name, expected_rect, pixels in [
            ("NatesList", (40, 32, 64, 48), {(0, 0): (0, 0, 255), (12, 12): (255, 255, 0), (60, 24): (255, 0, 255)}),
            ("ScreenshotRotatedClippedUi", (132, 40, 32, 56), {(16, 28): (0, 255, 255)}),
            ("ScreenshotNestedClippedUi", (40, 112, 40, 40), {(20, 20): (0, 255, 0), (0, 0): (0, 0, 0), (39, 39): (0, 0, 0)}),
            ("Screenshot2dAabb", (106, 98, 12, 60), {(6, 30): (255, 255, 0), (0, 0): (255, 0, 0)}),
        ]:
            capture(name, expected_rect, pixels)
        nested = call("world_find_entities_by_name", name="ScreenshotNestedClippedUi")["result"][0]["entity"]
        clip = call("world_get_components", entity=nested, components=["bevy_ui::ui_node::CalculatedClip"])
        (output / "nested-clip.json").write_text(json.dumps(clip, indent=2), encoding="utf-8")
        assert len(clip["result"]["components"]["bevy_ui::ui_node::CalculatedClip"]["Rects"]) == 2
        for name in ("ScreenshotHiddenUi", "ScreenshotPartialUi", "ScreenshotUnsupported", "ScreenshotDuplicateName"):
            value = call("brp_extras_screenshot", error=True, name=name, path=str(output / (name + ".png")))
            assert value["status"] == "error", value
            if name != "ScreenshotDuplicateName":
                assert value["error_info"]["method"] == "brp_extras/screenshot"
                assert value["error_info"]["port"] == args.port
                assert value["error_info"]["code"] == -32602
            assert not (output / (name + ".png")).exists()
        camera = call("world_find_entities_by_name", name="ScreenshotPrimaryWindowCamera")["result"][0]["entity"]
        mismatch = call("brp_extras_screenshot", error=True, name="NatesList", camera=camera,
            path=str(output / "wrong-camera.png"))
        assert mismatch["status"] == "error"
        assert mismatch["error_info"]["code"] == -32602
        assert not (output / "wrong-camera.png").exists()
        for name, active in [("Screenshot2dUiCamera", False), ("Screenshot3dCamera", True)]:
            entity = call("world_find_entities_by_name", name=name)["result"][0]["entity"]
            call("world_mutate_components", entity=entity, component="bevy_camera::camera::Camera", path="is_active", value=active)
        capture("Screenshot3dAabb", (162, 90, 12, 48), {(6, 24): (255, 255, 0), (2, 2): (0, 255, 0)})
    finally:
        try:
            if pid is not None:
                shutdown = call("brp_shutdown", app_name="extras_plugin")
                wait_until(lambda: not listening(args.port))
                assert shutdown["metadata"]["shutdown_method"] == "clean_shutdown", shutdown
        finally:
            mcp.close()
    check = subprocess.run(["pwsh", "-NoProfile", "-Command",
        f"if (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ exit 1 }}"], capture_output=True)
    assert check.returncode == 0
    assert mcp.process.returncode == 0
    summary = {"captures": captures, "port": args.port, "clean_shutdown": True,
        "port_released": True, "fixture_exited": True, "mcp_exited": True,
        "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
        "binary_sha256": {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in
            (repo / "target/debug/bevy_brp_mcp.exe", repo / "target/debug/examples/extras_plugin.exe")}}
    (output / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print("Screenshot geometry, pixels, errors and cleanup passed.")


if __name__ == "__main__":
    main()
