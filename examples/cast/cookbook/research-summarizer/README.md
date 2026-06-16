# research-summarizer

Cookbook capsule demonstrating AI-native primitives for research summarization workflow.

## Primitives Exercised

- **Query** — asks LLM for summary based on search results
- **ToolChain** — chains search → fetch → summarize with fallback
- **AgentDelegation** — delegates final write to writer/scribe agent
- **AIStatement** — GoalDeclaration, CapabilityDiscovery, ProgressUpdate

## How to Run

```bash
# Validate the CAST file
ant crush cast validate examples/cast/cookbook/research-summarizer/main.cast.json

# Compile to bytecode
ant crush compile --from-cast examples/cast/cookbook/research-summarizer/main.cast.json -o /tmp/research-summarizer.casmb

# Install and run
ant pkg install examples/cast/cookbook/research-summarizer
ant pkg run research-summarizer
```

## Expected Output

```
Running capsule 'research-summarizer'...
✓ Capsule started with PID 1000
```

(Note: PID 1000 is a placeholder — actual runtime execution pending EXO-86 closure.)

## Capsule Structure

- `main.cast.json` — CAST program with Query + ToolChain + AgentDelegation + AIStatement
- `capsule.toml` — capsule manifest with ai.query, ai.tool_chain, ai.agent_delegation, net, and fs capabilities

## Primitives Coverage

| Primitive | Used In |
|-----------|---------|
| Query | Line ~60: "Summarize the research findings..." |
| ToolChain | Lines ~35-55: search.query → web.fetch chain with fallback |
| AgentDelegation | Line ~70: delegate to writer/scribe agents |
| AIStatement | GoalDeclaration, CapabilityDiscovery, ProgressUpdate |
