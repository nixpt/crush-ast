use crush_cast::diff::diff_programs;
use crush_cast::format::canonical_form;

const FIXTURE_A: &str = r#"{
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

const FIXTURE_A_REORDERED: &str = r#"{
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

const FIXTURE_B: &str = r#"{
  "cast_version": "1.0",
  "entry": "main",
  "lang": "crush",
  "functions": {
    "main": {
      "params": [],
      "meta": {},
      "body": [
        {
          "type": "VarDecl",
          "name": "x",
          "value": {"type": "IntLiteral", "value": 10}
        },
        {
          "type": "VarDecl",
          "name": "y",
          "value": {"type": "IntLiteral", "value": 20}
        },
        {
          "type": "Return",
          "value": {"type": "Var", "name": "x"}
        }
      ]
    }
  }
}"#;

const FIXTURE_C: &str = r#"{
  "cast_version": "1.0",
  "entry": "main",
  "lang": "crush",
  "functions": {
    "main": {
      "params": [],
      "meta": {},
      "body": [
        {
          "type": "VarDecl",
          "name": "x",
          "value": {"type": "IntLiteral", "value": 11}
        },
        {
          "type": "Return",
          "value": {"type": "Var", "name": "x"}
        }
      ]
    }
  }
}"#;

fn parse_program(json: &str) -> crush_cast::Program {
    serde_json::from_str(json).expect("valid CAST JSON")
}

#[test]
fn diff_ignores_key_order_whitespace_and_elided_defaults() {
    let left = parse_program(FIXTURE_A);
    let right = parse_program(FIXTURE_A_REORDERED);

    assert!(diff_programs(&left, &right).is_empty());
}

#[test]
fn diff_of_formatted_program_is_empty() {
    let left = parse_program(FIXTURE_A);
    let formatted = canonical_form(&left);
    let right = parse_program(&formatted);

    assert!(diff_programs(&left, &right).is_empty());
}

#[test]
fn diff_reports_structural_additions() {
    let left = parse_program(FIXTURE_A);
    let right = parse_program(FIXTURE_B);
    let changes = diff_programs(&left, &right);

    assert!(
        changes
            .iter()
            .any(|line| line == "+ Statement::VarDecl { name: \"y\" } at functions.main.body[1]"),
        "expected inserted statement, got {changes:#?}"
    );
}

#[test]
fn diff_reports_scalar_changes() {
    let left = parse_program(FIXTURE_A);
    let right = parse_program(FIXTURE_C);
    let changes = diff_programs(&left, &right);

    assert!(
        changes
            .iter()
            .any(|line| line == "~ functions.main.body[0].value.value: 10 -> 11"),
        "expected scalar change, got {changes:#?}"
    );
}
