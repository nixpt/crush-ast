import os
import sys
import json

# Add the python bindings path
sys.path.append(os.path.join(os.path.dirname(__file__), '../../crates/crush-cast/python'))

try:
    from cast_types import (
        Program, Function, VarDecl, If, ExprStmt, Return, Call, StringLiteral, Var,
        AIExpr, AIStmt, SemanticSwitch, SemanticMatch, Synthesize,
        ModuleManifest, WipNode, TemporaryNode, DecisionNode, Invariant,
        FunctionAnnotations, WeightedError
    )
    import dataclasses
except ImportError:
    print("Failed to import cast_types. Ensure you ran `cargo run -p crush-cast --bin export-py` first.")
    sys.exit(1)

def main():
    # Define semantic switch cases
    cases = [
        ("User is asking for a refund or billing help", [
            Return(value=AIExpr(ai=Synthesize(
                output_type="RefundResponse",
                constraints=["polite", "explain 5-7 business days"],
                context_refs=[],
                examples=None
            )), meta={})
        ]),
        ("Technical issue or bug report", [
            VarDecl(name="is_critical", value=AIExpr(ai=SemanticMatch(
                target=Var(name="intent_text"),
                concept="Data loss, security vulnerability, or system down",
                confidence_threshold=0.85
            )), type_hint={"type": "Any"}, meta={}),
            If(condition=Var(name="is_critical"), then_body=[
                ExprStmt(expr=Call(function="escalate_issue", args=[
                    Var(name="intent_text")
                ], meta={}), meta={}),
                Return(value=StringLiteral(value="Your issue has been escalated to engineering immediately."), meta={})
            ], else_body=None, meta={}),
            Return(value=StringLiteral(value="Please provide a log file so we can investigate."), meta={})
        ])
    ]

    fallback = [
        Return(value=StringLiteral(value="I'm not sure how to help. Let me connect you to a human."), meta={})
    ]

    # Process function body
    process_body = [
        AIStmt(ai=SemanticSwitch(
            target=Var(name="intent_text"),
            cases=cases,
            fallback=fallback
        ))
    ]

    # Assemble the Program
    program = Program(
        cast_version="0.2.0",
        entry="process_user_intent",
        lang="crush",
        functions={
            "process_user_intent": Function(
                params=[("intent_text", {"type": "String"})],
                body=process_body,
                meta={},
                annotations=FunctionAnnotations(
                    errors=[],
                    errors_weighted=[
                        WeightedError(variant="NetworkTimeout", likelihood="likely"),
                        WeightedError(variant="DatabaseConnectionError", likelihood="rare"),
                        WeightedError(variant="Unauthorized", likelihood="possible")
                    ],
                    reads=[], writes=[], does_not_write=[], covers=[], relies_on=[],
                    complexity=None
                )
            )
        },
        ai_meta=None,
        manifest=ModuleManifest(
            purpose="Demonstrate AI-native capabilities of Crush (Phase 1 & Phase 2a features)",
            exports=["process_user_intent"],
            invariants=[
                Invariant(
                    name="escalation-requires-auth",
                    description="Any operation that escalates an issue must verify the user's session",
                    applies_to=["escalate_issue"],
                    consequence=None,
                    check_source=None
                )
            ],
            related=[], exhaustive_types=[], changelog=[]
        ),
        exhaustive_sites=[],
        wip=WipNode(
            intent="Build an autonomous support agent capable of escalating complex issues",
            started_by=None,
            done=["basic routing", "semantic matching"],
            todo=["integrate escalation API", "synthetic data generation tests"],
            unresolved=["how to handle non-English intents accurately"]
        ),
        temporaries=[
            TemporaryNode(
                reason="Using hardcoded API keys for escalation until secret manager lands in stdlib",
                expires_when="std::secrets module is available",
                owner=None,
                added="2026-06-18"
            )
        ],
        decisions=[
            DecisionNode(
                name="use-semantic-switch-routing",
                chose="semantic_switch",
                over=["regex matching", "LLM zero-shot prompt"],
                because="Semantic routing is native to VM and 10x faster than full LLM calls",
                revisit_if=["user intents become too highly contextual for basic embeddings"]
            )
        ]
    )

    # Dump the python dataclass to dict and print JSON
    program_dict = dataclasses.asdict(program)
    # Filter out None values to make the JSON cleaner
    def filter_none(obj):
        if isinstance(obj, dict):
            return {k: filter_none(v) for k, v in obj.items() if v is not None}
        elif isinstance(obj, list):
            return [filter_none(x) for x in obj if x is not None]
        return obj

    clean_program = filter_none(program_dict)
    
    print(json.dumps(clean_program, indent=2))

if __name__ == "__main__":
    main()
