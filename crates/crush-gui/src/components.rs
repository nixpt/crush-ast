//! `ui.button`, `ui.input`, … — small HTML builders for the page a program
//! passes to `ui.render`.
//!
//! They follow `arniko-crush`'s component mapping (the same arniko component
//! structs and variant names), with one addition that makes them usable
//! for an event loop: the interactive ones take an `id`, which is what
//! `ui.next_event()` reports back. Text is always escaped.

use arniko::{
    Alert, AlertVariant, Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, Input,
    escape_html,
};

/// Escape `text` for use inside markup.
pub fn escape(text: &str) -> String {
    escape_html(text)
}

fn variant<T: Clone>(name: &str, table: &[(&str, T)], default: T) -> T {
    let name = name.trim().to_ascii_lowercase();
    table
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(default, |(_, v)| v.clone())
}

/// Put `id="…"` on the first element of `html`, whose open tag is `<tag`.
fn with_id(html: String, tag: &str, id: &str) -> String {
    let open = format!("<{tag}");
    html.replacen(&open, &format!("{open} id=\"{}\"", escape_html(id)), 1)
}

pub fn button(id: &str, label: &str, variant_name: &str) -> String {
    let v = variant(
        variant_name,
        &[
            ("default", ButtonVariant::Default),
            ("destructive", ButtonVariant::Destructive),
            ("outline", ButtonVariant::Outline),
            ("secondary", ButtonVariant::Secondary),
            ("ghost", ButtonVariant::Ghost),
            ("accent", ButtonVariant::Accent),
        ],
        ButtonVariant::Default,
    );
    with_id(
        Button::new(label)
            .variant(v)
            .size(ButtonSize::Default)
            .render(),
        "button",
        id,
    )
}

pub fn input(id: &str, value: &str, placeholder: &str) -> String {
    with_id(
        Input::new().placeholder(placeholder).value(value).render(),
        "input",
        id,
    )
}

/// A titled card whose body is `body_html` (markup, not escaped — build it
/// from other `ui.*` helpers).
pub fn card(title: &str, body_html: &str) -> String {
    Card::new().title(title).body_html(body_html).render()
}

pub fn alert(message: &str, variant_name: &str) -> String {
    let v = variant(
        variant_name,
        &[
            ("info", AlertVariant::Info),
            ("success", AlertVariant::Success),
            ("warning", AlertVariant::Warning),
            ("error", AlertVariant::Error),
        ],
        AlertVariant::Info,
    );
    Alert::new(message).variant(v).render()
}

pub fn badge(text: &str, variant_name: &str) -> String {
    let v = variant(
        variant_name,
        &[
            ("default", BadgeVariant::Default),
            ("success", BadgeVariant::Success),
            ("warning", BadgeVariant::Warning),
            ("error", BadgeVariant::Error),
            ("info", BadgeVariant::Info),
            ("purple", BadgeVariant::Purple),
        ],
        BadgeVariant::Default,
    );
    Badge::new(text).variant(v).render()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_helpers_carry_the_id() {
        assert!(
            button("inc", "+1", "accent")
                .starts_with(r#"<button id="inc" class="arniko-btn arniko-btn-accent"#)
        );
        assert!(input("new", "x", "What?").starts_with(r#"<input id="new" "#));
    }

    #[test]
    fn text_and_ids_are_escaped() {
        let b = button(r#"a"b"#, "<script>", "nope");
        assert!(b.contains(r#"id="a&quot;b""#), "{b}");
        assert!(b.contains("&lt;script&gt;"), "{b}");
        assert!(
            b.contains("arniko-btn-default"),
            "unknown variant falls back: {b}"
        );
    }
}
