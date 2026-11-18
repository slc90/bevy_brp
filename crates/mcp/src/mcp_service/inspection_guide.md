# Inspect a Bevy App through BRP

The Bevy App that hosts BRP and the stdio MCP server are separate programs. The default BRP
port is 15702. If the App uses a different port, specify its actual port in each request.

1. Read the live method directory at `bevy-brp://app/{port}/methods` or call `rpc_discover`.
2. Inspect the relevant entities and state with `world_query`, `world_get_components`, or a
   discovered read-only method through `brp_execute`.
3. Use `brp_extras_screenshot` when you need to see what the App rendered. Verify the actual UI
   state or interaction separately from whether a tool accepted the request.
