# code-review-helper

Cookbook capsule demonstrating AI-native primitives for code review workflow.

## Primitives Exercised

- **AgentDelegation** — delegates code review to a reviewer agent
- **ToolChain** — chains git diff (staged + unstaged) in parallel
- **AIStatement** — GoalDeclaration, KnowledgeSharing, ProgressUpdate

## How to Run

```bash
# Validate the CAST file
ant crush cast validate examples/cast/cookbook/code-review-helper/main.cast.json

# Compile to bytecode
ant crush compile --from-cast examples/cast/cookbook/code-review-helper/main.cast.json -o /tmp/code-review-helper.casmb

# Install and run
ant pkg install examples/cast/cookbook/code-review-helper
ant pkg run code-review-helper
```

## Expected Output

```
Running capsule 'code-review-helper'...
✓ Capsule started with PID 1000
```

(Note: PID 1000 is a placeholder — actual runtime execution pending EXO-86 closure.)

## Capsule Structure

- `main.cast.json` — CAST program with AgentDelegation + ToolChain + AIStatement
- `capsule.toml` — capsule manifest with ai.query, ai.tool_chain, ai.agent_delegation, fs, and net capabilities
