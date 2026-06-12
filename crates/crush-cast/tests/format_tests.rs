use crush_cast::format::canonical_form;

/// Fixture 1: minimal program with a main function returning an integer.
const FIXTURE_1_CANONICAL: &str = r#"{
  "cast_version": "1.0",
  "entry": "main",
  "functions": {
    "main": {
      "body": [
        {
          "name": "x",
          "type": "VarDecl",
          "value": {
            "type": "IntLiteral",
            "value": 10
          }
        },
        {
          "type": "Return",
          "value": {
            "name": "x",
            "type": "Var"
          }
        }
      ],
      "meta": {},
      "params": []
    }
  },
  "lang": "crush"
}"#;

const FIXTURE_1_SHUFFLED_A: &str = r#"{
  "functions": {
    "main": {
      "params": [],
      "body": [
        {
          "type": "VarDecl",
          "value": {"type": "IntLiteral", "value": 10},
          "name": "x",
          "meta": {},
          "type_hint": "Any"
        },
        {
          "type": "Return",
          "value": {"type": "Var", "name": "x"},
          "meta": {}
        }
      ],
      "meta": {}
    }
  },
  "cast_version": "1.0",
  "entry": "main",
  "lang": "crush",
  "ai_meta": null
}"#;

const FIXTURE_1_SHUFFLED_B: &str = r#"{
  "entry": "main",
  "cast_version": "1.0",
  "lang": "crush",
  "ai_meta": null,
  "functions": {
    "main": {
      "meta": {},
      "body": [
        {
          "name": "x",
          "meta": {},
          "type_hint": "Any",
          "type": "VarDecl",
          "value": {
            "value": 10,
            "type": "IntLiteral"
          }
        },
        {
          "type": "Return",
          "meta": {},
          "value": {
            "name": "x",
            "type": "Var"
          }
        }
      ],
      "params": []
    }
  }
}"#;

/// Fixture 2: program with If, BinaryOp, and FunctionDef.
const FIXTURE_2_CANONICAL: &str = r#"{
  "cast_version": "1.0",
  "entry": "main",
  "functions": {
    "main": {
      "body": [
        {
          "name": "x",
          "type": "VarDecl",
          "value": {
            "left": {
              "type": "IntLiteral",
              "value": 1
            },
            "operator": "+",
            "right": {
              "type": "IntLiteral",
              "value": 2
            },
            "type": "BinaryOp"
          }
        },
        {
          "condition": {
            "left": {
              "name": "x",
              "type": "Var"
            },
            "operator": ">",
            "right": {
              "type": "IntLiteral",
              "value": 0
            },
            "type": "BinaryOp"
          },
          "then_body": [
            {
              "type": "Return",
              "value": {
                "name": "x",
                "type": "Var"
              }
            }
          ],
          "type": "If"
        },
        {
          "type": "Return",
          "value": {
            "type": "IntLiteral",
            "value": 0
          }
        }
      ],
      "meta": {},
      "params": []
    }
  },
  "lang": "crush"
}"#;

const FIXTURE_2_SHUFFLED: &str = r#"{
  "lang": "crush",
  "entry": "main",
  "cast_version": "1.0",
  "ai_meta": null,
  "functions": {
    "main": {
      "params": [],
      "meta": {},
      "body": [
        {
          "type": "VarDecl",
          "name": "x",
          "meta": {},
          "type_hint": "Any",
          "value": {
            "type": "BinaryOp",
            "operator": "+",
            "left": {"type": "IntLiteral", "value": 1},
            "right": {"type": "IntLiteral", "value": 2}
          }
        },
        {
          "type": "If",
          "meta": {},
          "condition": {
            "type": "BinaryOp",
            "operator": ">",
            "left": {"type": "Var", "name": "x"},
            "right": {"type": "IntLiteral", "value": 0}
          },
          "then_body": [
            {
              "type": "Return",
              "meta": {},
              "value": {"type": "Var", "name": "x"}
            }
          ]
        },
        {
          "type": "Return",
          "meta": {},
          "value": {"type": "IntLiteral", "value": 0}
        }
      ]
    }
  }
}"#;

