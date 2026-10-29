//! `aurora chat`: o assistente do Lace no terminal. Uma tela com a conversa,
//! o campo de texto e o que o assistente faz, sobre uma sessão do Claude
//! Code com as ferramentas do `aurora mcp` (`aurora_agent`). O mesmo motor do
//! chat do Lace Studio.
//!
//! O projeto é o da pasta atual (ou do `-C`), passado ao assistente como
//! padrão das ferramentas. Quem autentica é o Claude Code: a conta ou a
//! `ANTHROPIC_API_KEY`. O `session_id` sai no fim, para `--resume`.
//!
//! É parte do recurso experimental `assistant` (`aurora_core::experimental`):
//! `commands.rs` só chega aqui com ele ligado.

use std::io::IsTerminal;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use camino::{Utf8Path, Utf8PathBuf};
use aurora_agent::{AgentEvent, Permission, Session, SessionOptions};
use aurora_core::{CancelToken, Project};
use ratatui::crossterm::event::{
    self, Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use crate::settings::ToolchainArgs;

/// As opções de `aurora chat`.
#[derive(clap::Args, Debug, Clone)]
pub struct ChatArgs {
    /// The model (Claude Code's --model: sonnet, opus, or a full id); default: the account's
    #[arg(long)]
    pub model: Option<String>,
    /// Resume a conversation by the session id printed when a chat ends
    #[arg(long, value_name = "SESSION_ID")]
    pub resume: Option<String>,
    /// Let the assistant edit and create files in the project folder (besides the Lace tools and reading)
    #[arg(long)]
    pub allow_edits: bool,
    /// Also expose the tools that change the Lace installation (update, install)
    #[arg(long)]
    pub allow_update: bool,
    /// The Claude Code executable (or its folder); default: PATH and the installer folders
    #[arg(long, value_name = "PATH")]
    pub claude: Option<Utf8PathBuf>,
}

/// De quanto em quanto tempo a tela olha as teclas e os eventos.
const TICK: Duration = Duration::from_millis(50);
/// Quanto esperar o Claude Code sair ao fechar.
const STOP_WAIT: Duration = Duration::from_secs(3);
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn run(
    project_dir: &Utf8Path,
    toolchain: &ToolchainArgs,
    args: &ChatArgs,
    cancel: &CancelToken,
) -> anyhow::Result<bool> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("aurora chat needs a terminal; for a program, use aurora mcp or aurora serve");
    }
    let claude = aurora_agent::find_claude(args.claude.as_deref()).ok_or_else(|| {
        anyhow::anyhow!(
            "Claude Code was not found: install it (https://claude.com/claude-code), log in, and run \
             aurora chat again, or point to it with --claude <PATH>"
        )
    })?;
    let project = Project::discover(project_dir).ok();
    let cwd = project
        .as_ref()
        .map(|p| p.root().to_owned())
        .unwrap_or_else(|| {
            std::env::current_dir()
                .ok()
                .and_then(|d| Utf8PathBuf::from_path_buf(d).ok())
                .unwrap_or_else(|| Utf8PathBuf::from("."))
        });
    let aurora = std::env::current_exe().context("Could not find the aurora executable")?;
    let aurora = Utf8PathBuf::from_path_buf(dunce::canonicalize(&aurora).unwrap_or(aurora))
        .map_err(|p| anyhow::anyhow!("Path is not UTF-8: {}", p.display()))?;
    let options = SessionOptions {
        claude,
        cwd,
        aurora,
        toolchain: toolchain.toolchain.clone(),
        project: project.as_ref().map(|p| p.spf_path().to_owned()),
        permission: if args.allow_edits {
            Permission::Edits
        } else {
            Permission::AuroraOnly
        },
        allow_update: args.allow_update,
        model: args.model.clone(),
        resume: args.resume.clone(),
        system_prompt: None,
    };
    let (session, events) = Session::start(&options)?;
    let mut app = App::new(
        session,
        events,
        project.as_ref().map(|p| p.name().to_owned()),
        cancel.clone(),
    );
    let mut terminal = ratatui::try_init()?;
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    app.session.stop(STOP_WAIT);
    if let Some(id) = &app.session_id {
        println!("Session {id}; continue it with: aurora chat --resume {id}");
    }
    result.map(|()| true)
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> anyhow::Result<()> {
    loop {
        app.poll();
        terminal.draw(|frame| app.draw(frame))?;
        if app.quit || app.cancel.is_cancelled() {
            return Ok(());
        }
        if event::poll(TICK)?
            && let TermEvent::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle(key);
        }
    }
}

