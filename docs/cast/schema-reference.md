# CAST Schema Reference

The **CAST** (Configuration And Statement Template) format is a JSON-based language for defining agent tasks, workflows, and capabilities in Exosphere. This document provides a complete reference of all Statement, Expression, and AI construct variants with JSON examples.

## Program Structure

A CAST program is the top-level container:

```json
{
  "cast_version": "0.1.0",
  "entry": "main",
  "lang": null,
  "functions": {
    "main": {
      "params": [],
      "body": [],
      "meta": {}
    }
  },
  "ai_meta": null
}
```

- **cast_version** (string, required): Version of the CAST specification
- **entry** (string, required): Entry point function name
- **lang** (string, optional): Primary language (null for polyglot)
- **functions** (object, required): Map of function name → Function definition
- **ai_meta** (object, optional): Global AI metadata

## Capsule Manifest Capabilities

`capsule.toml` declares runtime capability requests under `[capabilities]`.
`required` and `optional` are arrays of strings. Each string uses one of these
formats:

```toml
[capabilities]
required = [
  "ai.query",
  "ai.tool_chain",
  "ai.agent_delegation",
  "fs.read:/data/foo",
  "fs.write:/data/output",
  "net.tcp.connect:example.com:443"
]
optional = ["time", "crypto.sign"]
```

### Capability type names

The leading token before `.` (or the entire string for a bare type) selects
the coarse capability boundary. Recognised names:

| Canonical    | Aliases                | Boundary covers |
|--------------|------------------------|-----------------|
| `memory`     |                        | RAM allocation, encrypted buffers (`memory.encrypt`) |
| `thread`     |                        | Thread spawn / control |
| `io`         | `print`                | Generic byte I/O including stdout (`print` is short for `io`) |
| `network`    | `net`                  | TCP/UDP/HTTP, including `net.tcp.connect:host:port` |
| `crypto`     | `keyring`              | Cryptographic operations + key material (keyring access) |
| `time`       |                        | Wall-clock time, sleep |
| `icc`        |                        | Inter-capsule communication |
| `platform`   | `system`               | OS-level info, metrics (`system.metrics`) |
| `storage`    | `block`                | Block / object storage |
| `fs`         | `filesystem`           | Filesystem with path scopes (`fs.read:/path`) |
| `graphics`   | `display`, `gui`, `gpu`| Display, UI toolkit, graphics hardware |
| `debug`      |                        | Debug control / inspect / trace |
| `process`    |                        | Process management |
| `environment`| `env`                  | Environment variables |
| `bus`        | `message_bus`          | Pub/sub message bus |
| `task`       | `capsule`              | High-level capsule/task lifecycle |
| `ai`         |                        | LLM query / tool-chain / agent-delegation primitives |

Aliases parse to the same `CapabilityType` enum value as their canonical name;
the runtime cannot tell them apart at enforcement time. Prefer the canonical
spelling in new code; aliases exist for compatibility with capsule manifests
that already shipped.

Action and scope conventions:
- An action after `.` (e.g. `fs.read`) is a free string; it is recorded but
  not validated against a per-type whitelist.
- A scope after `:` (e.g. `fs.read:/data`) is a free string; the runtime
  matches by exact value at enforcement time. A capability without a scope
  is a wildcard within its action.

## Function Definition

Each function in the `functions` map has this structure:

```json
{
  "params": [
    ["param_name", "string"],
    ["count", "int"]
  ],
  "body": [],
  "meta": {}
}
```

- **params** (array, required): List of `[name, type]` pairs for parameters
- **body** (array, required): List of Statement objects
- **meta** (object, required): Function metadata (can be empty `{}`)

## Statements

A **Statement** is an executable action. All statements must have a `type` field.

### VarDecl

Declare a variable:

```json
{
  "type": "VarDecl",
  "name": "message",
  "value": {
    "type": "StringLiteral",
    "value": "hello"
  },
  "type_hint": "string",
  "meta": {}
}
```

- **name** (string): Variable name
- **value** (Expression): Initial value
- **type_hint** (string, optional): Type hint (default: inferred)
- **meta** (object): Metadata

### ExprStmt

Execute an expression as a statement:

```json
{
  "type": "ExprStmt",
  "expr": {
    "type": "CapabilityCall",
    "name": "print",
    "args": [
      {"type": "StringLiteral", "value": "hello"}
    ],
    "meta": {}
  },
  "meta": {}
}
```

