import json
import sys
import os

sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), '../../crates/crush-cast/python')))

from dataclasses import asdict
from cast_types import (
    Program, ModuleManifest, FunctionAnnotations, Function, 
    Import, CrushModule, PolyglotModule, MCPImport, ExprStmt,
    Call, StringLiteral
)

def build_module_integration_ast():
    # 1. Standard library import (written in Crush)
    import_math = Import(
        import_=CrushModule(
            module_path="std.math",
            alias="math",
            selective=[]
        )
    )

    # 2. Polyglot import (bringing in Python's numpy seamlessly)
    import_numpy = Import(
        import_=PolyglotModule(
            language="python",
            module_path="numpy",
            alias="np"
        )
    )

    # 3. MCP import (connecting to a local postgres Model Context Protocol server)
    import_postgres = Import(
        import_=MCPImport(
            server_url="stdio://mcp-postgres-adapter",
            tools=["query_db", "list_tables"],
            alias="pg"
        )
    )
    
    # 4. A function that uses these imported modules
    # In a real AST, this would be represented by more complex expressions.
    # We will just show calling the MCP tool `pg.query_db`.
    call_mcp = ExprStmt(
        expr=Call(
            function="pg.query_db",
            args=[StringLiteral(value="SELECT * FROM users")],
            meta={}
        )
    )

    program = Program(
        cast_version="0.2.0",
        lang="crush",
        entry="data_analysis_workflow",
        functions={
            "data_analysis_workflow": Function(
                params=[],
                body=[import_math, import_numpy, import_postgres, call_mcp],
                annotations=FunctionAnnotations(
                    errors=[],
                    errors_weighted=[],
                    reads=["database"],
                    writes=[],
                    does_not_write=["production_db"],
                    covers=[],
                    relies_on=["mcp-postgres-adapter"],
                    complexity=None
                ),
                meta={}
            )
        },
        manifest=ModuleManifest(
            purpose="Demonstrates seamless polyglot and MCP integrations within Crush",
            exports=["data_analysis_workflow"],
            invariants=[],
            related=["numpy", "mcp.postgres"],
            exhaustive_types=[],
            changelog=[]
        )
    )

    return program

def main():
    ast = build_module_integration_ast()
    
    def dict_factory(data):
        return {k: v for k, v in data if v is not None}
    
    ast_json = json.dumps(asdict(ast, dict_factory=dict_factory), indent=2)
    print("AST Output:\n" + ast_json)

if __name__ == "__main__":
    main()
