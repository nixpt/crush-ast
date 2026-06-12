import type { Program, Statement, Expression } from './cast';

// Author a minimal CAST program in TypeScript
const program: Program = {
  cast_version: "1.0",
  entry: "main",
  lang: "crush",
  functions: {
    main: {
      params: [],
      body: [
        {
          type: "ExprStmt",
          expr: {
            type: "Call",
            function: "io.print",
            args: [
              { type: "StringLiteral", value: "Hello, CAST!", meta: {} }
            ],
            meta: {}
          },
          meta: {}
        },
        {
          type: "Return",
          value: null,
          meta: {}
        }
      ],
      meta: {}
    }
  },
  ai_meta: null
};

// Validate round-trip shape
const json = JSON.stringify(program);
console.log("Serialized CAST program:", json);