/// Uma entrada da conversa, como aparece na tela.
enum Entry {
    User(String),
    Assistant(String),
    Tool {
        name: String,
        args: String,
        result: Option<(bool, String)>,
    },
    Note(String),
}

/// O estado da tela.
struct App {
    session: Session,
    events: Receiver<AgentEvent>,
    cancel: CancelToken,
    project: Option<String>,
    entries: Vec<Entry>,
    input: String,
    cursor: usize,
    /// A resposta está sendo escrita.
    busy: bool,
    /// A sessão ainda não disse `ready` (que só vem depois da primeira
    /// mensagem).
    starting: bool,
    ended: bool,
    quit: bool,
    model: Option<String>,
    session_id: Option<String>,
    cost: f64,
    /// Quantas linhas acima do fim a transcrição está rolada.
    scroll_up: u16,
    started: Instant,
    streaming: bool,
}

impl App {
    fn new(
        session: Session,
        events: Receiver<AgentEvent>,
        project: Option<String>,
        cancel: CancelToken,
    ) -> App {
        App {
            session,
            events,
            cancel,
            project,
            entries: Vec::new(),
            input: String::new(),
            cursor: 0,
            busy: false,
            starting: true,
            ended: false,
            quit: false,
            model: None,
            session_id: None,
            cost: 0.0,
            scroll_up: 0,
            started: Instant::now(),
            streaming: false,
        }
    }

