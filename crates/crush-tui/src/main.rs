use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, List, ListItem, ListState},
    Terminal,
};
use std::io;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use serde::{Serialize, Deserialize};
use crush_lang_sdk::repl::{ReplState, ReplConfig, evaluate_silent};
use crush_lang_sdk::MessageFormat;
use crush_vm::Quotas;

enum AppEvent {
    Key(crossterm::event::Event),
    AiChunk(usize, String),
    AiDone(usize),
    PolyglotDone(usize, bool, String),
}

fn highlight_line(text: &str) -> Line {
    let mut spans = Vec::new();
    let mut buf = String::new();
    let mut in_string = false;
    let mut in_comment = false;

    let mut push_buf = |buf: &mut String, spans: &mut Vec<Span<'static>>, style: Style| {
        if !buf.is_empty() {
            let word = buf.clone();
            let final_style = if style == Style::default() {
                match word.as_str() {
                    "let" | "mut" | "fn" | "return" | "if" | "else" | "for" | "in" | "while" | "struct" | "import" => {
                        Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
                    }
                    w if w.starts_with('@') => Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                    "true" | "false" | "null" => Style::default().fg(Color::Yellow),
                    _ => {
                        if word.chars().all(|c| c.is_numeric() || c == '.') && word.chars().any(|c| c.is_numeric()) {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default().fg(Color::White)
                        }
                    }
                }
            } else {
                style
            };
            spans.push(Span::styled(word, final_style));
            buf.clear();
        }
    };

    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if in_comment {
            buf.push(c);
            continue;
        }
        if in_string {
            buf.push(c);
            if c == '"' {
                in_string = false;
                push_buf(&mut buf, &mut spans, Style::default().fg(Color::Green));
            }
            continue;
        }

        if c == '/' && chars.peek() == Some(&'/') {
            push_buf(&mut buf, &mut spans, Style::default());
            in_comment = true;
            buf.push(c);
            buf.push(chars.next().unwrap());
            continue;
        }

        if c == '"' {
            push_buf(&mut buf, &mut spans, Style::default());
            in_string = true;
            buf.push(c);
            continue;
        }

        if c.is_alphanumeric() || c == '_' || c == '@' || c == '.' {
            buf.push(c);
        } else {
            push_buf(&mut buf, &mut spans, Style::default());
            let symbol_style = if c.is_whitespace() {
                Style::default()
            } else {
                Style::default().fg(Color::LightBlue)
            };
            spans.push(Span::styled(c.to_string(), symbol_style));
        }
    }
    
    if in_comment {
        push_buf(&mut buf, &mut spans, Style::default().fg(Color::DarkGray));
    } else if in_string {
        push_buf(&mut buf, &mut spans, Style::default().fg(Color::Green));
    } else {
        push_buf(&mut buf, &mut spans, Style::default());
    }

    Line::from(spans)
}

fn main() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run app loop
    let res = run_app(&mut terminal);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
enum CellStatus {
    Pending,
    Success,
    Error,
    AiSynthesizing,
}

#[derive(Serialize, Deserialize)]
struct Cell {
    id: usize,
    input: String,
    output: Option<String>,
    status: CellStatus,
}

struct AppState {
    input_lines: Vec<String>,
    cursor_x: usize,
    cursor_y: usize,
    cells: Vec<Cell>,
    next_cell_id: usize,
    vm_state: ReplState,
    vm_config: ReplConfig,
    
    // File Manager
    fm_visible: bool,
    fm_path: PathBuf,
    fm_entries: Vec<String>,
    fm_selected: usize,

    help_visible: bool,
    agent_visible: bool,
}