- **expr** (Expression): Expression to execute
- **meta** (object): Metadata

### Export

Export a value from the function:

```json
{
  "type": "Export",
  "name": "result",
  "value": {
    "type": "IntLiteral",
    "value": 42
  },
  "meta": {}
}
```

- **name** (string): Export name
- **value** (Expression): Value to export
- **meta** (object): Metadata

### If

Conditional statement with optional else branch:

```json
{
  "type": "If",
  "condition": {
    "type": "BinaryOp",
    "operator": ">",
    "left": {"type": "Var", "name": "x"},
    "right": {"type": "IntLiteral", "value": 10}
  },
  "then_body": [
    {
      "type": "ExprStmt",
      "expr": {
        "type": "CapabilityCall",
        "name": "print",
        "args": [{"type": "StringLiteral", "value": "x is greater"}],
        "meta": {}
      }
    }
  ],
  "else_body": null,
  "meta": {}
}
```

- **condition** (Expression): Condition to evaluate
- **then_body** (array): Statements if condition is true
- **else_body** (array, optional): Statements if condition is false
- **meta** (object): Metadata

### While

Loop while condition is true:

```json
{
  "type": "While",
  "condition": {
    "type": "BinaryOp",
    "operator": "<",
    "left": {"type": "Var", "name": "i"},
    "right": {"type": "IntLiteral", "value": 10}
  },
  "body": [
    {
      "type": "VarDecl",
      "name": "i",
      "value": {
        "type": "BinaryOp",
        "operator": "+",
        "left": {"type": "Var", "name": "i"},
        "right": {"type": "IntLiteral", "value": 1}
      }
    }
  ],
  "meta": {}
}
```

- **condition** (Expression): Loop condition
- **body** (array): Statements to repeat
- **meta** (object): Metadata

### For

Iterate over a collection:

```json
{
  "type": "For",
  "variable": "item",
  "iterable": {
    "type": "ArrayLiteral",
    "elements": [
      {"type": "StringLiteral", "value": "a"},
      {"type": "StringLiteral", "value": "b"}
    ]
  },
  "body": [
    {
      "type": "ExprStmt",
      "expr": {
        "type": "CapabilityCall",
        "name": "print",
        "args": [{"type": "Var", "name": "item"}],
        "meta": {}
      }
    }
  ],
  "meta": {}
}
```

- **variable** (string): Loop variable name
- **iterable** (Expression): Collection to iterate
- **body** (array): Statements in loop body
- **meta** (object): Metadata

### Return

Return from function:

```json
{
  "type": "Return",
  "value": {
    "type": "IntLiteral",
    "value": 42
  },
  "meta": {}
}
```

- **value** (Expression, optional): Value to return
- **meta** (object): Metadata

### Break / Continue

Loop control statements:

```json
{
  "type": "Break",
  "meta": {}
}
```

```json
{
  "type": "Continue",
  "meta": {}
}
```

### TryCatch

Error handling:

```json
{
  "type": "TryCatch",
  "body": [
    {
      "type": "ExprStmt",
      "expr": {
        "type": "Call",
        "function": "risky_operation",
        "args": []
      }
    }
  ],
  "error_var": "e",
  "handler": [
    {
      "type": "ExprStmt",
      "expr": {
        "type": "CapabilityCall",
        "name": "print",
        "args": [
          {"type": "StringLiteral", "value": "Error caught"}
        ],
        "meta": {}
      }
    }
  ],
  "meta": {}
}
```

- **body** (array): Try block statements
- **error_var** (string): Variable name for caught error
- **handler** (array): Handler block statements
- **meta** (object): Metadata

### Throw

Throw an error:

```json
{
  "type": "Throw",
  "value": {
    "type": "StringLiteral",
    "value": "Something went wrong"
  },
  "meta": {}
}
```

- **value** (Expression): Error value
- **meta** (object): Metadata

### FunctionDef

Define a nested function:

```json
{
  "type": "FunctionDef",
  "name": "helper",
  "params": [["x", "int"]],
  "body": [
    {
      "type": "Return",
      "value": {
        "type": "BinaryOp",
        "operator": "*",
        "left": {"type": "Var", "name": "x"},
        "right": {"type": "IntLiteral", "value": 2}
      }
    }
  ],
  "meta": {}
}
```