    /// Recebe o que o Claude Code mandou.
    fn poll(&mut self) {
        loop {
            match self.events.try_recv() {
                Ok(event) => self.event(event),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !self.ended {
                        self.ended = true;
                        self.busy = false;
                        self.starting = false;
                        self.entries
                            .push(Entry::Note("The session ended.".to_owned()));
                    }
                    break;
                }
            }
        }
    }

    fn event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::Ready {
                session_id,
                model,
                aurora_connected,
                ..
            } => {
                self.starting = false;
                self.session_id = Some(session_id);
                self.model = model;
                if aurora_connected == Some(false) {
                    self.entries.push(Entry::Note(
                        "The Lace tools did not connect: the assistant answers without them."
                            .to_owned(),
                    ));
                }
            }
            AgentEvent::Text { text } => {
                self.busy = true;
                match self.entries.last_mut() {
                    Some(Entry::Assistant(current)) if self.streaming => current.push_str(&text),
                    _ => {
                        self.entries.push(Entry::Assistant(text));
                        self.streaming = true;
                    }
                }
            }
            AgentEvent::ToolUse { name, input, .. } => {
                self.streaming = false;
                let name = name
                    .strip_prefix("mcp__aurora__")
                    .map_or(name.clone(), str::to_owned);
                let args = match &input {
                    serde_json::Value::Object(fields) => fields
                        .iter()
                        .filter(|(_, v)| !v.is_null())
                        .map(|(k, v)| format!("{k}={}", compact(v)))
                        .collect::<Vec<_>>()
                        .join(" "),
                    other => compact(other),
                };
                self.entries.push(Entry::Tool {
                    name,
                    args,
                    result: None,
                });
            }
            AgentEvent::ToolResult {
                content, is_error, ..
            } => {
                if let Some(Entry::Tool { result, .. }) = self
                    .entries
                    .iter_mut()
                    .rev()
                    .find(|e| matches!(e, Entry::Tool { result: None, .. }))
                {
                    let first = content
                        .lines()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or("")
                        .to_owned();
                    *result = Some((is_error, first));
                }
            }
            AgentEvent::Done {
                is_error,
                text,
                cost_usd,
                session_id,
                ..
            } => {
                self.busy = false;
                self.streaming = false;
                if let Some(cost) = cost_usd {
                    self.cost += cost;
                }
                if session_id.is_some() {
                    self.session_id = session_id;
                }
                if let Some(text) = text {
                    let shown =
                        matches!(self.entries.last(), Some(Entry::Assistant(t)) if *t == text);
                    if is_error {
                        self.entries.push(Entry::Note(text));
                    } else if !shown && !matches!(self.entries.last(), Some(Entry::Assistant(_))) {
                        self.entries.push(Entry::Assistant(text));
                    }
                }
            }
            AgentEvent::Log { .. } => {}
            AgentEvent::Exited { code } => {
                self.ended = true;
                self.busy = false;
                self.starting = false;
                self.entries.push(Entry::Note(format!(
                    "Claude Code exited{}.",
                    code.map(|c| format!(" with code {c}")).unwrap_or_default()
                )));
            }
            _ => {}
        }
    }

    fn handle(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match (key.code, ctrl) {
            (KeyCode::Char('c'), true) | (KeyCode::Esc, _) if self.busy => {
                let _ = self.session.interrupt();
                self.entries.push(Entry::Note("Interrupted.".to_owned()));
            }
            (KeyCode::Char('c'), true)
            | (KeyCode::Char('d'), true)
            | (KeyCode::Char('q'), true) => self.quit = true,
            (KeyCode::Esc, _) => {}
            (KeyCode::Enter, _) => self.submit(),
            (KeyCode::Backspace, _) => {
                if self.cursor > 0 {
                    let at = prev_boundary(&self.input, self.cursor);
                    self.input.replace_range(at..self.cursor, "");
                    self.cursor = at;
                }
            }
            (KeyCode::Delete, _) => {
                if self.cursor < self.input.len() {
                    let end = next_boundary(&self.input, self.cursor);
                    self.input.replace_range(self.cursor..end, "");
                }
            }
            (KeyCode::Left, _) => self.cursor = prev_boundary(&self.input, self.cursor),
            (KeyCode::Right, _) => self.cursor = next_boundary(&self.input, self.cursor),
            (KeyCode::Home, _) => self.cursor = 0,
            (KeyCode::End, _) => self.cursor = self.input.len(),
            (KeyCode::Char('u'), true) => {
                self.input.clear();
                self.cursor = 0;
            }
            (KeyCode::PageUp, _) => self.scroll_up = self.scroll_up.saturating_add(10),
            (KeyCode::PageDown, _) => self.scroll_up = self.scroll_up.saturating_sub(10),
            (KeyCode::Char(c), false) => {
                self.input.insert(self.cursor, c);
                self.cursor += c.len_utf8();
            }
            _ => {}
        }
    }

    fn submit(&mut self) {
        let text = self.input.trim().to_owned();
        if text.is_empty() || self.busy || self.ended {
            return;
        }
        if text == "/quit" || text == "/exit" {
            self.quit = true;
            return;
        }
        self.input.clear();
        self.cursor = 0;
        self.scroll_up = 0;
        self.entries.push(Entry::User(text.clone()));
        match self.session.send(&text) {
            Ok(()) => self.busy = true,
            Err(error) => self.entries.push(Entry::Note(error.to_string())),
        }
    }

    fn draw(&self, frame: &mut Frame) {
        let [header, body, input, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .areas(frame.area());
        self.draw_header(frame, header);
        self.draw_transcript(frame, body);
        self.draw_input(frame, input);
        self.draw_footer(frame, footer);
    }

    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let mut spans = vec![
            Span::styled(
                " aurora chat ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ];
        match &self.project {
            Some(name) => spans.push(Span::styled(name.clone(), Style::default().fg(Color::Cyan))),
            None => spans.push(Span::styled(
                "no project",
                Style::default().fg(Color::DarkGray),
            )),
        }
        if let Some(model) = &self.model {
            spans.push(Span::styled(
                format!("  {model}"),
                Style::default().fg(Color::DarkGray),
            ));
        }
        if self.cost > 0.0 {
            spans.push(Span::styled(
                format!("  US$ {:.3}", self.cost),
                Style::default().fg(Color::DarkGray),
            ));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn draw_transcript(&self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();
        for entry in &self.entries {
            match entry {
                Entry::User(text) => {
                    for (i, l) in text.lines().enumerate() {
                        let prefix = if i == 0 { "› " } else { "  " };
                        lines.push(Line::from(vec![
                            Span::styled(prefix, Style::default().fg(Color::Cyan)),
                            Span::styled(
                                l.to_owned(),
                                Style::default().add_modifier(Modifier::BOLD),
                            ),
                        ]));
                    }
                    lines.push(Line::raw(""));
                }
                Entry::Assistant(text) => {
                    lines.extend(markdown_lines(text));
                    lines.push(Line::raw(""));
                }
                Entry::Tool { name, args, result } => {
                    let mut spans = vec![
                        Span::styled("⚙ ", Style::default().fg(Color::Yellow)),
                        Span::styled(name.clone(), Style::default().fg(Color::Yellow)),
                        Span::styled(format!(" {args}"), Style::default().fg(Color::DarkGray)),
                    ];
                    match result {
                        None => spans.push(Span::styled(
                            format!(" {}", self.spinner()),
                            Style::default().fg(Color::Yellow),
                        )),
                        Some((false, first)) => spans.push(Span::styled(
                            format!("  ✓ {first}"),
                            Style::default().fg(Color::Green),
                        )),
                        Some((true, first)) => spans.push(Span::styled(
                            format!("  ✗ {first}"),
                            Style::default().fg(Color::Red),
                        )),
                    }
                    lines.push(Line::from(spans));
                }
                Entry::Note(text) => lines.push(Line::styled(
                    text.clone(),
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )),
            }
        }
        if self.starting {
            lines.push(Line::styled(
                format!("{} opening the session...", self.spinner()),
                Style::default().fg(Color::DarkGray),
            ));
        } else if self.busy && !self.streaming {
            lines.push(Line::styled(
                format!("{} thinking...", self.spinner()),
                Style::default().fg(Color::DarkGray),
            ));
        }
        let text = Text::from(lines);
        let inner_width = area.width.saturating_sub(2).max(1) as usize;
        let total = text
            .lines
            .iter()
            .map(|l| (l.width().max(1)).div_ceil(inner_width) as u16)
            .sum::<u16>();
        let visible = area.height.saturating_sub(2);
        let max_scroll = total.saturating_sub(visible);
        let scroll = max_scroll.saturating_sub(self.scroll_up.min(max_scroll));
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray));
        frame.render_widget(
            Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .block(block)
                .scroll((scroll, 0)),
            area,
        );
    }

    fn draw_input(&self, frame: &mut Frame, area: Rect) {
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(if self.busy {
                Color::DarkGray
            } else {
                Color::Cyan
            }));
        let placeholder = if self.ended {
            "the session ended; Ctrl+Q to leave"
        } else if self.busy {
            "answering... Esc interrupts"
        } else {
            "ask about the project; Enter sends"
        };
        let paragraph = if self.input.is_empty() {
            Paragraph::new(Span::styled(
                placeholder,
                Style::default().fg(Color::DarkGray),
            ))
        } else {
            Paragraph::new(self.input.as_str())
        };
        frame.render_widget(paragraph.block(block), area);
        if !self.busy && !self.ended {
            let column = self.input[..self.cursor].chars().count() as u16;
            let width = area.width.saturating_sub(2);
            frame.set_cursor_position((
                area.x + 1 + column.min(width.saturating_sub(1)),
                area.y + 1,
            ));
        }
    }

    fn draw_footer(&self, frame: &mut Frame, area: Rect) {
        let text = if self.busy {
            " Esc: interrupt   PgUp/PgDn: scroll   Ctrl+Q: quit"
        } else {
            " Enter: send   PgUp/PgDn: scroll   Ctrl+U: clear   Ctrl+Q: quit"
        };
        frame.render_widget(
            Paragraph::new(Span::styled(text, Style::default().fg(Color::DarkGray))),
            area,
        );
    }

    fn spinner(&self) -> &'static str {
        let frame = (self.started.elapsed().as_millis() / 80) as usize;
        SPINNER[frame % SPINNER.len()]
    }
}

