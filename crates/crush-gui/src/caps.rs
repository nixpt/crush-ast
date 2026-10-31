//! The `ui.*` capabilities.
//!
//! All of them are granted together by `--ui`: a program that names any
//! `ui.*` capability is refused before it runs unless the grant is there.
//! Even the pure markup helpers (`ui.button`, …) are in the namespace on
//! purpose, so "does this program touch the screen" is one grep, and the
//! grant means the same thing on every backend.

use std::sync::Arc;

use crush_lang_sdk::caps::value_as_text;
use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};

use crate::backend::UiBackend;
use crate::components;

/// The grant that unlocks every `ui.*` capability.
pub const GRANT: &str = "--ui";

/// Every capability this module registers, in documentation order.
pub const NAMES: &[&str] = &[
    "ui.render",
    "ui.next_event",
    "ui.title",
    "ui.escape",
    "ui.button",
    "ui.input",
    "ui.card",
    "ui.alert",
    "ui.badge",
];

/// Is `capability` one of the `ui.*` family?
pub fn is_ui_cap(capability: &str) -> bool {
    capability.starts_with("ui.")
}

type Handler = dyn Fn(Vec<Value>) -> Result<Option<Value>, String> + Send + Sync;

struct UiCap {
    name: &'static str,
    argc: Option<usize>,
    returns: bool,
    effects: &'static [&'static str],
    handler: Box<Handler>,
}

impl HostCap for UiCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: self.name.to_string(),
            argc: self.argc,
            returns: self.returns,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        (self.handler)(args)
    }

    fn effects(&self) -> Option<&'static [&'static str]> {
        Some(self.effects)
    }
}

/// Text of argument `i`, or `default` when the call left it out.
fn text(args: &[Value], i: usize, default: &str) -> String {
    args.get(i)
        .map_or_else(|| default.to_string(), value_as_text)
}

fn need(args: &[Value], name: &str, min: usize, max: usize, usage: &str) -> Result<(), String> {
    if (min..=max).contains(&args.len()) {
        Ok(())
    } else {
        Err(format!(
            "{name}: expected {usage}, got {} argument(s)",
            args.len()
        ))
    }
}

fn str_result(s: String) -> Result<Option<Value>, String> {
    Ok(Some(Value::Str(s)))
}

/// Register every `ui.*` capability, talking to `backend`.
pub fn register(caps: &mut HostCaps, backend: Arc<dyn UiBackend>) {
    let mut add = |name: &'static str,
                   argc: Option<usize>,
                   returns: bool,
                   effects: &'static [&'static str],
                   handler: Box<Handler>| {
        caps.register(Box::new(UiCap {
            name,
            argc,
            returns,
            effects,
            handler,
        }));
    };

    let b = backend.clone();
    add(
        "ui.render",
        Some(1),
        false,
        &["ui/render"],
        Box::new(move |args| {
            b.render(&value_as_text(&args[0]))?;
            Ok(None)
        }),
    );

    let b = backend.clone();
    add(
        "ui.next_event",
        Some(0),
        true,
        &["ui/event"],
        Box::new(move |_| Ok(Some(b.next_event().to_value()))),
    );

    let b = backend;
    add(
        "ui.title",
        Some(1),
        false,
        &["ui/render"],
        Box::new(move |args| {
            b.set_title(&value_as_text(&args[0]))?;
            Ok(None)
        }),
    );

    add(
        "ui.escape",
        Some(1),
        true,
        &[],
        Box::new(|args| str_result(components::escape(&value_as_text(&args[0])))),
    );
    add(
        "ui.button",
        None,
        true,
        &[],
        Box::new(|args| {
            need(&args, "ui.button", 2, 3, "(id, label[, variant])")?;
            str_result(components::button(
                &text(&args, 0, ""),
                &text(&args, 1, ""),
                &text(&args, 2, "default"),
            ))
        }),
    );
    add(
        "ui.input",
        None,
        true,
        &[],
        Box::new(|args| {
            need(&args, "ui.input", 1, 3, "(id[, value[, placeholder]])")?;
            str_result(components::input(
                &text(&args, 0, ""),
                &text(&args, 1, ""),
                &text(&args, 2, ""),
            ))
        }),
    );
    add(
        "ui.card",
        None,
        true,
        &[],
        Box::new(|args| {
            need(&args, "ui.card", 2, 2, "(title, body_html)")?;
            str_result(components::card(&text(&args, 0, ""), &text(&args, 1, "")))
        }),
    );
    add(
        "ui.alert",
        None,
        true,
        &[],
        Box::new(|args| {
            need(&args, "ui.alert", 1, 2, "(message[, variant])")?;
            str_result(components::alert(
                &text(&args, 0, ""),
                &text(&args, 1, "info"),
            ))
        }),
    );
    add(
        "ui.badge",
        None,
        true,
        &[],
        Box::new(|args| {
            need(&args, "ui.badge", 1, 2, "(text[, variant])")?;
            str_result(components::badge(
                &text(&args, 0, ""),
                &text(&args, 1, "default"),
            ))
        }),
    );
}
