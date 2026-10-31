//! Headless tests: run the example programs against a real bliss DOM, inject
//! events by element id, and assert on the page the program renders next.
//! No display is needed; see `src/headless.rs`.

use std::path::PathBuf;

use crush_gui::headless::HeadlessApp;
use crush_gui::run::{Grants, RunEnd, Source};

fn example(name: &str) -> Source {
    Source::File(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/gui")
            .join(name),
    )
}

fn ui() -> Grants {
    Grants {
        ui: true,
        ..Grants::default()
    }
}

#[test]
fn counter_counts_clicks() {
    let mut app = HeadlessApp::start(example("counter.crush"), ui());
    assert!(app.is_running(), "ended early: {:?}", app.outcome());
    assert_eq!(app.title(), "Counter");
    assert!(app.text().contains("Count: 0"), "{}", app.text());

    app.click("inc").unwrap();
    app.click("inc").unwrap();
    app.click("inc").unwrap();
    assert_eq!(app.text_of("count").as_deref(), Some("Count: 3"));

    app.click("dec").unwrap();
    assert_eq!(app.text_of("count").as_deref(), Some("Count: 2"));

    app.click("reset").unwrap();
    assert_eq!(app.text_of("count").as_deref(), Some("Count: 0"));
    // One render at start plus one per click.
    assert_eq!(app.renders(), 1 + 5);
}

#[test]
fn counter_ends_on_close() {
    let mut app = HeadlessApp::start(example("counter.crush"), ui());
    app.click("inc").unwrap();
    assert!(app.is_running());
    app.close();
    assert_eq!(app.outcome(), Some(&Ok(RunEnd::Finished)));
    assert_eq!(app.output(), "bye, count was 1\n");
    // Nothing more can be injected into a program that has ended.
    assert!(app.click("inc").is_err());
}

#[test]
fn clicking_a_missing_element_is_an_error_not_a_hang() {
    let mut app = HeadlessApp::start(example("counter.crush"), ui());
    let err = app.click("nope").unwrap_err().to_string();
    assert!(err.contains("no element with id \"nope\""), "{err}");
}

#[test]
fn todo_adds_and_removes_items() {
    let mut app = HeadlessApp::start(example("todo.crush"), ui());
    assert!(app.is_running(), "ended early: {:?}", app.outcome());
    assert_eq!(app.text_of("empty").as_deref(), Some("Nothing to do."));

    // Type, then click Add.
    app.type_into("draft", "buy milk").unwrap();
    app.click("add").unwrap();
    assert!(!app.has_id("empty"));
    assert!(app.text_of("items").unwrap().contains("buy milk"));
    assert_eq!(app.text_of("summary").as_deref(), Some("1 item(s)"));
    // The page was redrawn with an empty draft.
    assert_eq!(app.value_of("draft").as_deref(), Some(""));

    // Type, then press Enter in the input: the same as Add.
    app.type_into("draft", "walk dog").unwrap();
    app.press_enter("draft").unwrap();
    app.type_into("draft", "<b>bold</b>").unwrap();
    app.click("add").unwrap();
    assert_eq!(app.text_of("summary").as_deref(), Some("3 item(s)"));
    let items = app.text_of("items").unwrap();
    assert!(
        items.contains("buy milk") && items.contains("walk dog"),
        "{items}"
    );
    // Item text is escaped, not interpreted as markup.
    assert!(items.contains("<b>bold</b>"), "{items}");
    assert!(app.markup().unwrap().contains("&lt;b&gt;bold&lt;/b&gt;"));

    // Remove the middle one.
    app.click("rm-1").unwrap();
    assert_eq!(app.text_of("summary").as_deref(), Some("2 item(s)"));
    let items = app.text_of("items").unwrap();
    assert!(
        items.contains("buy milk") && !items.contains("walk dog"),
        "{items}"
    );

    // Adding an empty draft does nothing (no render).
    let renders = app.renders();
    app.click("add").unwrap();
    assert_eq!(app.renders(), renders);

    app.click("rm-1").unwrap();
    app.click("rm-0").unwrap();
    assert_eq!(app.text_of("empty").as_deref(), Some("Nothing to do."));
}

/// A program that echoes the shape of every event it receives.
const ECHO: &str = r#"
ui.render(ui.button("b", "B", "default") + ui.input("i", "start", "hint"));
while true {
    let ev = ui.next_event();
    print(ev.kind + "|" + ev.id + "|" + ev.value + "|" + ev.key);
    if ev.kind == "close" { break; }
}
"#;

