"""Windows MCP launch regression for Main / Render port pairs."""

import argparse
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import socket
import time

from mcp_stdio import Mcp, listening


APP = {"target_name": "runtime_custom_port", "search_order": "example",
       "package_name": "bevy_brp_runtime", "path": "crates/runtime"}
CONFIGURED = (15752, 15753)
SYNCHRONIZE = 0x00100000
PROCESS_TERMINATE = 0x0001
WAIT_OBJECT_0 = 0


def wait_for_free(ports, timeout=15):
    deadline = time.monotonic() + timeout
    while any(listening(port) for port in ports):
        if time.monotonic() >= deadline:
            raise AssertionError(f"Listeners still active on {ports}")
        time.sleep(0.05)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    opts = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    opts.output.mkdir(parents=True, exist_ok=False)
    test_ports = (*CONFIGURED, 15940, 15941, 15942, 15943, 15944, 15946, 15960, 15961)
    assert not any(listening(port) for port in test_ports), "Test ports must be free"
    os.environ["BRP_EXTRAS_PORT"] = "15960"
    os.environ["BRP_EXTRAS_RENDER_PORT"] = "15961"
    conflicting_env = {"brp_extras_port": "15960", "brp_extras_render_port": "15961"}

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel32.OpenProcess.restype = wintypes.HANDLE
    kernel32.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
    kernel32.TerminateProcess.argtypes = [wintypes.HANDLE, wintypes.UINT]
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]

    mcp = Mcp(repo, opts.output)
    active = {}

    def discover(port):
        result = mcp.tool("rpc_discover", {"port": port})["result"]
        assert any(method.get("name") for method in result["methods"]), result

    def launch(expected, **params):
        result = mcp.tool("brp_launch", {**APP, "env": conflicting_env, **params})["result"]
        for instance in result:
            handle = kernel32.OpenProcess(SYNCHRONIZE | PROCESS_TERMINATE, False, instance["pid"])
            if not handle:
                raise ctypes.WinError(ctypes.get_last_error())
            active[instance["port"]] = (instance["render_port"], handle)
        assert [(instance["port"], instance["render_port"]) for instance in result] == expected
        assert len({instance["pid"] for instance in result}) == len(expected)
        for instance in result:
            assert instance["main_listening"] and instance["render_listening"], instance
            assert Path(instance["log_file"]).is_file(), instance
            discover(instance["port"])
            discover(instance["render_port"])

    def shutdown(port):
        render, handle = active[port]
        result = mcp.tool("brp_shutdown", {"app_name": APP["target_name"], "port": port})
        assert result["metadata"]["shutdown_method"] == "clean_shutdown", result
        assert kernel32.WaitForSingleObject(handle, 15000) == WAIT_OBJECT_0
        wait_for_free((port, render))
        kernel32.CloseHandle(handle)
        del active[port]

    def send(message):
        mcp.process.stdin.write(json.dumps({"jsonrpc": "2.0", **message}) + "\n")
        mcp.process.stdin.flush()

    def start_unconfirmed(request_id):
        # The fixture binds its App ports while MCP waits for a different pair.
        send({"id": request_id, "method": "tools/call", "params": {"name": "brp_launch",
              "arguments": {**APP, "port": 15940, "render_port": 15942, "ports_from_config": True}}})
        deadline = time.monotonic() + 30
        while not listening(CONFIGURED[0]):
            if time.monotonic() >= deadline:
                raise AssertionError("Fixture never reached the startup confirmation window")
            time.sleep(0.05)

    try:
        mcp.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                    "clientInfo": {"name": "launch-port-regression", "version": "1"}})
        mcp.process.stdin.write('{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        mcp.process.stdin.flush()
        tools = mcp.request("tools/list", {})["tools"]
        schema = next(tool["inputSchema"] for tool in tools if tool["name"] == "brp_launch")
        assert {"port", "render_port", "ports_from_config"} <= schema["properties"].keys()
        assert "render_port" not in schema.get("required", [])

        for params, field in [({"render_port": 15942}, "port"),
                              ({"port": 15940, "ports_from_config": True}, "render_port"),
                              ({"port": 15940, "render_port": 15940}, "render_port")]:
            failed = mcp.tool("brp_launch", {**APP, **params}, error=True)
            info = failed["error_info"]
            assert (info["stage"], info["field"]) == ("parameter_validation", field), failed

        with socket.socket() as occupied:
            occupied.setsockopt(socket.SOL_SOCKET, socket.SO_EXCLUSIVEADDRUSE, 1)
            occupied.bind(("127.0.0.1", 15942))
            occupied.listen()
            failed = mcp.tool("brp_launch", {**APP, "port": 15940, "render_port": 15942}, error=True)
            info = failed["error_info"]
            assert (info["stage"], info["instance"], info["endpoint"], info["port"]) == (
                "port_preflight", 1, "Render", 15942), failed
            assert not listening(15940)

        with socket.socket() as occupied:
            occupied.setsockopt(socket.SOL_SOCKET, socket.SO_EXCLUSIVEADDRUSE, 1)
            occupied.bind(("127.0.0.1", CONFIGURED[1]))
            occupied.listen()
            failed = mcp.tool("brp_launch", {**APP, "port": 15940, "render_port": 15942,
                              "ports_from_config": True, "env": {"RUST_LOG": "error"}}, error=True)
            info = failed["error_info"]
            assert (info["stage"], info["endpoint"], info["port"], info["planned_port"]) == (
                "listener_startup", "Render", CONFIGURED[1], 15942), failed
            assert Path(info["log_file"]).is_file()
        wait_for_free(test_ports)

        launch([CONFIGURED], port=CONFIGURED[0], render_port=CONFIGURED[1], ports_from_config=True)
        shutdown(CONFIGURED[0])
        launch([(15940, 15941)], port=15940)
        shutdown(15940)
        launch([(15940, 15942), (15941, 15943), (15944, 15946)],
               port=15940, render_port=15942, ports_from_config=True, instance_count=3)
        shutdown(15940)
        for remaining in (15941, 15943, 15944, 15946):
            discover(remaining)
        shutdown(15941)
        shutdown(15944)

        failed = mcp.tool("brp_launch", {**APP, "port": 15940, "render_port": 15942,
                          "ports_from_config": True}, error=True)
        assert failed["error_info"]["stage"] == "listener_startup", failed
        wait_for_free(test_ports)

        request_id = mcp.serial + 1
        start_unconfirmed(request_id)
        send({"method": "notifications/cancelled", "params": {"requestId": request_id}})
        wait_for_free(test_ports, timeout=3)

        start_unconfirmed(request_id + 1)
        mcp.process.stdin.close()
        assert mcp.process.wait(timeout=12) == 0, "MCP failed to exit on stdin close"
        wait_for_free(test_ports)
    finally:
        try:
            for _, handle in active.values():
                try:
                    if kernel32.WaitForSingleObject(handle, 0) != WAIT_OBJECT_0:
                        kernel32.TerminateProcess(handle, 1)
                        kernel32.WaitForSingleObject(handle, 5000)
                finally:
                    kernel32.CloseHandle(handle)
            wait_for_free(test_ports)
        finally:
            mcp.close()

    print("Passed MCP port-pair allocation, discovery, conflict and cleanup checks")


if __name__ == "__main__":
    main()