/// O Markdown de uma resposta em linhas do terminal: blocos de código em
/// outra cor, `código` e **negrito** marcados, o resto como veio.
fn markdown_lines(text: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code = false;
    for raw in text.lines() {
        if raw.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            lines.push(Line::styled(
                format!("  {raw}"),
                Style::default().fg(Color::LightBlue),
            ));
            continue;
        }
        lines.push(Line::from(inline_spans(raw)));
    }
    lines
}

fn inline_spans(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let code = rest.find('`');
        let bold = rest.find("**");
        match (code, bold) {
            (Some(c), b) if b.is_none_or(|b| c <= b) => {
                let Some(end) = rest[c + 1..].find('`') else {
                    break;
                };
                spans.push(Span::raw(rest[..c].to_owned()));
                spans.push(Span::styled(
                    rest[c + 1..c + 1 + end].to_owned(),
                    Style::default().fg(Color::LightBlue),
                ));
                rest = &rest[c + 2 + end..];
            }
            (_, Some(b)) => {
                let Some(end) = rest[b + 2..].find("**") else {
                    break;
                };
                spans.push(Span::raw(rest[..b].to_owned()));
                spans.push(Span::styled(
                    rest[b + 2..b + 2 + end].to_owned(),
                    Style::default().add_modifier(Modifier::BOLD),
                ));
                rest = &rest[b + 4 + end..];
            }
            _ => break,
        }
    }
    spans.push(Span::raw(rest.to_owned()));
    spans
}

