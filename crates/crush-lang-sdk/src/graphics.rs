//! Simple vector-graphics host capabilities for CRUSH capsules.
//!
//! Provides a minimal retained-mode canvas backed by SVG. Canvases are
//! identified by handles and can accumulate rectangles, circles, and text
//! before being serialized to SVG XML via `graphics.to_svg`.
//!
//! All capabilities are gated by the `graphics` cargo feature so the core
//! SDK stays dependency-free for hosts that do not need drawing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};

static CANVAS_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Default, Clone)]
struct Element {
    svg: String,
}

#[derive(Debug, Default)]
struct Canvas {
    width: u32,
    height: u32,
    elements: Vec<Element>,
}

#[derive(Debug, Default)]
pub(crate) struct GraphicsState {
    canvases: HashMap<String, Canvas>,
}

/// Register all graphics capabilities on the given [`HostCaps`] registry.
pub fn register(caps: &mut HostCaps) {
    let state = Arc::new(Mutex::new(GraphicsState::default()));
    caps.register(Box::new(CanvasCreateCap::new(Arc::clone(&state))));
    caps.register(Box::new(RectCap::new(Arc::clone(&state))));
    caps.register(Box::new(CircleCap::new(Arc::clone(&state))));
    caps.register(Box::new(TextCap::new(Arc::clone(&state))));
    caps.register(Box::new(ToSvgCap::new(Arc::clone(&state))));
}

fn next_handle() -> String {
    let seq = CANVAS_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("canvas:{seq}")
}

fn color_text(v: &Value) -> String {
    let s = crate::caps::value_as_text(v);
    if s.is_empty() { "black".to_string() } else { s }
}

fn u32_arg(v: &Value) -> Result<u32, String> {
    match v {
        Value::Int(i) => (*i)
            .try_into()
            .map_err(|_| "expected non-negative integer".to_string()),
        Value::Float(f) => Ok(*f as u32),
        v => crate::caps::value_as_text(v)
            .parse::<u32>()
            .map_err(|e| format!("expected unsigned integer: {e}")),
    }
}

pub struct CanvasCreateCap {
    state: Arc<Mutex<GraphicsState>>,
}

impl CanvasCreateCap {
    pub(crate) fn new(state: Arc<Mutex<GraphicsState>>) -> Self {
        Self { state }
    }
}

impl HostCap for CanvasCreateCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "graphics.canvas".to_string(),
            argc: Some(2),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let width = u32_arg(&args[0])?;
        let height = u32_arg(&args[1])?;
        let handle = next_handle();
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        state.canvases.insert(
            handle.clone(),
            Canvas {
                width,
                height,
                elements: vec![],
            },
        );
        Ok(Some(Value::Str(handle)))
    }
}

pub struct RectCap {
    state: Arc<Mutex<GraphicsState>>,
}

impl RectCap {
    pub(crate) fn new(state: Arc<Mutex<GraphicsState>>) -> Self {
        Self { state }
    }
}

impl HostCap for RectCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "graphics.rect".to_string(),
            argc: Some(6),
            returns: false,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let handle = crate::caps::value_as_text(&args[0]);
        let x = u32_arg(&args[1])?;
        let y = u32_arg(&args[2])?;
        let w = u32_arg(&args[3])?;
        let h = u32_arg(&args[4])?;
        let fill = color_text(&args[5]);
        let svg = format!(
            r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{}"/>"#,
            html_escape(&fill)
        );

        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        let canvas = state
            .canvases
            .get_mut(&handle)
            .ok_or("graphics.rect: canvas not found")?;
        canvas.elements.push(Element { svg });
        Ok(None)
    }
}

pub struct CircleCap {
    state: Arc<Mutex<GraphicsState>>,
}

impl CircleCap {
    pub(crate) fn new(state: Arc<Mutex<GraphicsState>>) -> Self {
        Self { state }
    }
}

impl HostCap for CircleCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "graphics.circle".to_string(),
            argc: Some(5),
            returns: false,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let handle = crate::caps::value_as_text(&args[0]);
        let cx = u32_arg(&args[1])?;
        let cy = u32_arg(&args[2])?;
        let r = u32_arg(&args[3])?;
        let fill = color_text(&args[4]);
        let svg = format!(
            r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{}"/>"#,
            html_escape(&fill)
        );

        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        let canvas = state
            .canvases
            .get_mut(&handle)
            .ok_or("graphics.circle: canvas not found")?;
        canvas.elements.push(Element { svg });
        Ok(None)
    }
}

pub struct TextCap {
    state: Arc<Mutex<GraphicsState>>,
}

impl TextCap {
    pub(crate) fn new(state: Arc<Mutex<GraphicsState>>) -> Self {
        Self { state }
    }
}

