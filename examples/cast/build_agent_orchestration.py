import json
import sys
import os

# Add python bindings path
sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), '../../crates/crush-cast/python')))

from dataclasses import asdict
from cast_types import (
    Program, ModuleManifest, FunctionAnnotations, Function, 
    ToolChain, ToolCall, Parallel, FailFast, 
    AgentDelegation, AIStmt, ExprStmt, AIExpr
)

def build_orchestration_ast():
    # 1. Parallel ToolChain for data gathering
    fetch_data = AIStmt(
        ai=ToolChain(
            tools=[
                ToolCall(
                    tool_name="aws_cloudtrail_fetch",
                    parameters={"timeframe": "last_1h"},
                    result_binding="cloudtrail_logs",
                    condition=None
                ),
                ToolCall(
                    tool_name="db_audit_scan",
                    parameters={"table": "users", "event_type": "DROP"},
                    result_binding="db_drop_events",
                    condition=None
                )
            ],
            strategy=Parallel(),
            error_handling=FailFast()
        )
    )

    # 2. Agent Delegation to analyze the gathered data
    delegate_analysis = AIStmt(
        ai=AgentDelegation(
            task="Analyze cloudtrail_logs and db_drop_events for anomalous drop commands indicating a potential security incident.",
            agents=["@security_analyst"],
            delegation_strategy="CapabilityMatch",
            expected_format="MarkdownReport"
        )
    )

    process_body = [fetch_data, delegate_analysis]

    program = Program(
        cast_version="0.2.0",
        lang="crush",
        entry="incident_response_workflow",
        functions={
            "incident_response_workflow": Function(
                params=[],
                body=process_body,
                annotations=FunctionAnnotations(
                    errors=[],
                    errors_weighted=[],
                    reads=["cloudtrail", "db_audit"],
                    writes=[],
                    does_not_write=["production_db"],
                    covers=[],
                    relies_on=["aws_cloudtrail_fetch", "db_audit_scan"],
                    complexity=None
                ),
                meta={}
            )
        },
        manifest=ModuleManifest(purpose="Orchestration entrypoint")
    )

    return program

def main():
    ast = build_orchestration_ast()
    # Serialize to JSON (removing None values dynamically is typically handled in rust side, but we can do a simple dump)
    # The rust schema expects "type": "Parallel" etc. which our dataclasses provide.
    
    def dict_factory(data):
        return {k: v for k, v in data if v is not None}
    
    ast_json = json.dumps(asdict(ast, dict_factory=dict_factory), indent=2)
    print("AST Output:\n" + ast_json)

if __name__ == "__main__":
    main()
