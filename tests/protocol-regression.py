"""Windows runtime Main/Render and real MCP reflection protocol regression."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import http.client
import json
import os
import socket
from pathlib import Path
import subprocess
import sys
import time

from mcp_stdio import Mcp, listening


def wait_until(predicate, seconds=30):
    deadline = time.monotonic() + seconds
    while not predicate():
        if time.monotonic() >= deadline:
            raise TimeoutError("Protocol fixture did not converge")
        time.sleep(0.05)


def http_request(port, method, params=None, request_id=1, timeout=40):
    wire = {"jsonrpc": "2.0", "id": request_id, "method": method}
    if params is not None:
        wire["params"] = params
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
    try:
        connection.request("POST", "/", json.dumps(wire), {"Content-Type": "application/json"})
        response = connection.getresponse()
        assert response.status == 200
        value = json.loads(response.read())
        assert value["id"] == request_id
        return value
    finally:
        connection.close()


def verify_bind_failure(repo, output, main_port, blocked_port):
    # Only this test's socket is occupied; the fixture must close the other listener itself.
    with socket.socket() as blocker:
        blocker.bind(("127.0.0.1", blocked_port))
        blocker.listen(1)
        environment = {**os.environ, "BRP_EXTRAS_PORT": str(main_port)}
        with (output / f"bind-failure-{blocked_port}.log").open("w", encoding="utf-8") as log:
            process = subprocess.Popen([str(repo / "target/debug/examples/event_test.exe")], cwd=repo,
                env=environment, stdout=log, stderr=log, creationflags=subprocess.CREATE_NO_WINDOW)
            try:
                code = process.wait(timeout=30)
                assert code == 1, (blocked_port, code)
                failure_log = (output / f"bind-failure-{blocked_port}.log").read_text(encoding="utf-8")
                endpoint = "Main" if blocked_port == main_port else "Render"
                assert "failed to bind listener" in failure_log and f'endpoint="{endpoint}"' in failure_log
                other_port = 15703 if blocked_port == main_port else main_port
                wait_until(lambda: not listening(other_port))
                return {"blocked_port": blocked_port, "exit_code": code, "other_port_released": True}
            finally:
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=10)


def fixture_alive(pid):
    assert isinstance(pid, int) and pid > 0
    check = subprocess.run(["pwsh", "-NoProfile", "-Command",
        f"$protocolFixtureProcess = Get-Process -Id {pid} -ErrorAction SilentlyContinue; "
        "if (-not $protocolFixtureProcess) { exit 0 }; "
        "if ($protocolFixtureProcess.ProcessName -ne 'event_test') { exit 2 }; exit 1"], capture_output=True)
    if check.returncode not in (0, 1):
        raise RuntimeError(f"Cannot verify owned event_test PID {pid}")
    return check.returncode == 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=15936)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 1024 <= args.port <= 65534 or args.port == 15703:
        parser.error("Main port must be between 1024 and 65534 and differ from Render 15703")
    repo = Path(__file__).resolve().parent.parent
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    if listening(args.port) or listening(15703):
        raise RuntimeError("Main or Render port is occupied")
    os.environ["TEMP"] = str(output)
    os.environ["TMP"] = str(output)
    mcp = Mcp(repo, output)
    pid = None
    watch_id = None
    results = {}
    data_type = "event_test::ProtocolData"
    resource_type = "event_test::ProtocolResource"

    def call(tool_name, error=False, **params):
        value = mcp.tool(tool_name, {"port": args.port, **params}, error=error)
        result = value.get("result")
        if isinstance(result, dict) and result.get("saved_to_file"):
            value["result"] = json.loads(Path(result["filepath"]).read_text(encoding="utf-8"))
        return value

    def state():
        return http_request(args.port, "fixture/protocol_state")["result"]

    try:
        mcp.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": {"name": "protocol-regression", "version": "1"}})
        mcp.process.stdin.write('{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        mcp.process.stdin.flush()
        tools = mcp.request("tools/list", {})
        (output / "tools-list.json").write_text(json.dumps(tools, indent=2), encoding="utf-8")
        pid = call("brp_launch", target_name="event_test", package_name="bevy_brp_test_apps",
            search_order="example", path="tests/test-app")["result"][0]["pid"]
        wait_until(lambda: listening(args.port) and listening(15703))
        for port in (args.port, 15703):
            discovery = http_request(port, "rpc.discover", request_id="discover")
            assert discovery["result"]["methods"]
            results[str(port)] = discovery
        schema = call("registry_schema")["result"]
        (output / "registry.json").write_text(json.dumps(schema, indent=2), encoding="utf-8")
        assert "bevy_render::view::Tonemapping" in schema
        assert "bevy_render::view::DebandDither" in schema
        assert "bevy_core_pipeline::tonemapping::Tonemapping" not in schema
        guide = call("brp_type_guide", types=[data_type, resource_type, "event_test::ProtocolLinks",
            "bevy_ui::ui_node::CalculatedClip", "bevy_ecs::entity::Entity", "bevy_asset::handle::Handle<bevy_image::image::Image>"])
        (output / "guides.json").write_text(json.dumps(guide, indent=2), encoding="utf-8")
        data = guide["result"]["type_guide"][data_type]["spawn"]["example"]
        entity = call("world_spawn_entity", components={data_type: data})["result"]["entity"]
        results["entity"] = entity
        read = call("world_get_components", entity=entity, components=[data_type])["result"]["components"][data_type]
        assert read == data, (read, data)
        links_guide = guide["result"]["type_guide"]["event_test::ProtocolLinks"]
        assert "example" not in links_guide["spawn"]  # Strong handles prevent a whole-struct example.
        handle_guide = guide["result"]["type_guide"]["bevy_asset::handle::Handle<bevy_image::image::Image>"]
        handle_examples = handle_guide["mutation_paths"][0]["examples"]
        assert any(item["mutability"] == "not_mutable" for item in handle_examples)
        image_example = next(item["example"] for item in handle_examples if item["mutability"] == "mutable")
        generated_links = {"target": entity, "image": image_example}
        call("world_insert_components", entity=entity, components={"event_test::ProtocolLinks": generated_links})
        links = call("world_get_components", entity=entity, components=["event_test::ProtocolLinks"])["result"]["components"]["event_test::ProtocolLinks"]
        assert links == generated_links
        high = 9007199254740993
        for path, value in [("count", high), ("mode", "Running"), ("optional", 7),
            ("nested.value", 2.5), ("values", [3, 5]), ("labels", {"probe": 11})]:
            call("world_mutate_components", entity=entity, component=data_type, path=path, value=value)
        read = call("world_get_components", entity=entity, components=[data_type])["result"]["components"][data_type]
        assert read["count"] == high and read["mode"] == "Running" and read["optional"] == 7
        assert read["nested"]["value"] == 2.5 and read["values"] == [3, 5] and read["labels"] == {"probe": 11}
        call("world_mutate_components", entity=entity, component=data_type, path="optional", value=None)
        assert call("world_get_components", entity=entity, components=[data_type])["result"]["components"][data_type]["optional"] is None
        missing_field = {key: value for key, value in data.items() if key != "count"}
        defaulted = call("world_spawn_entity", components={data_type: missing_field})["result"]["entity"]
        assert call("world_get_components", entity=defaulted, components=[data_type])["result"]["components"][data_type]["count"] == 0
        call("world_despawn_entity", entity=defaulted)
        call("world_spawn_entity", error=True, components={data_type: {**data, "count": None}})
        assert http_request(args.port, "fixture/protocol_params")["result"]["present"] is False
        assert http_request(args.port, "fixture/protocol_params", {})["result"]["present"] is True
        call("world_insert_components", entity=entity, components={"bevy_ecs::name::Name": "ProtocolProbe"})
        assert call("world_find_entities_by_name", name="ProtocolProbe")["result"][0]["entity"] == entity
        assert data_type in call("world_list_components", entity=entity)["result"]
        query = call("world_query", data={"components": [data_type]}, filter={"with": [data_type]})
        assert any(item["entity"] == entity for item in query["result"])
        parent = call("world_spawn_entity", components={})["result"]["entity"]
        call("world_reparent_entities", entities=[entity], parent=parent)
        relation = call("world_get_components", entity=entity, components=["bevy_ecs::hierarchy::ChildOf"])
        assert relation["result"]["components"]["bevy_ecs::hierarchy::ChildOf"] == parent
        call("world_reparent_entities", entities=[entity])
        assert "bevy_ecs::hierarchy::ChildOf" not in call("world_list_components", entity=entity)["result"]
        resource = guide["result"]["type_guide"][resource_type]["resource"]["example"]
        call("world_insert_resources", resource=resource_type, value=resource)
        call("world_mutate_resources", resource=resource_type, path="value", value=high)
        assert call("world_get_resources", resource=resource_type)["result"]["value"]["value"] == high
        call("world_remove_resources", resource=resource_type)
        call("world_get_resources", error=True, resource=resource_type)
        call("world_trigger_event", event="event_test::TestUnitEvent")
        call("world_trigger_event", event="event_test::TestPayloadEvent", value={"message": "protocol-observer", "value": 42})
        tracker = call("world_get_resources", resource="event_test::EventTriggerTracker")["result"]["value"]
        assert tracker["unit_events"] == 1 and tracker["payload_events"] == 1
        assert tracker["last_payload_message"] == "protocol-observer" and tracker["last_payload_value"] == 42
        assert call("brp_list_agent_tools")["result"]["tools"] == []
        watch = call("world_get_components_watch", entity=entity, types=[data_type])
        watch_id = watch["metadata"]["watch_id"]
        log_path = Path(watch["metadata"]["log_path"])
        def watch_received(count):
            if not log_path.exists():
                return False
            for line in log_path.read_text(encoding="utf-8").splitlines():
                payload = line.partition(" COMPONENT_UPDATE: ")[2]
                if payload:
                    try:
                        value = json.loads(payload)
                    except json.JSONDecodeError:
                        continue  # A concurrent append may not have completed this last line.
                    if value.get("components", {}).get(data_type, {}).get("count") == count:
                        return True
            return False
        pending_count = 15
        def establish_watch():
            nonlocal pending_count
            pending_count = 16 if pending_count == 15 else 15
            call("world_mutate_components", entity=entity, component=data_type, path="count", value=pending_count)
            return watch_received(16)
        wait_until(establish_watch)
        call("world_mutate_components", entity=entity, component=data_type, path="count", value=17)
        wait_until(lambda: watch_received(17))
        mcp.tool("brp_stop_watch", {"watch_id": watch_id})
        watch_id = None
        assert mcp.tool("brp_list_active_watches", {})["metadata"]["watch_count"] == 0
        for method, params, code in [("fixture/missing", {}, -32601), ("world.get_components", {"entity": entity, "components": ["missing::Type"], "strict": True}, -23402)]:
            bad = http_request(args.port, method, params)
            assert bad["error"]["code"] == code, bad
        # Both endpoints must continue after unknown methods and concurrent queued work.
        with ThreadPoolExecutor(max_workers=12) as pool:
            jobs = [pool.submit(http_request, port, "fixture/missing" if i % 3 == 0 else "rpc.discover", request_id=i)
                for i in range(36) for port in (args.port, 15703)]
            for i, job in enumerate(jobs):
                value = job.result()
                assert ("error" in value) == ((i // 2) % 3 == 0)
        before = state()
        connection = http.client.HTTPConnection("127.0.0.1", args.port, timeout=10)
        connection.request("POST", "/", json.dumps({"jsonrpc": "2.0", "id": "sse-id", "method": "fixture/protocol_stream+watch"}))
        stream = connection.getresponse()
        assert stream.getheader("Content-Type") == "text/event-stream"
        line = stream.readline()
        assert b'sse-id' in line and b'watch_calls' in line, line
        stream.close()
        connection.close()
        time.sleep(0.2)
        after = state()
        time.sleep(0.5)
        idle = state()
        assert idle["watch_calls"] == after["watch_calls"], (after, idle)
        assert idle["updates"] - after["updates"] <= 15, (after, idle)
        assert idle["active_operations"] == 0 and not idle["focused"]
        # Dropping a normal HTTP client must release its implicit Watching request too.
        abort = http.client.HTTPConnection("127.0.0.1", args.port, timeout=10)
        try:
            abort.request("POST", "/", json.dumps({"jsonrpc": "2.0", "id": "aborted", "method": "fixture/protocol_wait"}))
            wait_until(lambda: state()["watch_calls"] > idle["watch_calls"])
        finally:
            abort.close()
        time.sleep(0.2)
        cancelled = state()
        time.sleep(0.5)
        idle_cancelled = state()
        assert idle_cancelled["watch_calls"] == cancelled["watch_calls"], (cancelled, idle_cancelled)
        start = time.monotonic()
        deadline = http_request(args.port, "fixture/protocol_wait", request_id="timeout-id")
        elapsed = time.monotonic() - start
        assert deadline["error"]["code"] == -32603 and "30 seconds" in deadline["error"]["message"]
        assert 29 <= elapsed <= 40, elapsed
        time.sleep(0.2)
        completed = state()
        time.sleep(0.5)
        idle_timeout = state()
        assert idle_timeout["watch_calls"] == completed["watch_calls"]
        assert idle_timeout["updates"] - completed["updates"] <= 15
        call("world_remove_components", entity=entity, components=[data_type])
        assert data_type not in call("world_list_components", entity=entity)["result"]
        call("world_despawn_entity", entity=entity)
        call("world_get_components", error=True, entity=entity, components=[data_type])
        call("world_despawn_entity", entity=parent)
        results.update({"before_sse": before, "idle": idle, "idle_cancelled": idle_cancelled,
            "timeout": deadline, "timeout_seconds": elapsed, "idle_timeout": idle_timeout})
    finally:
        original_error = sys.exception()
        cleanup_errors = []
        if watch_id is not None:
            try:
                mcp.tool("brp_stop_watch", {"watch_id": watch_id})
            except Exception as error:
                cleanup_errors.append(f"stop watch: {error}")
        if pid is not None:
            try:
                shutdown = call("brp_shutdown", app_name="event_test")
                assert shutdown["metadata"]["shutdown_method"] == "clean_shutdown"
                # Acceptance precedes AppExit; allow the host's final frames and normal exit.
                wait_until(lambda: not fixture_alive(pid), seconds=15)
            except Exception as error:
                cleanup_errors.append(f"normal shutdown: {error}")
            try:
                if fixture_alive(pid):
                    # The normal tool call may fail or leave a host running; never kill an unverified PID.
                    killed = subprocess.run(["taskkill", "/PID", str(pid), "/T", "/F"], capture_output=True)
                    assert killed.returncode == 0 or not fixture_alive(pid), killed.stderr
                    cleanup_errors.append("owned fixture required forced cleanup")
                wait_until(lambda: not fixture_alive(pid))
                wait_until(lambda: not listening(args.port) and not listening(15703))
            except Exception as error:
                cleanup_errors.append(f"fixture cleanup: {error}")
        try:
            mcp.close()
        except Exception as error:
            cleanup_errors.append(f"MCP cleanup: {error}")
        (output / "cleanup.json").write_text(json.dumps({"errors": cleanup_errors,
            "original_error": str(original_error) if original_error else None}, indent=2), encoding="utf-8")
        if cleanup_errors and original_error is None:
            raise RuntimeError("; ".join(cleanup_errors))
    check = subprocess.run(["pwsh", "-NoProfile", "-Command",
        f"if (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ exit 1 }}"], capture_output=True)
    assert check.returncode == 0 and mcp.process.returncode == 0
    results["bind_failures"] = [verify_bind_failure(repo, output, args.port, port) for port in (args.port, 15703)]
    results.update({"clean_shutdown": True, "ports_released": True, "fixture_exited": True, "mcp_exited": True,
        "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
        "binary_sha256": {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in
            (repo / "target/debug/bevy_brp_mcp.exe", repo / "target/debug/examples/event_test.exe")}})
    (output / "summary.json").write_text(json.dumps(results, indent=2), encoding="utf-8")
    print("Main/Render, reflection CRUD, observer, watch, SSE, deadline and cleanup passed.")


if __name__ == "__main__":
    main()