#[test]
fn every_event_has_the_same_four_fields() {
    let mut app = HeadlessApp::start(Source::Text(ECHO.into()), ui());
    app.click("b").unwrap();
    app.type_into("i", "abc").unwrap();
    app.press_enter("i").unwrap();
    app.press_key(arniko::reactive::keyboard_types::Key::Escape)
        .unwrap();
    // Printable keys are not reported.
    app.press_key(arniko::reactive::keyboard_types::Key::Character("x".into()))
        .unwrap();
    app.close();
    assert_eq!(
        app.output(),
        "click|b||\ninput|i|abc|\nsubmit|i|abc|\nkey|||Escape\nclose|||\n"
    );
}

#[test]
fn enter_reports_the_initial_value_when_nothing_was_typed() {
    let mut app = HeadlessApp::start(Source::Text(ECHO.into()), ui());
    app.press_enter("i").unwrap();
    app.close();
    assert!(
        app.output().starts_with("submit|i|start|\n"),
        "{}",
        app.output()
    );
}

#[test]
fn stale_handlers_from_an_earlier_page_stay_quiet() {
    // Page 1 has button "a" (and nothing else); page 2 has "c" in the same
    // DOM slots. Clicking by id always targets the live page.
    let src = r#"
ui.render(ui.button("a", "A", "default"));
let ev = ui.next_event();
print("1:" + ev.id);
ui.render(ui.button("c", "C", "default"));
let ev2 = ui.next_event();
print("2:" + ev2.id);
"#;
    let mut app = HeadlessApp::start(Source::Text(src.into()), ui());
    app.click("a").unwrap();
    assert!(!app.has_id("a"));
    assert!(app.click("a").is_err());
    app.click("c").unwrap();
    assert_eq!(app.output(), "1:a\n2:c\n");
    assert_eq!(app.outcome(), Some(&Ok(RunEnd::Finished)));
}

#[test]
fn render_after_close_is_a_no_op_and_close_repeats() {
    let src = r#"
ui.render("<p id=\"p\">hi</p>");
let ev = ui.next_event();
print(ev.kind);
ui.render("<p>still here</p>");
print(ui.next_event().kind);
print(ui.next_event().kind);
"#;
    let mut app = HeadlessApp::start(Source::Text(src.into()), ui());
    app.close();
    assert_eq!(app.output(), "close\nclose\nclose\n");
    assert_eq!(app.outcome(), Some(&Ok(RunEnd::Finished)));
}

// ---- the grant -------------------------------------------------------------

const NEEDS_UI: &str = r#"
print("must not print");
ui.render("<p>x</p>");
"#;

fn refused(grants: Grants) -> String {
    let app = HeadlessApp::start(Source::Text(NEEDS_UI.into()), grants);
    assert_eq!(app.renders(), 0, "nothing may render");
    assert_eq!(app.output(), "", "nothing may run");
    match app.outcome() {
        Some(Err(message)) => message.clone(),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn refused_without_the_ui_grant_naming_the_flag() {
    let message = refused(Grants::default());
    assert!(message.contains("nothing was run"), "{message}");
    assert!(message.contains("--ui: ui.render"), "{message}");
}

#[test]
fn other_grants_do_not_grant_ui() {
    let everything_but_ui = Grants {
        ui: false,
        fs: true,
        env: true,
        time: true,
        bus: true,
        task: true,
        akg: true,
        process: true,
        crypto: true,
        net: true,
        polyglot: true,
        ..Grants::default()
    };
    let message = refused(everything_but_ui);
    assert!(message.contains("--ui: ui.render"), "{message}");
}

#[test]
fn the_refusal_lists_every_ui_capability_and_other_missing_grants() {
    let src = r#"
let x = ui.next_event();
ui.title("t");
print(time.now());
"#;
    let app = HeadlessApp::start(Source::Text(src.into()), Grants::default());
    let Some(Err(message)) = app.outcome() else {
        panic!("expected a refusal")
    };
    assert!(message.contains("--time: time.now"), "{message}");
    assert!(
        message.contains("--ui: ui.next_event, ui.title"),
        "{message}"
    );
}

#[test]
fn ui_granted_but_nothing_else_still_refuses_fs() {
    let src = r#"
ui.render("<p>x</p>");
print(fs.cat("Cargo.toml"));
"#;
    let app = HeadlessApp::start(Source::Text(src.into()), ui());
    let Some(Err(message)) = app.outcome() else {
        panic!("expected a refusal")
    };
    assert!(message.contains("--fs: fs.cat"), "{message}");
    assert!(!message.contains("--ui"), "ui was granted: {message}");
    assert_eq!(app.renders(), 0);
}

#[test]
fn bad_arguments_are_runtime_errors_not_panics() {
    let app = HeadlessApp::start(Source::Text(r#"ui.button("only-id");"#.into()), ui());
    let Some(Err(message)) = app.outcome() else {
        panic!("expected an error")
    };
    assert!(
        message.contains("ui.button: expected (id, label[, variant])"),
        "{message}"
    );
}