fn compact(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => {
            let parts: Vec<&str> = s.split(['/', '\\']).collect();
            if parts.len() > 2 {
                parts[parts.len() - 2..].join("/")
            } else {
                s.clone()
            }
        }
        other => other.to_string(),
    }
}

fn prev_boundary(text: &str, at: usize) -> usize {
    let mut i = at.saturating_sub(1);
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn next_boundary(text: &str, at: usize) -> usize {
    let mut i = (at + 1).min(text.len());
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_marks_code_and_bold() {
        let lines = markdown_lines("Use `aurora sim` for **this**.\n```v\nmodule a;\n```\ndone");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].spans.len(), 5);
        assert_eq!(lines[0].spans[1].content, "aurora sim");
        assert_eq!(lines[0].spans[3].content, "this");
        assert_eq!(lines[1].spans[0].content, "  module a;");
        assert_eq!(lines[2].spans[0].content, "done");
    }

    #[test]
    fn paths_in_tool_arguments_are_shortened() {
        assert_eq!(
            compact(&serde_json::json!("C:/a/b/soma/soma.spf")),
            "soma/soma.spf"
        );
        assert_eq!(compact(&serde_json::json!("x")), "x");
        assert_eq!(compact(&serde_json::json!(["a"])), "[\"a\"]");
    }

    #[test]
    fn cursor_moves_by_char_boundaries() {
        let text = "aé";
        assert_eq!(next_boundary(text, 0), 1);
        assert_eq!(next_boundary(text, 1), 3);
        assert_eq!(prev_boundary(text, 3), 1);
        assert_eq!(prev_boundary(text, 1), 0);
    }
}