impl AppState {
    fn new() -> Self {
        let mut app = Self {
            input_lines: vec![String::new()],
            cursor_x: 0,
            cursor_y: 0,
            cells: vec![],
            next_cell_id: 1,
            vm_state: ReplState::new(),
            vm_config: ReplConfig {
                quotas: Quotas::default(),
                stdlib: true,
                message_format: MessageFormat::Text,
            },
            fm_visible: false,
            fm_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            fm_entries: vec![],
            fm_selected: 0,
            help_visible: false,
            agent_visible: false,
        };
        app.refresh_fm();
        app
    }

    fn refresh_fm(&mut self) {
        self.fm_entries.clear();
        self.fm_entries.push("..".to_string());
        if let Ok(entries) = fs::read_dir(&self.fm_path) {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        self.fm_entries.push(format!("{}/", name));
                    } else {
                        self.fm_entries.push(name);
                    }
                }
            }
        }
        self.fm_entries.sort();
        if self.fm_selected >= self.fm_entries.len() {
            self.fm_selected = 0;
        }
    }

    fn load_notebook(&mut self) {
        if let Ok(contents) = fs::read_to_string("notebook.crushnb") {
            if let Ok(cells) = serde_json::from_str::<Vec<Cell>>(&contents) {
                self.cells = cells;
                self.next_cell_id = self.cells.last().map(|c| c.id + 1).unwrap_or(1);
                
                self.vm_state = ReplState::new();
                for cell in &self.cells {
                    if cell.status == CellStatus::Success {
                        if !cell.input.trim().starts_with("@") {
                            let _ = evaluate_silent(&cell.input, &mut self.vm_state, &self.vm_config);
                        }
                    }
                }
            }
        }
    }
    
    fn save_notebook(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.cells) {
            let _ = fs::write("notebook.crushnb", json);
        }
    }

    fn insert_char(&mut self, c: char) {
        if self.cursor_y < self.input_lines.len() {
            let line = &mut self.input_lines[self.cursor_y];
            if self.cursor_x <= line.chars().count() {
                let byte_idx = line.char_indices().nth(self.cursor_x).map(|(i, _)| i).unwrap_or(line.len());
                line.insert(byte_idx, c);
                self.cursor_x += 1;
            }
        }
    }

    fn insert_newline(&mut self) {
        if self.cursor_y < self.input_lines.len() {
            let line = &mut self.input_lines[self.cursor_y];
            let byte_idx = line.char_indices().nth(self.cursor_x).map(|(i, _)| i).unwrap_or(line.len());
            let remainder = line.split_off(byte_idx);
            self.input_lines.insert(self.cursor_y + 1, remainder);
            self.cursor_y += 1;
            self.cursor_x = 0;
        }
    }

    fn backspace(&mut self) {
        if self.cursor_x > 0 {
            let line = &mut self.input_lines[self.cursor_y];
            let byte_idx = line.char_indices().nth(self.cursor_x - 1).map(|(i, _)| i).unwrap();
            line.remove(byte_idx);
            self.cursor_x -= 1;
        } else if self.cursor_y > 0 {
            let current_line = self.input_lines.remove(self.cursor_y);
            self.cursor_y -= 1;
            let prev_line = &mut self.input_lines[self.cursor_y];
            self.cursor_x = prev_line.chars().count();
            prev_line.push_str(&current_line);
        }
    }

    fn execute_block(&mut self, tx: &mpsc::Sender<AppEvent>) {
        let block = self.input_lines.join("\n");
        if !block.trim().is_empty() {
            let cell_id = self.next_cell_id;
            
            if block.trim().starts_with("@ai.synthesize") {
                // AI Intercept Mode
                self.cells.push(Cell {
                    id: cell_id,
                    input: block.clone(),
                    output: Some(String::new()),
                    status: CellStatus::AiSynthesizing,
                });
                
                let tx_clone = tx.clone();
                std::thread::spawn(move || {
                    let mock_code = "@python {\n  print(\"Hello, I am the AI synthesized code!\")\n  import json\n  data = {\"status\": \"synthesized\"}\n  print(json.dumps(data))\n}\n";
                    for ch in mock_code.chars() {
                        std::thread::sleep(Duration::from_millis(15));
                        let _ = tx_clone.send(AppEvent::AiChunk(cell_id, ch.to_string()));
                    }
                    let _ = tx_clone.send(AppEvent::AiDone(cell_id));
                });
            } else if block.trim().starts_with("@python") || block.trim().starts_with("@sh") {
                // Polyglot Mode
                let is_python = block.trim().starts_with("@python");
                let code = if is_python {
                    block.trim().strip_prefix("@python").unwrap().trim()
                } else {
                    block.trim().strip_prefix("@sh").unwrap().trim()
                };
                
                let code = if code.starts_with('{') && code.ends_with('}') {
                    code[1..code.len()-1].trim().to_string()
                } else {
                    code.to_string()
                };
                
                self.cells.push(Cell {
                    id: cell_id,
                    input: block.clone(),
                    output: None,
                    status: CellStatus::Pending,
                });
                
                let tx_clone = tx.clone();
                std::thread::spawn(move || {
                    let cmd = if is_python {
                        std::process::Command::new("python3").arg("-c").arg(&code).output()
                    } else {
                        std::process::Command::new("bash").arg("-c").arg(&code).output()
                    };
                    
                    let (success, out_str) = match cmd {
                        Ok(out) => {
                            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
                            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
                            let mut res = stdout;
                            if !stderr.is_empty() {
                                if !res.is_empty() { res.push_str("\n"); }
                                res.push_str(&stderr);
                            }
                            (out.status.success(), res.trim().to_string())
                        }
                        Err(e) => (false, e.to_string()),
                    };
                    let _ = tx_clone.send(AppEvent::PolyglotDone(cell_id, success, out_str));
                });
            } else if block.trim().starts_with("@dot") || block.trim().starts_with("@graph") || block.trim().starts_with("@serve") {
                let cmd_str = block.trim().split_whitespace().next().unwrap();
                let is_dot = cmd_str == "@dot";
                let is_graph = cmd_str == "@graph";
                
                let code = block.trim().strip_prefix(cmd_str).unwrap().trim();
                let code = if code.starts_with('{') && code.ends_with('}') {
                    code[1..code.len()-1].trim().to_string()
                } else {
                    code.to_string()
                };

                self.cells.push(Cell {
                    id: cell_id,
                    input: block.clone(),
                    output: None,
                    status: CellStatus::Pending,
                });
                
                let tx_clone = tx.clone();
                std::thread::spawn(move || {
                    let temp_path = std::env::temp_dir().join(format!("cell_{}.crush", cell_id));
                    let _ = std::fs::write(&temp_path, &code);
                    
                    let arg = if is_dot { "dot" } else if is_graph { "graph" } else { "serve" };
                    let cmd = std::process::Command::new("/build/debug/crush-visuals")
                        .arg(arg)
                        .arg(&temp_path)
                        .output();
                    
                    let (success, out_str) = match cmd {
                        Ok(out) => {
                            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
                            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
                            let mut res = stdout;
                            if !stderr.is_empty() {
                                if !res.is_empty() { res.push_str("\n"); }
                                res.push_str(&stderr);
                            }
                            (out.status.success(), res.trim().to_string())
                        }
                        Err(e) => (false, format!("Failed to execute /build/debug/crush-visuals: {}", e)),
                    };
                    let _ = std::fs::remove_file(temp_path);
                    let _ = tx_clone.send(AppEvent::PolyglotDone(cell_id, success, out_str));
                });
            } else {
                // Normal Native Mode
                let result = evaluate_silent(&block, &mut self.vm_state, &self.vm_config);
                let status = if result.error.is_some() { CellStatus::Error } else { CellStatus::Success };
                let output = if result.error.is_some() { result.error } else { result.output };

                self.cells.push(Cell {
                    id: cell_id,
                    input: block,
                    output,
                    status,
                });
            }
            self.next_cell_id += 1;
        }
        self.input_lines = vec![String::new()];
        self.cursor_x = 0;
        self.cursor_y = 0;
    }
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>) -> Result<()> {
    let mut state = AppState::new();
    let (tx, rx) = mpsc::channel();

    loop {
        terminal.draw(|f| {
            let mut main_area = f.area();
            
            // Render FM if visible
            if state.fm_visible {
                let h_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
                    .split(main_area);
                
                main_area = h_chunks[1];
                
                let fm_title = format!(" File Manager ({}) ", state.fm_path.display());
                let mut list_items = Vec::new();
                for (i, entry) in state.fm_entries.iter().enumerate() {
                    let mut style = Style::default().fg(Color::White);
                    if i == state.fm_selected {
                        style = style.bg(Color::Blue).fg(Color::Black);
                    }
                    if entry.ends_with('/') {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    list_items.push(ListItem::new(entry.clone()).style(style));
                }
                
                let list = List::new(list_items)
                    .block(Block::default().borders(Borders::ALL).title(fm_title));
                f.render_widget(list, h_chunks[0]);
            }

            // Render Help if visible
            if state.help_visible {
                let h_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                    .split(main_area);
                main_area = h_chunks[1];
                let help_text = vec![
                    Line::from(" Ctrl+E : Execute code block "),
                    Line::from(" Ctrl+S : Save notebook "),
                    Line::from(" Ctrl+O : Open notebook "),
                    Line::from(" Ctrl+F : Toggle File Manager "),
                    Line::from(" Ctrl+A : Toggle Agent Chat "),
                    Line::from(" Ctrl+H : Toggle Help Menu "),
                    Line::from(" ESC    : Close modals / Exit app "),
                    Line::from(""),
                    Line::from(" Supported Cell Commands: "),
                    Line::from(" @python : Run Python code "),
                    Line::from(" @sh     : Run Shell code "),
                    Line::from(" @dot    : Render AST to DOT "),
                    Line::from(" @graph  : Render AST to Graph JSON "),
                    Line::from(" @serve  : Render AST to Interactive HTML "),
                    Line::from(" @agent  : Send prompt to Agent "),
                ];
                let help_widget = Paragraph::new(help_text)
                    .block(Block::default().borders(Borders::ALL).title(" Help Menu "));
                f.render_widget(help_widget, h_chunks[0]);
            }

            // Render Agent if visible
            if state.agent_visible {
                let h_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
                    .split(main_area);
                main_area = h_chunks[0];
                let agent_text = vec![
                    Line::from(" [Agent Mode] "),
                    Line::from(" Agent integration is coming soon. "),
                    Line::from(" You can also use the `@agent` command "),
                    Line::from(" within a cell to ask questions! "),
                ];
                let agent_widget = Paragraph::new(agent_text)
                    .block(Block::default().borders(Borders::ALL).title(" AI Agent "));
                f.render_widget(agent_widget, h_chunks[1]);
            }

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(0),
                    Constraint::Length(8),
                ])
                .split(main_area);

            let header = Paragraph::new(" Crush Smart REPL (crush-tui) v0.1 ")
                .style(Style::default().fg(Color::Magenta))
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(header, chunks[0]);

            let mut history_lines = Vec::new();
            if state.cells.is_empty() {
                history_lines.push(Line::from(Span::styled("Welcome to the Crush Polyglot Workspace.", Style::default().fg(Color::DarkGray))));
                history_lines.push(Line::from(Span::styled("Hit [ESC] to exit, [Ctrl+E] to execute block.", Style::default().fg(Color::DarkGray))));
                history_lines.push(Line::from(Span::styled("Hit [Ctrl+S] to save, [Ctrl+O] to open notebook.crushnb.", Style::default().fg(Color::DarkGray))));
            } else {
                for cell in &state.cells {
                    // Top border with Cell ID
                    let top_border = format!("╭── [Cell {}] ──────────────────────────", cell.id);
                    history_lines.push(Line::from(Span::styled(top_border, Style::default().fg(Color::Blue))));
                    
                    // Input source code
                    for line in cell.input.lines() {
                        let mut hl = highlight_line(line);
                        hl.spans.insert(0, Span::styled("│ ", Style::default().fg(Color::Blue)));
                        history_lines.push(hl);
                    }
                    
                    // Bottom border
                    history_lines.push(Line::from(Span::styled("╰───────────────────────────────────────", Style::default().fg(Color::Blue))));

                    match cell.status {
                        CellStatus::Pending => {
                            history_lines.push(Line::from(Span::styled("  => [Executing Polyglot Subprocess...]", Style::default().fg(Color::Yellow).add_modifier(Modifier::RAPID_BLINK))));
                        }
                        CellStatus::AiSynthesizing => {
                            if let Some(out) = &cell.output {
                                for out_line in out.lines() {
                                    history_lines.push(Line::from(Span::styled(format!("  {}", out_line), Style::default().fg(Color::LightMagenta))));
                                }
                            }
                            history_lines.push(Line::from(Span::styled("  => [AI Streaming...]", Style::default().fg(Color::LightMagenta).add_modifier(Modifier::RAPID_BLINK))));
                        }
                        CellStatus::Success => {
                            if let Some(out) = &cell.output {
                                for out_line in out.lines() {
                                    history_lines.push(Line::from(Span::raw(format!("  {}", out_line))));
                                }
                            }
                        }
                        CellStatus::Error => {
                            if let Some(err) = &cell.output {
                                for err_line in err.lines() {
                                    history_lines.push(Line::from(Span::styled(format!("  {}", err_line), Style::default().fg(Color::Red))));
                                }
                            }
                        }
                    }
                    history_lines.push(Line::from("")); 
                }
            }

            let history_text = Text::from(history_lines);
            let history = Paragraph::new(history_text)
                .block(Block::default().borders(Borders::ALL).title(" Execution History "));
            f.render_widget(history, chunks[1]);

            let mut editor_lines = Vec::new();
            for line in &state.input_lines {
                editor_lines.push(highlight_line(line));
            }
            let editor_text = Text::from(editor_lines);
            
            let editor = Paragraph::new(editor_text)
                .style(Style::default().fg(Color::Cyan))
                .block(Block::default().borders(Borders::ALL).title(" Editor ([Ctrl+E] Execute) "));
            f.render_widget(editor, chunks[2]);

            // Set cursor position manually based on state
            let cursor_offset_x = chunks[2].x + 1 + state.cursor_x as u16;
            let cursor_offset_y = chunks[2].y + 1 + state.cursor_y as u16;
            f.set_cursor_position((cursor_offset_x, cursor_offset_y));
        })?;

        // Poll for inputs or background events
        if crossterm::event::poll(Duration::from_millis(50))? {
            let _ = tx.send(AppEvent::Key(event::read()?));
        }

        while let Ok(app_event) = rx.try_recv() {
            match app_event {
                AppEvent::AiChunk(cell_id, chunk) => {
                    if let Some(cell) = state.cells.iter_mut().find(|c| c.id == cell_id) {
                        if let Some(out) = &mut cell.output {
                            out.push_str(&chunk);
                        }
                    }
                }
                AppEvent::AiDone(cell_id) => {
                    if let Some(cell) = state.cells.iter_mut().find(|c| c.id == cell_id) {
                        cell.status = CellStatus::Success;
                    }
                }
                AppEvent::PolyglotDone(cell_id, success, out_str) => {
                    if let Some(cell) = state.cells.iter_mut().find(|c| c.id == cell_id) {
                        cell.status = if success { CellStatus::Success } else { CellStatus::Error };
                        cell.output = if out_str.is_empty() { None } else { Some(out_str) };
                    }
                }
                AppEvent::Key(Event::Key(key)) => {
                    if key.kind == KeyEventKind::Press {
                        use crossterm::event::KeyModifiers;
                        match key.code {
                            KeyCode::Esc => {
                                if state.help_visible || state.fm_visible || state.agent_visible {
                                    state.help_visible = false;
                                    state.fm_visible = false;
                                    state.agent_visible = false;
                                } else {
                                    return Ok(());
                                }
                            }
                            KeyCode::Char('s') | KeyCode::Char('S') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                state.save_notebook();
                            }
                            KeyCode::Char('o') | KeyCode::Char('O') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                state.load_notebook();
                            }
                            KeyCode::Char('f') | KeyCode::Char('F') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                state.fm_visible = !state.fm_visible;
                                if state.fm_visible { state.refresh_fm(); }
                            }
                            KeyCode::Char('h') | KeyCode::Char('H') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                state.help_visible = !state.help_visible;
                            }
                            KeyCode::Char('a') | KeyCode::Char('A') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                state.agent_visible = !state.agent_visible;
                            }
                            KeyCode::Char('e') | KeyCode::Char('E') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                state.execute_block(&tx);
                            }
                            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(()),
                            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                                if !state.fm_visible && !state.help_visible && !state.agent_visible { state.insert_char(c); }
                            }
                            KeyCode::Enter => {
                                if state.fm_visible {
                                    let entry = &state.fm_entries[state.fm_selected];
                                    if entry == ".." {
                                        if let Some(parent) = state.fm_path.parent() {
                                            state.fm_path = parent.to_path_buf();
                                            state.refresh_fm();
                                        }
                                    } else if entry.ends_with('/') {
                                        state.fm_path.push(entry.trim_end_matches('/'));
                                        state.refresh_fm();
                                    } else {
                                        let file_path = state.fm_path.join(entry);
                                        if let Ok(contents) = fs::read_to_string(file_path) {
                                            state.input_lines = contents.lines().map(String::from).collect();
                                            if state.input_lines.is_empty() { state.input_lines.push(String::new()); }
                                            state.cursor_y = state.input_lines.len() - 1;
                                            state.cursor_x = state.input_lines[state.cursor_y].len();
                                            state.fm_visible = false;
                                        }
                                    }
                                } else {
                                    state.insert_newline();
                                }
                            }
                            KeyCode::Backspace => { if !state.fm_visible { state.backspace(); } }
                            KeyCode::Left => {
                                if !state.fm_visible {
                                    if state.cursor_x > 0 { state.cursor_x -= 1; }
                                    else if state.cursor_y > 0 {
                                        state.cursor_y -= 1;
                                        state.cursor_x = state.input_lines[state.cursor_y].chars().count();
                                    }
                                }
                            }
                            KeyCode::Right => {
                                if !state.fm_visible {
                                    let len = state.input_lines[state.cursor_y].chars().count();
                                    if state.cursor_x < len { state.cursor_x += 1; }
                                    else if state.cursor_y < state.input_lines.len() - 1 {
                                        state.cursor_y += 1;
                                        state.cursor_x = 0;
                                    }
                                }
                            }
                            KeyCode::Up => {
                                if state.fm_visible {
                                    if state.fm_selected > 0 { state.fm_selected -= 1; }
                                } else {
                                    if state.cursor_y > 0 {
                                        state.cursor_y -= 1;
                                        let len = state.input_lines[state.cursor_y].chars().count();
                                        if state.cursor_x > len { state.cursor_x = len; }
                                    }
                                }
                            }
                            KeyCode::Down => {
                                if state.fm_visible {
                                    if state.fm_selected < state.fm_entries.len().saturating_sub(1) {
                                        state.fm_selected += 1;
                                    }
                                } else {
                                    if state.cursor_y < state.input_lines.len() - 1 {
                                        state.cursor_y += 1;
                                        let len = state.input_lines[state.cursor_y].chars().count();
                                        if state.cursor_x > len { state.cursor_x = len; }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