/// Fixture 3: program with imports and a lang block.
const FIXTURE_3_CANONICAL: &str = r#"{
  "cast_version": "1.0",
  "entry": "main",
  "functions": {
    "main": {
      "body": [
        {
          "code": "print('hello')",
          "lang": "python",
          "type": "LangBlock"
        },
        {
          "type": "Return"
        }
      ],
      "meta": {},
      "params": []
    }
  },
  "lang": "crush"
}"#;

const FIXTURE_3_SHUFFLED: &str = r#"{
  "cast_version": "1.0",
  "functions": {
    "main": {
      "body": [
        {
          "type": "LangBlock",
          "lang": "python",
          "code": "print('hello')",
          "variables": [],
          "imports": [],
          "meta": {}
        },
        {
          "type": "Return",
          "value": null,
          "meta": {}
        }
      ],
      "params": [],
      "meta": {}
    }
  },
  "entry": "main",
  "lang": "crush",
  "ai_meta": null
}"#;

fn parse_program(json: &str) -> crush_cast::Program {
    serde_json::from_str(json).expect("valid CAST JSON")
}

#[test]
fn format_normalizes_key_order_and_whitespace() {
    let p1a = parse_program(FIXTURE_1_SHUFFLED_A);
    let p1b = parse_program(FIXTURE_1_SHUFFLED_B);

    let out_a = canonical_form(&p1a);
    let out_b = canonical_form(&p1b);

    assert_eq!(
        out_a, out_b,
        "Two shuffled variants of the same program should format identically"
    );
    assert_eq!(out_a, FIXTURE_1_CANONICAL, "Output should match canonical fixture");
}

#[test]
fn format_is_idempotent_for_fixture_1() {
    let p = parse_program(FIXTURE_1_SHUFFLED_A);
    let once = canonical_form(&p);
    let p2 = parse_program(&once);
    let twice = canonical_form(&p2);
    assert_eq!(once, twice, "format(format(x)) should equal format(x)");
}

#[test]
fn format_normalizes_fixture_2() {
    let p = parse_program(FIXTURE_2_SHUFFLED);
    let out = canonical_form(&p);
    assert_eq!(out, FIXTURE_2_CANONICAL);
}

#[test]
fn format_is_idempotent_for_fixture_2() {
    let p = parse_program(FIXTURE_2_SHUFFLED);
    let once = canonical_form(&p);
    let p2 = parse_program(&once);
    let twice = canonical_form(&p2);
    assert_eq!(once, twice);
}

#[test]
fn format_normalizes_fixture_3() {
    let p = parse_program(FIXTURE_3_SHUFFLED);
    let out = canonical_form(&p);
    assert_eq!(out, FIXTURE_3_CANONICAL);
}

#[test]
fn format_is_idempotent_for_fixture_3() {
    let p = parse_program(FIXTURE_3_SHUFFLED);
    let once = canonical_form(&p);
    let p2 = parse_program(&once);
    let twice = canonical_form(&p2);
    assert_eq!(once, twice);
}

#[test]
fn format_preserves_round_trip_semantics() {
    // Parse a shuffled file, format it, parse the output, format again — both
    // should be byte-identical and the deserialized programs should be equal
    // when compared via their canonical forms.
    let fixtures = [
        FIXTURE_1_SHUFFLED_A,
        FIXTURE_1_SHUFFLED_B,
        FIXTURE_2_SHUFFLED,
        FIXTURE_3_SHUFFLED,
    ];

    for (i, json) in fixtures.iter().enumerate() {
        let p1 = parse_program(json);
        let c1 = canonical_form(&p1);

        let p2 = parse_program(&c1);
        let c2 = canonical_form(&p2);

        assert_eq!(
            c1, c2,
            "Round-trip semantic equivalence failed for fixture {}",
            i
        );
    }
}
