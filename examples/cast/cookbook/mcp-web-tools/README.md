# mcp-web-tools

Demonstrates `ImportStatement::MCPImport` — connecting a capsule to an MCP
server and binding a subset of its tools.

The import compiles to a `mcp.connect` capability call (handle stored under
`alias`), followed by one `mcp.get_tool` call per listed tool, each stored
under the tool's own name. The `mcp.client` permission is recorded in the
compiled CASM manifest automatically.

```bash
ant crush cast validate main.cast.json
ant crush compile --from-cast main.cast.json -o mcp-web-tools.casmb
```