- **name** (string): Function name
- **params** (array): `[name, type]` pairs
- **body** (array): Function body
- **meta** (object): Metadata

### Import

Import modules or capabilities:

```json
{
  "type": "Import",
  "import": {
    "type": "CrushModule",
    "module_path": "std/io",
    "alias": "io"
  },
  "meta": {}
}
```

See [ImportStatement variants](#importstatement) for details.

### LangBlock

Execute code in a language sandbox:

```json
{
  "type": "LangBlock",
  "lang": "python",
  "code": "result = x * 2",
  "variables": ["x"],
  "imports": [],
  "meta": {}
}
```

- **lang** (string): Language (python, javascript, rust, etc.)
- **code** (string): Raw source code
- **variables** (array): Variables to inject
- **imports** (array): Import statements
- **meta** (object): Metadata

### StructDef

Define a struct type:

```json
{
  "type": "StructDef",
  "name": "Person",
  "fields": [
    ["name", "string"],
    ["age", "int"]
  ],
  "meta": {}
}
```

- **name** (string): Struct type name
- **fields** (array): `[name, type]` pairs
- **meta** (object): Metadata

### SetField

Set a field on an object:

```json
{
  "type": "SetField",
  "target": {"type": "Var", "name": "person"},
  "field": "name",
  "value": {"type": "StringLiteral", "value": "Alice"},
  "meta": {}
}
```

- **target** (Expression): Object to modify
- **field** (string): Field name
- **value** (Expression): New value
- **meta** (object): Metadata

### DomMutate

DOM manipulation (browser contexts):

```json
{
  "type": "DomMutate",
  "target": {
    "type": "DomQuery",
    "query_type": "QuerySelector",
    "selector": {"type": "StringLiteral", "value": "#button"}
  },
  "mutation_type": "SetTextContent",
  "value": {"type": "StringLiteral", "value": "Click me"},
  "meta": {}
}
```

- **target** (Expression): DOM element
- **mutation_type** (enum): SetTextContent | SetAttribute | RemoveAttribute | SetStyle | SetInnerHtml | AppendHtml | Remove | AddClass | RemoveClass
- **value** (Expression, optional): New value
- **value2** (Expression, optional): Second value (for some mutations)
- **meta** (object): Metadata

### DomEventListener

Attach event listener to DOM element:

```json
{
  "type": "DomEventListener",
  "target": {
    "type": "DomQuery",
    "query_type": "QuerySelector",
    "selector": {"type": "StringLiteral", "value": "#button"}
  },
  "event": "click",
  "callback": {
    "type": "Lambda",
    "params": [["event", "object"]],
    "body": [
      {
        "type": "ExprStmt",
        "expr": {
          "type": "CapabilityCall",
          "name": "print",
          "args": [{"type": "StringLiteral", "value": "clicked"}],
          "meta": {}
        }
      }
    ]
  },
  "meta": {}
}
```

- **target** (Expression): DOM element
- **event** (string): Event name (click, change, etc.)
- **callback** (Expression): Callback function
- **meta** (object): Metadata

### AI Statement

AI-native orchestration:

```json
{
  "type": "AI",
  "ai_type": "Query",
  "query": "What is the capital of France?",
  "result_type": "string"
}
```

See [AIStatement variants](#aistatement) for details.

## Expressions

An **Expression** is a value-producing computation. All expressions must have a `type` field.

### Literal Expressions

#### IntLiteral
```json
{"type": "IntLiteral", "value": 42}
```

#### FloatLiteral
```json
{"type": "FloatLiteral", "value": 3.14}
```

#### StringLiteral
```json
{"type": "StringLiteral", "value": "hello"}
```

#### BoolLiteral
```json
{"type": "BoolLiteral", "value": true}
```

#### NullLiteral
```json
{"type": "NullLiteral"}
```

#### ArrayLiteral
```json
{
  "type": "ArrayLiteral",
  "elements": [
    {"type": "IntLiteral", "value": 1},
    {"type": "IntLiteral", "value": 2}
  ]
}
```

#### ObjectLiteral
```json
{
  "type": "ObjectLiteral",
  "properties": [
    ["name", {"type": "StringLiteral", "value": "Alice"}],
    ["age", {"type": "IntLiteral", "value": 30}]
  ]
}
```

### Var

Reference a variable:

```json
{"type": "Var", "name": "x"}
```

### Call

Call a function:

```json
{
  "type": "Call",
  "function": "greet",
  "args": [
    {"type": "StringLiteral", "value": "hello"}
  ]
}
```

**Note**: There is no `Print` statement. `Call` targets must be functions defined
in `Program.functions` — the semantic analyzer rejects anything else as
`Undefined function`, including `Call("print", ...)`, which **validates but does
not compile** (there is no builtin registry yet). To print, use
`CapabilityCall("print", ...)` — see the next section; that is the pattern the
production capsules under `projects/crush-capsules/` use.

### CapabilityCall

Call a capability:

```json
{
  "type": "CapabilityCall",
  "name": "file_read",
  "args": [
    {"type": "StringLiteral", "value": "/path/to/file"}
  ],
  "meta": {}
}
```

### BinaryOp

Binary operation:

```json
{
  "type": "BinaryOp",
  "operator": "+",
  "left": {"type": "IntLiteral", "value": 1},
  "right": {"type": "IntLiteral", "value": 2}
}
```

Supported operators: `+`, `-`, `*`, `/`, `%`, `>`, `<`, `>=`, `<=`, `==`, `!=`, `&&`, `||`, etc.

### UnaryOp

Unary operation:

```json
{
  "type": "UnaryOp",
  "operator": "!",
  "operand": {"type": "BoolLiteral", "value": true}
}
```

### Pipeline

Chain expressions:

```json
{
  "type": "Pipeline",
  "segments": [
    {"type": "Call", "function": "read_file", "args": []},
    {"type": "Call", "function": "parse_json", "args": []},
    {"type": "Call", "function": "validate", "args": []}
  ]
}
```

### Spawn

Spawn a concurrent task:

```json
{
  "type": "Spawn",
  "function": "background_task",
  "args": []
}
```

### Lambda

Anonymous function:

```json
{
  "type": "Lambda",
  "params": [
    ["x", "int"],
    ["y", "int"]
  ],
  "body": [
    {
      "type": "Return",
      "value": {
        "type": "BinaryOp",
        "operator": "+",
        "left": {"type": "Var", "name": "x"},
        "right": {"type": "Var", "name": "y"}
      }
    }
  ]
}
```

### Yield

Yield from generator:

```json
{"type": "Yield"}
```

### GetField

Access object field:

```json
{
  "type": "GetField",
  "target": {"type": "Var", "name": "person"},
  "field": "name"
}
```

### Index

Array/object indexing:

```json
{
  "type": "Index",
  "target": {"type": "Var", "name": "arr"},
  "index": {"type": "IntLiteral", "value": 0}
}
```

### Range

Create a range:

```json
{
  "type": "Range",
  "start": {"type": "IntLiteral", "value": 1},
  "end": {"type": "IntLiteral", "value": 10}
}
```

### Await

Wait for async operation:

```json
{
  "type": "Await",
  "expression": {
    "type": "Call",
    "function": "async_task",
    "args": []
  }
}
```

### Match

Pattern matching:

```json
{
  "type": "Match",
  "expression": {"type": "Var", "name": "status"},
  "arms": [
    {
      "pattern": {"type": "Literal", "value": {"type": "StringLiteral", "value": "ok"}},
      "body": [
        {"type": "ExprStmt", "expr": {"type": "CapabilityCall", "name": "print", "args": [{"type": "StringLiteral", "value": "Success"}], "meta": {}}}
      ]
    },
    {
      "pattern": {"type": "Wildcard"},
      "body": [
        {"type": "ExprStmt", "expr": {"type": "CapabilityCall", "name": "print", "args": [{"type": "StringLiteral", "value": "Other"}], "meta": {}}}
      ]
    }
  ]
}
```

### DomQuery

Query DOM elements:

```json
{
  "type": "DomQuery",
  "query_type": "QuerySelector",
  "selector": {"type": "StringLiteral", "value": ".button"}
}
```

Query types: `QuerySelector`, `QuerySelectorAll`, `GetElementById`, `GetElementsByClassName`, `GetElementsByTagName`

### NewStruct

Create struct instance:

```json
{
  "type": "NewStruct",
  "name": "Person"
}
```

### AI Expression

AI-native expressions:

```json
{
  "type": "AI",
  "ai_type": "Query",
  "query": "Summarize this text",
  "result_type": "string"
}
```

See [AIExpression variants](#aiexpression) for details.

## AI Constructs

AI-specific statements and expressions for agent orchestration.

### AIExpression

AI expressions handle natural language queries, tool chains, and agent delegation.

#### Query

Execute a natural language query:

```json
{
  "type": "AI",
  "ai_type": "Query",
  "query": "What is 2 + 2?",
  "result_type": "int",
  "context": {
    "domain": "math"
  }
}
```

- **query** (string): Natural language query
- **result_type** (string, optional): Expected result type
- **context** (object, optional): Additional context

#### ToolChain

Chain tool calls with execution strategy:

```json
{
  "type": "AI",
  "ai_type": "ToolChain",
  "tools": [
    {
      "tool_name": "search",
      "parameters": {"query": "Python tutorial"},
      "result_binding": "search_result"
    },
    {
      "tool_name": "summarize",
      "parameters": {"text": {"ref": "search_result"}},
      "result_binding": "summary"
    }
  ],
  "strategy": {
    "type": "Sequential"
  },
  "error_handling": {
    "type": "FailFast"
  }
}
```

- **tools** (array): Tool calls to execute
- **strategy** (ExecutionStrategy): Execution order/method
- **error_handling** (ErrorHandling): Error handling strategy

**ExecutionStrategy variants:**
- `{"type": "Sequential"}` — tools run one after another
- `{"type": "Parallel"}` — tools run concurrently
- `{"type": "Conditional", "conditions": ["tool1", "tool2"]}` — conditional execution
- `{"type": "Retry", "max_attempts": 3, "backoff_strategy": {"type": "Exponential"}}` — retry failed tools

**ErrorHandling variants:**
- `{"type": "FailFast"}` — stop on first error
- `{"type": "ContinueOnError"}` — skip failed tools, continue
- `{"type": "Retry", "max_retries": 3, "retry_condition": "status != ok"}` — retry specific errors
- `{"type": "Fallback", "fallback_tools": [...]}` — fallback tools to run on error

#### AgentDelegation

Delegate task to other agents:

```json
{
  "type": "AI",
  "ai_type": "AgentDelegation",
  "task": "Analyze the codebase for performance issues",
  "agents": ["analyzer/*", "profiler"],
  "delegation_strategy": "Best",
  "expected_format": "report"
}
```

- **task** (string): Task description
- **agents** (array): Target agent names/patterns
- **delegation_strategy** (string): Strategy for selecting agents:
  - `"FirstAvailable"` — use first responding agent
  - `"CapabilityMatch"` — select agents matching task capability
  - `"ParallelSplit"` — distribute work across agents
  - `"Hierarchical"` — multi-level delegation
  - `"Consensus"` — require agreement from agents
  - `"Broadcast"` — send to all agents in parallel
  - `"Best"` — pick highest-rated agent
  - `"RoundRobin"` — cycle through agents
- **expected_format** (string, optional): Format of result

#### LearningLoop

Enable agent learning and adaptation:

```json
{
  "type": "AI",
  "ai_type": "LearningLoop",
  "learning_target": "UserBehavior",
  "strategy": "PatternRecognition",
  "adaptations": [
    {
      "trigger": "error_rate > 0.1",
      "action": "RetryWithModifiedParameters"
    }
  ]
}
```

- **learning_target** (string): What to learn from:
  - `"UserBehavior"`
  - `"ExecutionPatterns"`
  - `"ErrorPatterns"`
  - `"PerformanceMetrics"`
  - `"ToolUsage"`
- **strategy** (string): Learning strategy:
  - `"PatternRecognition"`
  - `"StatisticalAnalysis"`
  - `"MachineLearning"`
  - `"RuleBased"`
- **adaptations** (array): Adaptation actions to take

#### ContextAware

Require/provide context:

```json
{
  "type": "AI",
  "ai_type": "ContextAware",
  "expression": {
    "type": "Call",
    "function": "task",
    "args": []
  },
  "requires_context": ["user_id", "session_token"],
  "provides_context": ["task_result", "execution_time"]
}
```

- **expression** (Expression): Expression to execute
- **requires_context** (array): Context variables needed
- **provides_context** (array): Context variables provided

### AIStatement

AI-specific statements (same variants as AIExpression, used at statement level).

## ImportStatement

Import variants for bringing in code and capabilities.

### CrushModule

Import a Crush module:

```json
{
  "type": "CrushModule",
  "module_path": "std/io",
  "alias": "io",
  "selective": ["read_file", "write_file"]
}
```

### PolyglotModule

Import from another language:

```json
{
  "type": "PolyglotModule",
  "language": "python",
  "module_path": "numpy",
  "alias": "np"
}
```

### MCPImport

Import from MCP server:

```json
{
  "type": "MCPImport",
  "server_url": "http://localhost:3000/mcp",
  "tools": ["tool1", "tool2"],
  "alias": "mcp"
}
```

### Capability

Import a capability:

```json
{
  "type": "Capability",
  "capability_path": "file_system/read",
  "permissions": ["read"],
  "alias": "fs_read"
}
```

### External

Import external resource:

```json
{
  "type": "External",
  "uri": "https://example.com/data.json",
  "resource_type": "JSON",
  "alias": "data"
}
```

### SecureEnv

Import encrypted secrets:

```json
{
  "type": "SecureEnv",
  "selective": ["DATABASE_URL", "API_KEY"]
}
```

## Common Mistakes and Fixes

Here are the most common schema errors and their fixes:

| Error | Wrong | Right |
|-------|-------|-------|
| `Print` statement doesn't exist | `{"type": "Print"}` | `{"type": "ExprStmt", "expr": {"type": "CapabilityCall", "name": "print", "args": [...], "meta": {}}}` (a `Call` to `print` validates but fails to compile: `print` is a capability, not a function) |
| `Identifier` expression doesn't exist | `{"type": "Identifier", "name": "x"}` | `{"type": "Var", "name": "x"}` |
| `Literal` expression doesn't exist | `{"type": "Literal", "value": "x"}` | `{"type": "StringLiteral", "value": "x"}` |
| Missing `meta` field | `{"type": "VarDecl", "name": "x"}` | `{"type": "VarDecl", "name": "x", "meta": {}}` |
| AI Query field name | `{"ai_type": "Query", "prompt": "..."}` | `{"ai_type": "Query", "query": "..."}` |
| DelegationStrategy shape | `{"delegation_strategy": {"type": "Best"}}` | `{"delegation_strategy": "Best"}` (plain string) |

## Type Reference

Valid CastType values (for parameters and type hints):

- `"string"`
- `"int"`
- `"float"`
- `"bool"`
- `"any"`
- `"object"`
- `"array"`
- `"null"`

## Validation

To validate a CAST file (JSON or binary), use:

```bash
ant crush cast validate <file.cast.json>
ant crush cast validate <file.castb>
```

Exit codes:
- `0` — valid
- `1` — validation failed

For programmatic validation, use the `crush_cast::validate_json()` function which returns detailed error information with JSON paths. Binary files are validated by deserializing into `Program`, which enforces the same schema constraints plus the version gate.

## Binary Format

CAST is dual-format (EXO-176). JSON (`.cast.json`) stays the canonical authoring and debug form; the binary form is CBOR via `cbor4ii` — the same binary codec family the mesh wire protocol uses. The encoding is exactly the serde derivation of the JSON schema above: no custom header, no magic bytes, no schema changes.

**Extension convention:** `.castb` is the binary extension (mirroring CASM's `.casmb`); `.cbor` is also recognized. Anything else is treated as JSON. Format selection on load is by extension (`crush_cast::Format::from_path`), with an explicit `Format` argument API underneath (`Program::serialize` / `Program::deserialize`).

```bash
# JSON → binary
ant crush cast pack examples/cast/cookbook/code-review-helper/main.cast.json
# → main.castb next to the input; -o <path> to choose

# binary → pretty JSON (the debug direction; stdout by default)
ant crush cast unpack main.castb
ant crush cast unpack main.castb -o main.cast.json
```

**Version gate:** every load path (JSON and binary) checks `cast_version` against the supported `crush_cast::CAST_VERSION` (currently `0.1`) on the **major** component and fails closed on a mismatch or unparseable version — modeled on the CASM VER-02 gate, reported through the unified `crush_errors::VersionMismatch` shape (boundary = `cast`).