impl HostCap for TextCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "graphics.text".to_string(),
            argc: Some(5),
            returns: false,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let handle = crate::caps::value_as_text(&args[0]);
        let x = u32_arg(&args[1])?;
        let y = u32_arg(&args[2])?;
        let content = crate::caps::value_as_text(&args[3]);
        let fill = color_text(&args[4]);
        let svg = format!(
            r#"<text x="{x}" y="{y}" fill="{}" font-family="sans-serif" font-size="12">{}</text>"#,
            html_escape(&fill),
            html_escape(&content),
        );

        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        let canvas = state
            .canvases
            .get_mut(&handle)
            .ok_or("graphics.text: canvas not found")?;
        canvas.elements.push(Element { svg });
        Ok(None)
    }
}

pub struct ToSvgCap {
    state: Arc<Mutex<GraphicsState>>,
}

impl ToSvgCap {
    pub(crate) fn new(state: Arc<Mutex<GraphicsState>>) -> Self {
        Self { state }
    }
}

impl HostCap for ToSvgCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "graphics.to_svg".to_string(),
            argc: Some(1),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let handle = crate::caps::value_as_text(&args[0]);
        let state = self.state.lock().map_err(|e| e.to_string())?;
        let canvas = state
            .canvases
            .get(&handle)
            .ok_or("graphics.to_svg: canvas not found")?;
        let mut svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}">"#,
            canvas.width, canvas.height
        );
        for el in &canvas.elements {
            svg.push_str(&el.svg);
        }
        svg.push_str("</svg>");
        Ok(Some(Value::Str(svg)))
    }
}

fn html_escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&#x27;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_to_svg_roundtrip() {
        let state = Arc::new(Mutex::new(GraphicsState::default()));
        let create = CanvasCreateCap::new(Arc::clone(&state));
        let rect = RectCap::new(Arc::clone(&state));
        let to_svg = ToSvgCap::new(Arc::clone(&state));

        let handle = create
            .call(vec![Value::Int(100), Value::Int(50)])
            .unwrap()
            .unwrap();
        rect.call(vec![
            handle.clone(),
            Value::Int(10),
            Value::Int(20),
            Value::Int(30),
            Value::Int(40),
            Value::Str("#ff0000".into()),
        ])
        .unwrap();

        let svg = to_svg.call(vec![handle]).unwrap().unwrap();
        let text = crate::caps::value_as_text(&svg);
        assert!(text.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(text.contains(r##"<rect x="10" y="20" width="30" height="40" fill="#ff0000"/>"##));
        assert!(text.ends_with("</svg>"));
    }

    #[test]
    fn text_is_escaped() {
        let state = Arc::new(Mutex::new(GraphicsState::default()));
        let create = CanvasCreateCap::new(Arc::clone(&state));
        let text = TextCap::new(Arc::clone(&state));
        let to_svg = ToSvgCap::new(Arc::clone(&state));

        let handle = create
            .call(vec![Value::Int(50), Value::Int(50)])
            .unwrap()
            .unwrap();
        text.call(vec![
            handle.clone(),
            Value::Int(5),
            Value::Int(10),
            Value::Str("a < b & c".into()),
            Value::Str("black".into()),
        ])
        .unwrap();

        let svg = to_svg.call(vec![handle]).unwrap().unwrap();
        assert!(crate::caps::value_as_text(&svg).contains("a &lt; b &amp; c"));
    }

    #[test]
    fn runtime_with_store_load() {
        use crate::{HostCapsBuilder, ProgramBuilder, Runtime};
        let program = ProgramBuilder::new()
            .permission("io.print")
            .permission("graphics.canvas")
            .permission("graphics.rect")
            .permission("graphics.to_svg")
            .line(".func main")
            .line(r#"PUSH 100"#)
            .line(r#"PUSH 50"#)
            .line(r#"CAP_CALL "graphics.canvas" 2"#)
            .line(r#"DUP"#)
            .line(r#"PUSH 10"#)
            .line(r#"PUSH 20"#)
            .line(r#"PUSH 30"#)
            .line(r#"PUSH 40"#)
            .line(r#"PUSH_STR "red""#)
            .line(r#"CAP_CALL "graphics.rect" 6"#)
            .line(r#"CAP_CALL "graphics.to_svg" 1"#)
            .line(r#"CAP_CALL "io.print" 1"#)
            .line("HALT")
            .build()
            .expect("build");

        let host_caps = HostCapsBuilder::new().graphics(true).build();
        let result = Runtime::new()
            .with_host_caps(host_caps)
            .run(&program)
            .expect("run");
        assert!(
            result
                .output
                .starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\"")
        );
    }
}
