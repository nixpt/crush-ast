//! `time.format` / `time.parse` — the pure half of exosphere's `time` family.
//!
//! Ported from exosphere `core/base/stdlib/src/time_cap.rs`. Timestamps are
//! Unix epoch **milliseconds**, UTC, and formats are chrono `strftime`
//! strings, as in nanovm. The clock-reading half (`time.now_ms`,
//! `time.now_iso`, `time.elapsed`, `time.sleep`) observes or spends real time,
//! so it lives with the grant-gated `--time` host capabilities in
//! `host_caps.rs`, next to the existing `time.now`.

use super::{get_int, get_str};
use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};

pub(super) fn register(caps: &mut HostCaps) {
    caps.register(Box::new(TimeFormatCap));
    caps.register(Box::new(TimeParseCap));
}

/// A chrono format string with no invalid specifiers (chrono's `Display`
/// would otherwise panic mid-format).
fn format_items(cap: &str, fmt: &str) -> Result<Vec<chrono::format::Item<'static>>, String> {
    let items: Vec<_> = chrono::format::StrftimeItems::new(fmt)
        .map(|item| item.to_owned())
        .collect();
    if items
        .iter()
        .any(|i| matches!(i, chrono::format::Item::Error))
    {
        return Err(format!("{cap}: invalid format string {fmt:?}"));
    }
    Ok(items)
}

std_cap!(TimeFormatCap, "time.format", Some(2), |args: &[Value]| {
    let ms = get_int(args, 0)?;
    let fmt = get_str(args, 1)?;
    let items = format_items("time.format", &fmt)?;
    let dt = chrono::DateTime::from_timestamp_millis(ms)
        .ok_or_else(|| format!("time.format: timestamp {ms} out of range"))?;
    Ok(Some(Value::Str(
        dt.format_with_items(items.into_iter()).to_string(),
    )))
});

// Parses a *naive* date-time (the format must include a date and a time)
// and interprets it as UTC.
std_cap!(TimeParseCap, "time.parse", Some(2), |args: &[Value]| {
    let s = get_str(args, 0)?;
    let fmt = get_str(args, 1)?;
    let dt = chrono::NaiveDateTime::parse_from_str(&s, &fmt)
        .map_err(|e| format!("time.parse: {s:?} does not match {fmt:?}: {e}"))?;
    Ok(Some(Value::Int(dt.and_utc().timestamp_millis())))
});

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: Vec<Value>) -> Result<Option<Value>, String> {
        let mut caps = HostCaps::new();
        register(&mut caps);
        caps.get(name).expect(name).call(args)
    }

    fn s(v: &str) -> Value {
        Value::Str(v.to_string())
    }

    #[test]
    fn format_and_parse_round_trip_in_milliseconds() {
        let fmt = "%Y-%m-%d %H:%M:%S";
        let ms = call("time.parse", vec![s("2026-09-25 12:34:56"), s(fmt)])
            .unwrap()
            .unwrap();
        assert_eq!(ms, Value::Int(1_790_339_696_000));
        assert_eq!(
            call("time.format", vec![ms, s(fmt)]),
            Ok(Some(s("2026-09-25 12:34:56")))
        );
        assert_eq!(
            call("time.format", vec![Value::Int(0), s("%Y")]),
            Ok(Some(s("1970")))
        );
    }

    #[test]
    fn bad_inputs_are_errors_not_panics() {
        assert!(call("time.parse", vec![s("yesterday"), s("%Y-%m-%d %H:%M")]).is_err());
        let err = call("time.format", vec![Value::Int(0), s("%Q")]).unwrap_err();
        assert!(err.contains("invalid format"), "{err}");
        assert!(call("time.format", vec![Value::Int(i64::MAX), s("%Y")]).is_err());
    }
}
