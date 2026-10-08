//! O modo watch do `lace learn` (sem subcomando): uma tela de terminal no
//! estilo do rustlings, com o progresso, o exercício, o enunciado com cores,
//! as dicas e a correção.
//!
//! A correção só roda quando o aluno grava o arquivo do exercício (a data de
//! modificação muda) ou aperta `r`. Abrir, trocar de exercício e restaurar
//! não corrigem: o resultado aparece quando ele pede. A correção roda numa
//! thread, com a tela respondendo e uma animação enquanto ela dura.
//!
//! Os textos da tela são em inglês, como o resto da CLI; os enunciados e as
//! dicas vêm da trilha, em português.

use std::cell::Cell;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant, SystemTime};

use lace_core::{CancelToken, Control, Severity, Toolchain};
use lace_learn::{Exercise, Grade, Verdict, Workspace};
use ratatui::crossterm::event::{
    self, Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Clear, Gauge, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use crate::learn::{finding_text, open_wave};

/// De quanto em quanto tempo a tela olha as teclas, o arquivo e a correção.
const TICK: Duration = Duration::from_millis(100);
/// Depois de ver o arquivo mudar, quanto esperar o editor terminar de
/// gravar antes de corrigir.
const SETTLE: Duration = Duration::from_millis(150);
/// Quanto tempo um aviso fica no rodapé.
const NOTE_FOR: Duration = Duration::from_secs(5);
/// A animação enquanto corrige.
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
/// A partir desta largura, o enunciado e o resultado ficam lado a lado.
const SIDE_BY_SIDE: u16 = 110;

/// Roda a tela até o aluno sair.
pub fn run(toolchain: Toolchain, workspace: Workspace, cancel: CancelToken) -> anyhow::Result<()> {
    let mut app = App::new(Some(toolchain), workspace, cancel);
    let mut terminal = ratatui::try_init()?;
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    app.stop();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> anyhow::Result<()> {
    loop {
        app.poll();
        terminal.draw(|frame| app.draw(frame))?;
        if app.quit {
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

// ------------------------------------------------------------ cores

/// O terminal mostra cores de 24 bits? No Windows, sim: o console e o
/// Windows Terminal as mostram desde o Windows 10. Nos outros, só quando a
/// `COLORTERM` diz (o terminal do VS Code, o iTerm2, o GNOME Terminal e o
/// kitty a definem); sem ela, a tela usa a paleta de 256 cores.
fn truecolor() -> bool {
    cfg!(windows) || std::env::var("COLORTERM").is_ok_and(|v| v == "truecolor" || v == "24bit")
}

/// A cor mais perto de `(r, g, b)` entre as 256 do xterm: o cubo 6x6x6 (16 a
/// 231) ou a escala de cinza (232 a 255).
fn xterm256(r: u8, g: u8, b: u8) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let level = |v: u8| {
        (0..LEVELS.len())
            .min_by_key(|&i| LEVELS[i].abs_diff(v))
            .unwrap_or(0)
    };
    let distance = |(x, y, z): (u8, u8, u8)| {
        [(x, r), (y, g), (z, b)]
            .iter()
            .map(|&(a, c)| u32::from(a.abs_diff(c)).pow(2))
            .sum::<u32>()
    };
    let (ri, gi, bi) = (level(r), level(g), level(b));
    let cube = (LEVELS[ri], LEVELS[gi], LEVELS[bi]);
    let mean = (u32::from(r) + u32::from(g) + u32::from(b)) / 3;
    let step = (mean.saturating_sub(3) / 10).min(23) as u8;
    let gray = 8 + 10 * step;
    if distance(cube) <= distance((gray, gray, gray)) {
        16 + 36 * ri as u8 + 6 * gi as u8 + bi as u8
    } else {
        232 + step
    }
}

/// Como o terminal mostra as cores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Colors {
    /// 24 bits.
    Rgb,
    /// As 256 do xterm.
    Indexed,
    /// Nenhuma, com `NO_COLOR`.
    None,
}

/// `color` como o terminal a mostra.
fn fit_to(colors: Colors, color: Color) -> Color {
    match (colors, color) {
        (Colors::None, _) => Color::Reset,
        (Colors::Indexed, Color::Rgb(r, g, b)) => Color::Indexed(xterm256(r, g, b)),
        _ => color,
    }
}

/// As cores da tela. Com `NO_COLOR`, nenhuma: ficam o negrito e o inverso.
#[derive(Debug, Clone, Copy)]
struct Palette {
    /// O azul do ATLAS, o destaque do Lace.
    accent: Color,
    ok: Color,
    error: Color,
    warn: Color,
    dim: Color,
    code: Color,
    keyword: Color,
    number: Color,
    /// O texto sobre os selos coloridos.
    on_badge: Color,
}

impl Palette {
    fn new() -> Self {
        Palette::with(if std::env::var_os("NO_COLOR").is_some() {
            Colors::None
        } else if truecolor() {
            Colors::Rgb
        } else {
            Colors::Indexed
        })
    }

    fn with(colors: Colors) -> Self {
        let fit = |color| fit_to(colors, color);
        Palette {
            accent: fit(Color::Rgb(11, 128, 195)),
            ok: fit(Color::Rgb(63, 185, 80)),
            error: fit(Color::Rgb(248, 81, 73)),
            warn: fit(Color::Rgb(210, 153, 34)),
            dim: fit(Color::Rgb(125, 133, 144)),
            code: fit(Color::Rgb(156, 220, 254)),
            keyword: fit(Color::Rgb(86, 156, 214)),
            number: fit(Color::Rgb(181, 206, 168)),
            on_badge: fit(Color::Rgb(13, 17, 23)),
        }
    }

    fn plain(&self) -> Style {
        Style::default()
    }

    fn dim(&self) -> Style {
        Style::default().fg(self.dim)
    }

    fn bold(&self) -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    fn accent(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// Um selo: texto escuro sobre a cor, ou inverso sem cores.
    fn badge(&self, color: Color) -> Style {
        if color == Color::Reset {
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::default()
                .fg(self.on_badge)
                .bg(color)
                .add_modifier(Modifier::BOLD)
        }
    }
}

// ------------------------------------------------------------ estado

/// A correção do exercício na tela.
enum Check {
    /// Ainda não corrigido desde que o exercício apareceu.
    Waiting,
    /// Corrigindo numa thread.
    Running {
        rx: Receiver<Result<Grade, String>>,
        cancel: CancelToken,
        started: Instant,
    },
    Done(Grade),
    /// Nem deu para corrigir (ferramenta que falta, projeto que não abre).
    Failed(String),
}

/// O que está por cima da tela.
enum Overlay {
    None,
    /// A lista dos exercícios, com o selecionado.
    List {
        selected: usize,
    },
    /// Restaurar o arquivo de um exercício, depois de confirmar.
    ConfirmReset {
        exercise: String,
    },
    /// Corrigir todos.
    CheckAll(CheckAll),
    Help,
}

struct CheckAll {
    rx: Receiver<AllMessage>,
    cancel: CancelToken,
    total: usize,
    /// O nome e o veredito de cada um já corrigido.
    done: Vec<(String, Result<Verdict, String>)>,
    finished: bool,
}

enum AllMessage {
    Graded(String, Box<Result<Grade, String>>),
    Finished,
}

struct Note {
    text: String,
    style: Style,
    until: Instant,
}

/// A tela do modo watch.
pub struct App {
    /// `None` só nos testes da tela: corrigir vira aviso.
    toolchain: Option<Toolchain>,
    workspace: Workspace,
    /// O Ctrl+C e o SIGTERM da CLI.
    cancel: CancelToken,
    exercise: Exercise,
    hints_shown: usize,
    scroll: u16,
    /// O máximo de rolagem do enunciado no último desenho.
    max_scroll: Cell<u16>,
    /// A altura visível do enunciado no último desenho.
    page: Cell<u16>,
    check: Check,
    /// Gravar de novo durante uma correção pede outra depois dela.
    recheck: bool,
    /// As datas dos arquivos vigiados que a tela já viu.
    stamps: Vec<Option<SystemTime>>,
    /// Uma mudança vista e ainda não corrigida (espera o editor terminar).
    changed_at: Option<Instant>,
    overlay: Overlay,
    note: Option<Note>,
    tick: usize,
    quit: bool,
    pal: Palette,
}

impl App {
    fn new(toolchain: Option<Toolchain>, workspace: Workspace, cancel: CancelToken) -> Self {
        let pal = Palette::new();
        let exercise = workspace.current().clone();
        let mut app = App {
            toolchain,
            exercise,
            workspace,
            cancel,
            hints_shown: 0,
            scroll: 0,
            max_scroll: Cell::new(0),
            page: Cell::new(10),
            check: Check::Waiting,
            recheck: false,
            stamps: Vec::new(),
            changed_at: None,
            overlay: Overlay::None,
            note: None,
            tick: 0,
            quit: false,
            pal,
        };
        app.load_exercise();
        app
    }

    /// Mostra o exercício atual da pasta, sem corrigir.
    fn load_exercise(&mut self) {
        self.exercise = self.workspace.current().clone();
        self.hints_shown = 0;
        self.scroll = 0;
        self.check = Check::Waiting;
        self.recheck = false;
        self.stamps = self.current_stamps();
        self.changed_at = None;
    }

    fn current_stamps(&self) -> Vec<Option<SystemTime>> {
        self.workspace
            .watched_files(&self.exercise)
            .iter()
            .map(|f| std::fs::metadata(f).and_then(|m| m.modified()).ok())
            .collect()
    }

    fn say(&mut self, text: impl Into<String>, style: Style) {
        self.note = Some(Note {
            text: text.into(),
            style,
            until: Instant::now() + NOTE_FOR,
        });
    }

    /// Para o que roda numa thread (ao sair).
    fn stop(&mut self) {
        if let Check::Running { cancel, .. } = &self.check {
            cancel.cancel();
        }
        if let Overlay::CheckAll(all) = &self.overlay {
            all.cancel.cancel();
        }
    }

    // -------------------------------------------------------- a cada volta

    fn poll(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        if self.cancel.is_cancelled() {
            self.quit = true;
        }
        if self
            .note
            .as_ref()
            .is_some_and(|n| Instant::now() >= n.until)
        {
            self.note = None;
        }
        self.poll_check();
        self.poll_check_all();
        self.poll_files();
    }

    fn poll_check(&mut self) {
        let Check::Running { rx, .. } = &self.check else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("the check stopped unexpectedly".into()),
        };
        self.check = match result {
            Ok(grade) => {
                if let Err(e) = self.workspace.record(&grade) {
                    self.say(format!("Could not record the result: {e}"), self.pal.dim());
                }
                Check::Done(grade)
            }
            Err(error) => Check::Failed(error),
        };
        if std::mem::take(&mut self.recheck) {
            self.start_check();
        }
    }

    fn poll_check_all(&mut self) {
        let Overlay::CheckAll(all) = &mut self.overlay else {
            return;
        };
        let mut graded = Vec::new();
        loop {
            match all.rx.try_recv() {
                Ok(AllMessage::Graded(name, result)) => graded.push((name, *result)),
                Ok(AllMessage::Finished) | Err(TryRecvError::Disconnected) => {
                    all.finished = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        for (name, result) in graded {
            let verdict = result.as_ref().map(|g| g.verdict).map_err(Clone::clone);
            if let Ok(grade) = &result {
                let _ = self.workspace.record(grade);
                // O atual foi corrigido a pedido: o resultado vale na tela.
                if name == self.exercise.name {
                    self.check = Check::Done(grade.clone());
                }
            }
            if let Overlay::CheckAll(all) = &mut self.overlay {
                all.done.push((name, verdict));
            }
        }
    }

    /// Corrige quando o arquivo muda e para de mudar.
    fn poll_files(&mut self) {
        if matches!(self.overlay, Overlay::CheckAll(_)) {
            return;
        }
        let stamps = self.current_stamps();
        if stamps != self.stamps {
            self.stamps = stamps;
            self.changed_at = Some(Instant::now());
            return;
        }
        if self.changed_at.is_some_and(|at| at.elapsed() >= SETTLE) {
            self.changed_at = None;
            if matches!(self.check, Check::Running { .. }) {
                self.recheck = true;
            } else {
                self.start_check();
            }
        }
    }

    fn start_check(&mut self) {
        if matches!(self.check, Check::Running { .. }) {
            self.recheck = true;
            return;
        }
        let Some(toolchain) = self.toolchain.clone() else {
            self.check = Check::Failed("no toolchain to check with".into());
            return;
        };
        let (tx, rx) = mpsc::channel();
        let cancel = CancelToken::new();
        let control = Control::new().with_cancel(cancel.clone());
        let workspace = self.workspace.clone();
        let exercise = self.exercise.clone();
        std::thread::spawn(move || {
            let result = lace_learn::grade(&toolchain, &workspace, &exercise, &control)
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
        self.check = Check::Running {
            rx,
            cancel,
            started: Instant::now(),
        };
    }

    fn start_check_all(&mut self) {
        let Some(toolchain) = self.toolchain.clone() else {
            self.say(
                "No toolchain to check with.",
                Style::default().fg(self.pal.error),
            );
            return;
        };
        if let Check::Running { cancel, .. } = &self.check {
            cancel.cancel();
            self.check = Check::Waiting;
        }
        let exercises: Vec<Exercise> = self.workspace.track().exercises().cloned().collect();
        let total = exercises.len();
        let (tx, rx) = mpsc::channel();
        let cancel = CancelToken::new();
        let control = Control::new().with_cancel(cancel.clone());
        let workspace = self.workspace.clone();
        std::thread::spawn(move || {
            for exercise in exercises {
                if control.is_cancelled() {
                    break;
                }
                let result = lace_learn::grade(&toolchain, &workspace, &exercise, &control)
                    .map_err(|e| e.to_string());
                if tx
                    .send(AllMessage::Graded(exercise.name.clone(), Box::new(result)))
                    .is_err()
                {
                    return;
                }
            }
            let _ = tx.send(AllMessage::Finished);
        });
        self.overlay = Overlay::CheckAll(CheckAll {
            rx,
            cancel,
            total,
            done: Vec::new(),
            finished: false,
        });
    }

    /// O nome do exercício na posição `index` da trilha.
    fn exercise_at(&self, index: usize) -> Option<String> {
        self.workspace
            .track()
            .exercises()
            .nth(index)
            .map(|e| e.name.clone())
    }

    /// Torna atual outro exercício e o mostra, sem corrigir.
    fn go_to(&mut self, name: &str) {
        if let Check::Running { cancel, .. } = &self.check {
            cancel.cancel();
        }
        match self.workspace.set_current(name) {
            Ok(()) => self.load_exercise(),
            Err(e) => self.say(e.to_string(), Style::default().fg(self.pal.error)),
        }
    }

    fn reset(&mut self, name: &str) {
        let Ok(exercise) = self.workspace.track().require(name).cloned() else {
            return;
        };
        match self.workspace.reset(&exercise) {
            Ok(()) => {
                if exercise.name == self.exercise.name {
                    // Restaurar não corrige: o arquivo novo é o começo.
                    self.load_exercise();
                }
                let file = format!("{}.v", exercise.module);
                self.say(
                    format!("{file} is back to its starting point."),
                    Style::default().fg(self.pal.ok),
                );
            }
            Err(e) => self.say(e.to_string(), Style::default().fg(self.pal.error)),
        }
    }

    // -------------------------------------------------------- teclas

    fn handle(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return;
        }
        match &self.overlay {
            Overlay::None => self.handle_main(key),
            Overlay::List { .. } => self.handle_list(key),
            Overlay::ConfirmReset { .. } => self.handle_confirm(key),
            Overlay::CheckAll(_) => self.handle_check_all(key),
            Overlay::Help => self.overlay = Overlay::None,
        }
    }

    fn handle_main(&mut self, key: KeyEvent) {
        // A rolagem parte do que está na tela.
        self.scroll = self.scroll.min(self.max_scroll.get());
        let page = self.page.get().saturating_sub(1).max(1);
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('r') => self.start_check(),
            KeyCode::Char('h') => {
                if self.hints_shown < self.exercise.hints.len() {
                    self.hints_shown += 1;
                    // A dica nova aparece no fim do enunciado.
                    self.scroll = u16::MAX;
                } else {
                    self.say("No more hints for this exercise.", self.pal.dim());
                }
            }
            KeyCode::Char('n') => {
                if !self.workspace.is_solved(&self.exercise.name) {
                    self.say(
                        "Solve this exercise first, or choose another one with l.",
                        Style::default().fg(self.pal.warn),
                    );
                } else if let Some(next) = self.workspace.next_pending().map(|e| e.name.clone()) {
                    self.go_to(&next);
                } else {
                    let farewell = self
                        .workspace
                        .track()
                        .farewell
                        .clone()
                        .unwrap_or_else(|| "Every exercise is solved.".into());
                    self.say(farewell, Style::default().fg(self.pal.ok));
                }
            }
            KeyCode::Char('l') => {
                let selected = self
                    .workspace
                    .track()
                    .position(&self.exercise.name)
                    .unwrap_or(0);
                self.overlay = Overlay::List { selected };
            }
            KeyCode::Char('w') => self.open_wave(),
            KeyCode::Char('c') => self.start_check_all(),
            KeyCode::Char('x') => {
                self.overlay = Overlay::ConfirmReset {
                    exercise: self.exercise.name.clone(),
                }
            }
            KeyCode::Char('?') => self.overlay = Overlay::Help,
            KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll = self.scroll.saturating_add(1),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(page),
            KeyCode::PageDown | KeyCode::Char(' ') => {
                self.scroll = self.scroll.saturating_add(page)
            }
            KeyCode::Home | KeyCode::Char('g') => self.scroll = 0,
            KeyCode::End | KeyCode::Char('G') => self.scroll = u16::MAX,
            _ => {}
        }
    }

    fn handle_list(&mut self, key: KeyEvent) {
        let Overlay::List { selected } = &mut self.overlay else {
            return;
        };
        let last = self.workspace.track().len().saturating_sub(1);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => *selected = (*selected + 1).min(last),
            KeyCode::PageUp => *selected = selected.saturating_sub(10),
            KeyCode::PageDown => *selected = (*selected + 10).min(last),
            KeyCode::Home | KeyCode::Char('g') => *selected = 0,
            KeyCode::End | KeyCode::Char('G') => *selected = last,
            KeyCode::Enter => {
                let index = *selected;
                self.overlay = Overlay::None;
                let chosen = self.exercise_at(index);
                if let Some(name) = chosen
                    && name != self.exercise.name
                {
                    self.go_to(&name);
                }
            }
            KeyCode::Char('x') => {
                let index = *selected;
                if let Some(name) = self.exercise_at(index) {
                    self.overlay = Overlay::ConfirmReset { exercise: name };
                }
            }
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('l') => self.overlay = Overlay::None,
            _ => {}
        }
    }

    fn handle_confirm(&mut self, key: KeyEvent) {
        let Overlay::ConfirmReset { exercise } = &self.overlay else {
            return;
        };
        let exercise = exercise.clone();
        self.overlay = Overlay::None;
        if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            self.reset(&exercise);
        }
    }

    fn handle_check_all(&mut self, key: KeyEvent) {
        let Overlay::CheckAll(all) = &self.overlay else {
            return;
        };
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') if !all.finished => all.cancel.cancel(),
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter if all.finished => {
                let solved = self.workspace.solved_count();
                let total = self.workspace.track().len();
                self.overlay = Overlay::None;
                self.say(
                    format!("{solved} of {total} exercises solved."),
                    Style::default().fg(self.pal.ok),
                );
            }
            _ => {}
        }
    }

    fn open_wave(&mut self) {
        let Some(toolchain) = self.toolchain.clone() else {
            self.say(
                "No toolchain to open the waveform with.",
                Style::default().fg(self.pal.error),
            );
            return;
        };
        match open_wave(&toolchain, &self.workspace, &self.exercise) {
            Ok(_) => self.say(
                "Waveform opened in surfer-aurora: your outputs next to the reference ones.",
                Style::default().fg(self.pal.ok),
            ),
            Err(e) => self.say(format!("{e:#}"), Style::default().fg(self.pal.error)),
        }
    }

    // -------------------------------------------------------- desenho

    fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        let [header, bar, body, footer] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .areas(area);
        self.draw_header(frame, header);
        self.draw_bar(frame, bar);
        let (prompt, result) = if body.width >= SIDE_BY_SIDE {
            let [left, right] =
                Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
                    .areas(body);
            (left, right)
        } else {
            let result_height = (self.result_lines().len() as u16 + 2).clamp(5, body.height / 2);
            let [top, bottom] =
                Layout::vertical([Constraint::Min(4), Constraint::Length(result_height)])
                    .areas(body);
            (top, bottom)
        };
        self.draw_prompt(frame, prompt);
        self.draw_result(frame, result);
        self.draw_footer(frame, footer);
        match &self.overlay {
            Overlay::None => {}
            Overlay::List { selected } => self.draw_list(frame, area, *selected),
            Overlay::ConfirmReset { exercise } => self.draw_confirm(frame, area, exercise),
            Overlay::CheckAll(all) => self.draw_check_all(frame, area, all),
            Overlay::Help => self.draw_help(frame, area),
        }
    }

    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let pal = &self.pal;
        let track = self.workspace.track();
        let solved = self.workspace.solved_count();
        let total = track.len();
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(pal.accent))
            .title(Line::from(vec![
                Span::raw(" "),
                Span::styled("Lace learn", pal.accent()),
                Span::raw(" "),
            ]))
            .title(
                Line::from(vec![
                    Span::raw(" "),
                    Span::styled(track.title.clone(), pal.bold()),
                    Span::raw(" "),
                ])
                .right_aligned(),
            );
        let ratio = if total == 0 {
            0.0
        } else {
            solved as f64 / total as f64
        };
        let gauge = Gauge::default()
            .block(block)
            .ratio(ratio)
            .use_unicode(true)
            .label(Span::styled(
                format!("{solved} of {total} solved"),
                pal.bold(),
            ))
            .gauge_style(Style::default().fg(pal.ok).bg(Color::Reset));
        frame.render_widget(gauge, area);
    }

    fn draw_bar(&self, frame: &mut Frame, area: Rect) {
        let pal = &self.pal;
        let track = self.workspace.track();
        let chapter = track
            .chapter_of(&self.exercise)
            .map_or(self.exercise.chapter.clone(), |c| c.title.clone());
        let position = track.position(&self.exercise.name).unwrap_or(0) + 1;
        let mut first = vec![
            Span::raw(" "),
            Span::styled(format!("{chapter} › "), pal.dim()),
            Span::styled(self.exercise.title.clone(), pal.bold()),
            Span::raw("  "),
            Span::styled(self.exercise.name.clone(), Style::default().fg(pal.accent)),
            Span::styled(format!("  ·  {position} of {}", track.len()), pal.dim()),
        ];
        if self.workspace.is_solved(&self.exercise.name) {
            first.push(Span::raw("  "));
            first.push(Span::styled(" SOLVED ", pal.badge(pal.ok)));
        }
        let file = self.workspace.student_file(&self.exercise);
        let shown = file
            .strip_prefix(self.workspace.root())
            .map_or_else(|_| file.to_string(), |p| p.to_string());
        let second = Line::from(vec![
            Span::raw(" "),
            Span::styled(format!("edit {shown} and save to check it"), pal.dim()),
        ]);
        frame.render_widget(Paragraph::new(vec![Line::from(first), second]), area);
    }

    /// O enunciado e as dicas à mostra, para a largura `width`.
    fn prompt_text(&self, width: usize) -> Text<'static> {
        let pal = &self.pal;
        let mut lines = markdown_lines(&self.exercise.prompt, pal, width);
        for (i, hint) in self
            .exercise
            .hints
            .iter()
            .take(self.hints_shown)
            .enumerate()
        {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                format!("Hint {}", i + 1),
                Style::default().fg(pal.warn).add_modifier(Modifier::BOLD),
            )));
            lines.extend(markdown_lines(hint, pal, width));
        }
        Text::from(lines)
    }

    fn draw_prompt(&self, frame: &mut Frame, area: Rect) {
        let pal = &self.pal;
        let inner_width = area.width.saturating_sub(4);
        let text = self.prompt_text(usize::from(inner_width));
        let inner_height = area.height.saturating_sub(2);
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
        let total = paragraph.line_count(inner_width.max(1)) as u16;
        let max = total.saturating_sub(inner_height);
        self.max_scroll.set(max);
        self.page.set(inner_height);
        let scroll = self.scroll.min(max);
        let more = if scroll < max {
            Span::styled(" ↓ more (↑↓ PgUp PgDn) ", pal.dim())
        } else if scroll > 0 {
            Span::styled(" ↑ top: Home ", pal.dim())
        } else {
            Span::raw("")
        };
        let hints = self.exercise.hints.len();
        let title = if hints > 0 {
            format!(" Exercise · hints {}/{hints} ", self.hints_shown)
        } else {
            " Exercise ".to_owned()
        };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(pal.dim())
            .title(Line::from(Span::styled(title, pal.bold())))
            .title_bottom(Line::from(more).right_aligned())
            .padding(ratatui::widgets::Padding::horizontal(1));
        frame.render_widget(paragraph.block(block).scroll((scroll, 0)), area);
    }

    fn result_lines(&self) -> Vec<Line<'static>> {
        let pal = &self.pal;
        match &self.check {
            Check::Waiting => {
                let mut lines = vec![
                    Line::from(Span::styled(" NOT CHECKED YET ", pal.badge(pal.dim))),
                    Line::default(),
                    Line::from(vec![
                        Span::raw("Save the file to check it, or press "),
                        Span::styled("r", pal.accent()),
                        Span::raw("."),
                    ]),
                ];
                if self.workspace.is_solved(&self.exercise.name) {
                    lines.push(Line::from(Span::styled(
                        "You solved this one already.",
                        Style::default().fg(pal.ok),
                    )));
                }
                lines
            }
            Check::Running { started, .. } => {
                let frame = SPINNER[self.tick % SPINNER.len()];
                vec![Line::from(vec![
                    Span::styled(format!("{frame} "), Style::default().fg(pal.accent)),
                    Span::styled("Checking", pal.bold()),
                    Span::styled(
                        format!("  {:.1} s", started.elapsed().as_secs_f32()),
                        pal.dim(),
                    ),
                ])]
            }
            Check::Failed(error) => vec![
                Line::from(Span::styled(" COULD NOT CHECK ", pal.badge(pal.error))),
                Line::default(),
                Line::from(Span::styled(error.clone(), Style::default().fg(pal.error))),
            ],
            Check::Done(grade) => grade_lines(grade, &self.workspace, &self.exercise, pal),
        }
    }

    fn draw_result(&self, frame: &mut Frame, area: Rect) {
        let pal = &self.pal;
        let color = match &self.check {
            Check::Waiting => pal.dim,
            Check::Running { .. } => pal.accent,
            Check::Failed(_) => pal.error,
            Check::Done(grade) => match grade.verdict {
                Verdict::Solved => pal.ok,
                Verdict::Cancelled | Verdict::TimedOut => pal.warn,
                _ => pal.error,
            },
        };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(color))
            .title(Line::from(Span::styled(" Result ", pal.bold())))
            .padding(ratatui::widgets::Padding::horizontal(1));
        let paragraph = Paragraph::new(self.result_lines())
            .wrap(Wrap { trim: false })
            .block(block);
        frame.render_widget(paragraph, area);
    }

    fn draw_footer(&self, frame: &mut Frame, area: Rect) {
        let pal = &self.pal;
        if let Some(note) = &self.note {
            let line = Line::from(vec![
                Span::raw(" "),
                Span::styled(note.text.clone(), note.style),
            ]);
            frame.render_widget(Paragraph::new(line), area);
            return;
        }
        let mut keys: Vec<(&str, &str)> = Vec::new();
        if self.workspace.is_solved(&self.exercise.name) {
            keys.push(("n", "next"));
        }
        keys.push(("r", "check"));
        if self.hints_shown < self.exercise.hints.len() {
            keys.push(("h", "hint"));
        }
        keys.extend([
            ("l", "list"),
            ("w", "wave"),
            ("c", "check all"),
            ("x", "reset"),
            ("?", "help"),
            ("q", "quit"),
        ]);
        let mut spans = vec![Span::raw(" ")];
        for (key, label) in keys {
            spans.push(Span::styled(key.to_owned(), pal.accent()));
            spans.push(Span::styled(format!(" {label}   "), pal.dim()));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn draw_list(&self, frame: &mut Frame, area: Rect, selected: usize) {
        let pal = &self.pal;
        let popup = centered(area, 76, area.height.saturating_sub(4).max(8));
        frame.render_widget(Clear, popup);
        let track = self.workspace.track();
        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut selected_line = 0;
        let mut index = 0;
        for chapter in &track.chapters {
            if !lines.is_empty() {
                lines.push(Line::default());
            }
            let solved = chapter
                .exercises
                .iter()
                .filter(|e| self.workspace.is_solved(&e.name))
                .count();
            lines.push(Line::from(vec![
                Span::styled(chapter.title.clone(), pal.accent()),
                Span::styled(format!("  {solved}/{}", chapter.exercises.len()), pal.dim()),
            ]));
            for exercise in &chapter.exercises {
                let is_selected = index == selected;
                let current = exercise.name == self.exercise.name;
                let solved = self.workspace.is_solved(&exercise.name);
                let mark = if solved {
                    Span::styled("● ", Style::default().fg(pal.ok))
                } else {
                    Span::styled("○ ", pal.dim())
                };
                let pointer = if current { "› " } else { "  " };
                let mut row = Line::from(vec![
                    Span::styled(pointer, pal.accent()),
                    mark,
                    Span::styled(
                        format!("{:<24}", exercise.name),
                        Style::default().fg(pal.accent),
                    ),
                    Span::raw(exercise.title.clone()),
                ]);
                if is_selected {
                    row = row.style(Style::default().add_modifier(Modifier::REVERSED));
                    selected_line = lines.len();
                }
                lines.push(row);
                index += 1;
            }
        }
        let inner_height = popup.height.saturating_sub(2) as usize;
        let offset = selected_line
            .saturating_sub(inner_height / 2)
            .min(lines.len().saturating_sub(inner_height));
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(pal.accent))
            .title(Line::from(Span::styled(" Exercises ", pal.bold())))
            .title_bottom(
                Line::from(Span::styled(
                    " ↑↓ move   enter open   x reset   esc close ",
                    pal.dim(),
                ))
                .right_aligned(),
            )
            .padding(ratatui::widgets::Padding::horizontal(1));
        frame.render_widget(
            Paragraph::new(lines)
                .block(block)
                .scroll((offset as u16, 0)),
            popup,
        );
    }

    fn draw_confirm(&self, frame: &mut Frame, area: Rect, exercise: &str) {
        let pal = &self.pal;
        let file = self
            .workspace
            .track()
            .find(exercise)
            .map_or_else(|| exercise.to_owned(), |e| format!("{}.v", e.module));
        let popup = centered(area, 60, 7);
        frame.render_widget(Clear, popup);
        let lines = vec![
            Line::from(vec![
                Span::raw("Reset "),
                Span::styled(file, pal.bold()),
                Span::raw(" to its starting point?"),
            ]),
            Line::from(Span::styled(
                "What you wrote in it is lost.",
                Style::default().fg(pal.warn),
            )),
            Line::default(),
            Line::from(vec![
                Span::styled("y", pal.accent()),
                Span::styled(" reset   ", pal.dim()),
                Span::styled("any other key", pal.accent()),
                Span::styled(" keep it", pal.dim()),
            ]),
        ];
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(pal.warn))
            .title(Line::from(Span::styled(" Reset ", pal.bold())))
            .padding(ratatui::widgets::Padding::horizontal(1));
        frame.render_widget(Paragraph::new(lines).block(block), popup);
    }

    fn draw_check_all(&self, frame: &mut Frame, area: Rect, all: &CheckAll) {
        let pal = &self.pal;
        let popup = centered(area, 64, area.height.saturating_sub(6).clamp(8, 20));
        frame.render_widget(Clear, popup);
        let [gauge_area, list_area] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(popup);
        let done = all.done.len();
        let title = if all.finished {
            " Checked "
        } else {
            " Checking every exercise "
        };
        let gauge = Gauge::default()
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(pal.accent))
                    .title(Line::from(Span::styled(title, pal.bold()))),
            )
            .ratio(if all.total == 0 {
                1.0
            } else {
                done as f64 / all.total as f64
            })
            .use_unicode(true)
            .label(format!("{done} of {}", all.total))
            .gauge_style(Style::default().fg(pal.accent));
        frame.render_widget(gauge, gauge_area);
        let height = list_area.height.saturating_sub(2) as usize;
        let lines: Vec<Line<'static>> = all
            .done
            .iter()
            .skip(all.done.len().saturating_sub(height))
            .map(|(name, verdict)| {
                let (word, color) = match verdict {
                    Ok(v) => verdict_word(*v, pal),
                    Err(_) => ("could not check", pal.error),
                };
                Line::from(vec![
                    Span::styled(format!("{word:<16}"), Style::default().fg(color)),
                    Span::raw(name.clone()),
                ])
            })
            .collect();
        let hint = if all.finished {
            " enter close "
        } else {
            " esc stop "
        };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(pal.dim())
            .title_bottom(Line::from(Span::styled(hint, pal.dim())).right_aligned())
            .padding(ratatui::widgets::Padding::horizontal(1));
        frame.render_widget(Paragraph::new(lines).block(block), list_area);
    }

    fn draw_help(&self, frame: &mut Frame, area: Rect) {
        let pal = &self.pal;
        let rows: [(&str, &str); 12] = [
            ("save the file", "check the exercise"),
            ("r", "check now"),
            ("h", "show the next hint"),
            ("n", "next exercise to solve (once this one is solved)"),
            ("l", "list the exercises and choose one"),
            ("w", "waveform: your outputs next to the reference ones"),
            ("c", "check every exercise"),
            ("x", "reset the file to its starting point"),
            ("↑ ↓ PgUp PgDn", "scroll the exercise text"),
            ("Home End", "top and end of the text"),
            ("?", "this help"),
            ("q  Ctrl+C", "quit"),
        ];
        let popup = centered(area, 70, rows.len() as u16 + 4);
        frame.render_widget(Clear, popup);
        let lines: Vec<Line<'static>> = rows
            .iter()
            .map(|(key, what)| {
                Line::from(vec![
                    Span::styled(format!("{key:<16}"), pal.accent()),
                    Span::raw(*what),
                ])
            })
            .collect();
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(pal.accent))
            .title(Line::from(Span::styled(" Keys ", pal.bold())))
            .title_bottom(Line::from(Span::styled(" any key closes ", pal.dim())).right_aligned())
            .padding(ratatui::widgets::Padding::new(1, 1, 1, 0));
        frame.render_widget(Paragraph::new(lines).block(block), popup);
    }
}

// ------------------------------------------------------------ o resultado

/// A palavra e a cor de cada veredito.
fn verdict_word(verdict: Verdict, pal: &Palette) -> (&'static str, Color) {
    match verdict {
        Verdict::Solved => ("solved", pal.ok),
        Verdict::Mismatch => ("wrong output", pal.error),
        Verdict::CompileError => ("does not compile", pal.error),
        Verdict::TimedOut => ("timed out", pal.warn),
        Verdict::Cancelled => ("cancelled", pal.warn),
        _ => ("incomplete", pal.error),
    }
}

/// As linhas da correção: o selo, cada saída que errou, os erros com
/// arquivo e linha, o que a correção percebeu e o que fazer.
fn grade_lines(
    grade: &Grade,
    workspace: &Workspace,
    exercise: &Exercise,
    pal: &Palette,
) -> Vec<Line<'static>> {
    let (word, color) = verdict_word(grade.verdict, pal);
    let mut lines = Vec::new();
    let mut head = vec![Span::styled(
        format!(" {} ", word.to_uppercase()),
        pal.badge(color),
    )];
    if grade.verdict == Verdict::Mismatch {
        head.push(Span::styled(
            format!("  {} of {} samples wrong", grade.mismatched, grade.samples),
            Style::default().fg(color),
        ));
    }
    head.push(Span::styled(
        format!("  {:.1} s", grade.duration_ms as f64 / 1000.0),
        pal.dim(),
    ));
    lines.push(Line::from(head));

    let wrong: Vec<_> = grade.outputs.iter().filter(|o| o.mismatches > 0).collect();
    if !wrong.is_empty() {
        lines.push(Line::default());
        let width = wrong
            .iter()
            .map(|o| o.name.chars().count())
            .max()
            .unwrap_or(6)
            .max(6);
        lines.push(Line::from(Span::styled(
            format!(
                "{:<width$}  {:>7}  {:>11}  {:>5}",
                "output", "wrong", "first", "x/z"
            ),
            pal.dim(),
        )));
        for o in wrong {
            let first = o
                .first_ns
                .map_or_else(|| "-".to_owned(), |ns| format!("{ns} ns"));
            lines.push(Line::from(vec![
                Span::styled(format!("{:<width$}", o.name), pal.accent()),
                Span::styled(
                    format!("  {:>7}", o.mismatches),
                    Style::default().fg(pal.error),
                ),
                Span::raw(format!("  {first:>11}")),
                Span::styled(format!("  {:>5}", o.unknown), pal.dim()),
            ]));
        }
    }

    if !grade.diagnostics.is_empty() {
        lines.push(Line::default());
        let dir = workspace.exercise_dir(exercise);
        for d in &grade.diagnostics {
            let place = match (&d.file, d.line) {
                (Some(file), line) => {
                    let shown = file
                        .strip_prefix(&dir)
                        .map_or_else(|_| file.to_string(), |p| p.to_string());
                    match line {
                        Some(n) => format!("{shown}:{n}"),
                        None => shown,
                    }
                }
                _ => String::new(),
            };
            let (label, severity) = match d.severity {
                Severity::Error => ("error", pal.error),
                Severity::Warning => ("warning", pal.warn),
                _ => ("note", pal.dim),
            };
            let mut spans = Vec::new();
            if !place.is_empty() {
                spans.push(Span::styled(place, Style::default().fg(pal.accent)));
                spans.push(Span::raw(" "));
            }
            spans.push(Span::styled(
                format!("{label}: "),
                Style::default().fg(severity).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(d.message.clone()));
            lines.push(Line::from(spans));
        }
    }

    if !grade.findings.is_empty() {
        lines.push(Line::default());
        for finding in &grade.findings {
            lines.push(Line::from(vec![
                Span::styled("› ", Style::default().fg(pal.warn)),
                Span::styled(finding_text(finding), Style::default().fg(pal.warn)),
            ]));
        }
    }

    if !grade.output.is_empty() {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled("Testbench output", pal.dim())));
        for out in &grade.output {
            lines.push(Line::from(Span::styled(format!("│ {out}"), pal.dim())));
        }
    }

    lines.push(Line::default());
    match grade.verdict {
        Verdict::Solved => {
            let solution = workspace.solution_path(exercise);
            let shown = solution
                .strip_prefix(workspace.root())
                .map_or_else(|_| solution.to_string(), |p| p.to_string());
            lines.push(Line::from(vec![
                Span::raw("Compare with the reference solution: "),
                Span::styled(shown, Style::default().fg(pal.accent)),
            ]));
            lines.push(Line::from(vec![
                Span::raw("Press "),
                Span::styled("n", pal.accent()),
                Span::raw(" for the next exercise."),
            ]));
        }
        Verdict::Mismatch => lines.push(Line::from(vec![
            Span::raw("Press "),
            Span::styled("w", pal.accent()),
            Span::raw(" to see your outputs next to the reference ones."),
        ])),
        Verdict::CompileError => lines.push(Line::from(Span::raw(
            "Fix the line above and save again.",
        ))),
        Verdict::TimedOut => lines.push(Line::from(Span::raw(
            "The simulation did not finish in time: a loop that never ends, or a combinational loop that never settles.",
        ))),
        Verdict::Incomplete => lines.push(Line::from(Span::raw(
            "The simulation stopped before the testbench finished.",
        ))),
        _ => {}
    }
    lines
}

// ------------------------------------------------------------ o enunciado

/// O markdown da trilha em linhas com cor: títulos no destaque, listas com
/// marcador, blocos de código com as palavras do Verilog em destaque,
/// tabelas alinhadas, e código, negrito e itálico no texto. Os itens de
/// lista já saem quebrados em `width`, com as linhas de baixo alinhadas ao
/// texto do item.
fn markdown_lines(markdown: &str, pal: &Palette, width: usize) -> Vec<Line<'static>> {
    let source: Vec<&str> = markdown.lines().collect();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut paragraph: Vec<String> = Vec::new();
    let flush = |paragraph: &mut Vec<String>, lines: &mut Vec<Line<'static>>| {
        if !paragraph.is_empty() {
            lines.push(Line::from(inline_spans(
                &paragraph.join(" "),
                pal,
                pal.plain(),
            )));
            paragraph.clear();
        }
    };
    let mut i = 0;
    while i < source.len() {
        let line = source[i];
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            flush(&mut paragraph, &mut lines);
            i += 1;
            while i < source.len() && !source[i].trim().starts_with("```") {
                let mut spans = vec![Span::styled("  ", pal.plain())];
                spans.extend(verilog_spans(source[i], pal));
                lines.push(Line::from(spans));
                i += 1;
            }
            i += 1;
            continue;
        }
        if let Some(title) = trimmed
            .strip_prefix('#')
            .map(|t| t.trim_start_matches('#').trim())
            && trimmed.starts_with('#')
        {
            flush(&mut paragraph, &mut lines);
            if !lines.is_empty() && lines.last().is_some_and(|l| !l.spans.is_empty()) {
                lines.push(Line::default());
            }
            lines.push(Line::from(Span::styled(title.to_owned(), pal.accent())));
            i += 1;
            continue;
        }
        if is_table_row(trimmed) && source.get(i + 1).is_some_and(|l| is_table_rule(l.trim())) {
            flush(&mut paragraph, &mut lines);
            let header = cells(trimmed);
            let mut rows: Vec<Vec<String>> = Vec::new();
            i += 2;
            while i < source.len() && is_table_row(source[i].trim()) {
                rows.push(cells(source[i].trim()));
                i += 1;
            }
            lines.extend(table_lines(&header, &rows, pal));
            continue;
        }
        let bullet = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "));
        let numbered = numbered_item(trimmed);
        if bullet.is_some() || numbered.is_some() {
            flush(&mut paragraph, &mut lines);
            let (mark, text) = match (bullet, numbered) {
                (Some(text), _) => ("•".to_owned(), text.to_owned()),
                (None, Some((n, text))) => (format!("{n}."), text.to_owned()),
                _ => unreachable!(),
            };
            // As linhas recuadas logo abaixo continuam o item.
            let mut text = text;
            while i + 1 < source.len()
                && source[i + 1].starts_with("  ")
                && !source[i + 1].trim().is_empty()
                && source[i + 1].trim_start().strip_prefix("- ").is_none()
            {
                i += 1;
                text.push(' ');
                text.push_str(source[i].trim());
            }
            let prefix = Span::styled(format!("  {mark} "), Style::default().fg(pal.accent));
            lines.extend(hanging(
                prefix,
                inline_spans(&text, pal, pal.plain()),
                width,
            ));
            i += 1;
            continue;
        }
        if trimmed.is_empty() {
            flush(&mut paragraph, &mut lines);
            if lines.last().is_some_and(|l| !l.spans.is_empty()) {
                lines.push(Line::default());
            }
            i += 1;
            continue;
        }
        paragraph.push(trimmed.to_owned());
        i += 1;
    }
    flush(&mut paragraph, &mut lines);
    while lines.last().is_some_and(|l| l.spans.is_empty()) {
        lines.pop();
    }
    lines
}

/// Um item de lista quebrado em `width` colunas, com as linhas de baixo
/// recuadas até o texto do item (o `Paragraph` voltaria à margem).
fn hanging(prefix: Span<'static>, content: Vec<Span<'static>>, width: usize) -> Vec<Line<'static>> {
    let indent = prefix.content.chars().count();
    let mut lines = Vec::new();
    let mut current: Vec<Span<'static>> = vec![prefix];
    let mut used = indent;
    for span in content {
        let style = span.style;
        for token in words(&span.content) {
            let word = token.trim_end().chars().count();
            if width > indent + 8 && used > indent && used + word > width {
                lines.push(Line::from(std::mem::take(&mut current)));
                current.push(Span::raw(" ".repeat(indent)));
                used = indent;
            }
            used += token.chars().count();
            current.push(Span::styled(token.to_owned(), style));
        }
    }
    lines.push(Line::from(current));
    lines
}

/// As palavras de um texto, cada uma com os espaços que vêm depois dela.
fn words(text: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let mut start = 0;
    let mut in_space = false;
    for (at, c) in text.char_indices() {
        if c == ' ' {
            in_space = true;
        } else if in_space {
            words.push(&text[start..at]);
            start = at;
            in_space = false;
        }
    }
    if start < text.len() {
        words.push(&text[start..]);
    }
    words
}

fn numbered_item(text: &str) -> Option<(String, &str)> {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let rest = &text[digits..];
    rest.strip_prefix(". ")
        .or_else(|| rest.strip_prefix(") "))
        .map(|item| (text[..digits].to_owned(), item))
}

/// O texto de uma linha com `código`, **negrito** e *itálico*.
fn inline_spans(text: &str, pal: &Palette, base: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let next = ["`", "**", "*", "_"]
            .iter()
            .filter_map(|m| rest.find(m).map(|at| (at, *m)))
            .filter(|(at, m)| {
                // `_` e `*` só marcam no começo de palavra.
                *m == "`"
                    || *m == "**"
                    || *at == 0
                    || !rest[..*at].ends_with(|c: char| c.is_alphanumeric())
            })
            .min_by_key(|(at, m)| (*at, std::cmp::Reverse(m.len())));
        let Some((at, mark)) = next else {
            spans.push(Span::styled(rest.to_owned(), base));
            break;
        };
        let after = &rest[at + mark.len()..];
        let Some(end) = after.find(mark) else {
            spans.push(Span::styled(rest.to_owned(), base));
            break;
        };
        if at > 0 {
            spans.push(Span::styled(rest[..at].to_owned(), base));
        }
        let inner = after[..end].to_owned();
        let style = match mark {
            "`" => Style::default().fg(pal.code),
            "**" => base.add_modifier(Modifier::BOLD),
            _ => base.add_modifier(Modifier::ITALIC),
        };
        spans.push(Span::styled(inner, style));
        rest = &after[end + mark.len()..];
    }
    spans
}

/// Uma linha de código Verilog: palavras reservadas, números e comentários
/// com cores próprias.
fn verilog_spans(line: &str, pal: &Palette) -> Vec<Span<'static>> {
    const KEYWORDS: &[&str] = &[
        "module",
        "endmodule",
        "input",
        "output",
        "inout",
        "wire",
        "reg",
        "assign",
        "always",
        "initial",
        "begin",
        "end",
        "if",
        "else",
        "case",
        "casez",
        "endcase",
        "default",
        "posedge",
        "negedge",
        "or",
        "parameter",
        "localparam",
        "integer",
        "for",
        "generate",
        "endgenerate",
        "genvar",
        "signed",
        "function",
        "endfunction",
    ];
    let code = Style::default().fg(pal.code);
    let (body, comment) = match line.find("//") {
        Some(at) => (&line[..at], Some(&line[at..])),
        None => (line, None),
    };
    let mut spans = Vec::new();
    let mut word = String::new();
    let flush = |word: &mut String, spans: &mut Vec<Span<'static>>| {
        if word.is_empty() {
            return;
        }
        let style = if KEYWORDS.contains(&word.as_str()) {
            Style::default()
                .fg(pal.keyword)
                .add_modifier(Modifier::BOLD)
        } else if word.starts_with(|c: char| c.is_ascii_digit()) {
            Style::default().fg(pal.number)
        } else {
            code
        };
        spans.push(Span::styled(std::mem::take(word), style));
    };
    for c in body.chars() {
        if c.is_alphanumeric() || c == '_' || c == '\'' || c == '$' {
            word.push(c);
        } else {
            flush(&mut word, &mut spans);
            spans.push(Span::styled(c.to_string(), code));
        }
    }
    flush(&mut word, &mut spans);
    if let Some(comment) = comment {
        spans.push(Span::styled(
            comment.to_owned(),
            pal.dim().add_modifier(Modifier::ITALIC),
        ));
    }
    spans
}

fn is_table_row(line: &str) -> bool {
    line.len() > 1 && line.starts_with('|') && line.ends_with('|')
}

fn is_table_rule(line: &str) -> bool {
    is_table_row(line) && line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

fn cells(line: &str) -> Vec<String> {
    line[1..line.len() - 1]
        .split('|')
        .map(|c| c.trim().to_owned())
        .collect()
}

/// Uma tabela com as colunas alinhadas, o cabeçalho em negrito e um traço
/// embaixo dele.
fn table_lines(header: &[String], rows: &[Vec<String>], pal: &Palette) -> Vec<Line<'static>> {
    let visible = |cell: &str| cell.replace('`', "").chars().count();
    let columns = header.len();
    let mut widths: Vec<usize> = header.iter().map(|h| visible(h)).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate().take(columns) {
            widths[i] = widths[i].max(visible(cell));
        }
    }
    let row_line = |row: &[String], base: Style| -> Line<'static> {
        let mut spans = vec![Span::raw("  ")];
        for (i, width) in widths.iter().enumerate() {
            let cell = row.get(i).map(String::as_str).unwrap_or("");
            spans.extend(inline_spans(cell, pal, base));
            let pad = width.saturating_sub(visible(cell)) + 3;
            spans.push(Span::raw(" ".repeat(pad)));
        }
        Line::from(spans)
    };
    let mut lines = vec![row_line(header, pal.bold())];
    let rule: usize = widths
        .iter()
        .map(|w| w + 3)
        .sum::<usize>()
        .saturating_sub(3);
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled("─".repeat(rule), pal.dim()),
    ]));
    for row in rows {
        lines.push(row_line(row, pal.plain()));
    }
    lines
}

/// Um retângulo de `width` x `height` no meio de `area` (menor, se não
/// couber).
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::{Utf8Path, Utf8PathBuf};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// Uma pasta de exercícios da trilha de teste do lace-learn, sem
    /// ferramenta: a tela anda, e corrigir vira aviso.
    fn app() -> (tempfile::TempDir, App) {
        let fixtures =
            Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("../lace-learn/tests/fixtures");
        let track = lace_learn::load_track(&fixtures, "teste", None).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let dir = Utf8PathBuf::from_path_buf(dunce::canonicalize(tmp.path()).unwrap()).unwrap();
        let workspace = Workspace::init(&dir.join("ex"), track).unwrap();
        (tmp, App::new(None, workspace, CancelToken::new()))
    }

    fn screen(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| app.draw(f)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let mut text = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }

    /// A tela inteira, com uma dica à mostra: o registro do layout.
    #[test]
    fn the_screen_layout() {
        let (_tmp, mut app) = app();
        app.handle(key(KeyCode::Char('h')));
        insta::assert_snapshot!(screen(&app, 120, 34).replace('\\', "/"));
    }

    #[test]
    fn the_exercise_appears_without_being_checked() {
        let (_tmp, mut app) = app();
        app.poll();
        assert!(matches!(app.check, Check::Waiting));
        let text = screen(&app, 140, 36);
        assert!(text.contains("Multiplexador 2:1"), "{text}");
        assert!(text.contains("NOT CHECKED YET"), "{text}");
        assert!(text.contains("0 of 4 solved"), "{text}");
        // Estreito, o resultado vai para baixo do enunciado.
        let narrow = screen(&app, 80, 36);
        assert!(narrow.contains("NOT CHECKED YET"), "{narrow}");
    }

    #[test]
    fn hints_come_one_at_a_time() {
        let (_tmp, mut app) = app();
        assert!(!screen(&app, 140, 36).contains("Hint 1"));
        app.handle(key(KeyCode::Char('h')));
        let text = screen(&app, 140, 36);
        assert!(text.contains("Hint 1"), "{text}");
        assert!(text.contains("Use o operador"), "{text}");
        app.handle(key(KeyCode::Char('h')));
        assert!(app.note.is_some(), "sem mais dicas, um aviso");
    }

    #[test]
    fn the_list_chooses_another_exercise_without_checking_it() {
        let (_tmp, mut app) = app();
        app.handle(key(KeyCode::Char('l')));
        let text = screen(&app, 140, 36);
        assert!(text.contains("Exercises"), "{text}");
        assert!(text.contains("Sequencial"), "{text}");
        app.handle(key(KeyCode::Down));
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.exercise.name, "soma16");
        assert!(matches!(app.check, Check::Waiting));
        assert!(matches!(app.overlay, Overlay::None));
    }

    #[test]
    fn next_needs_the_exercise_solved_and_reset_asks_first() {
        let (_tmp, mut app) = app();
        app.handle(key(KeyCode::Char('n')));
        assert_eq!(app.exercise.name, "mux2");
        assert!(
            app.note
                .as_ref()
                .is_some_and(|n| n.text.contains("Solve this exercise first"))
        );

        let file = app.workspace.student_file(&app.exercise);
        std::fs::write(&file, "// meu\n").unwrap();
        app.handle(key(KeyCode::Char('x')));
        assert!(screen(&app, 140, 36).contains("to its starting point?"));
        app.handle(key(KeyCode::Char('n')));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "// meu\n");
        app.handle(key(KeyCode::Char('x')));
        app.handle(key(KeyCode::Char('y')));
        assert!(
            std::fs::read_to_string(&file)
                .unwrap()
                .contains("module mux2")
        );
        // Restaurar não corrige.
        app.poll();
        std::thread::sleep(SETTLE + Duration::from_millis(50));
        app.poll();
        assert!(matches!(app.check, Check::Waiting));
    }

    #[test]
    fn saving_the_file_asks_for_a_check() {
        let (_tmp, mut app) = app();
        let file = app.workspace.student_file(&app.exercise);
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(
            &file,
            "module mux2(input a, input b, input sel, output y);\nendmodule\n",
        )
        .unwrap();
        app.poll();
        assert!(
            matches!(app.check, Check::Waiting),
            "espera o editor terminar"
        );
        std::thread::sleep(SETTLE + Duration::from_millis(50));
        app.poll();
        // Sem ferramenta nos testes, a correção pedida vira aviso.
        assert!(matches!(app.check, Check::Failed(_)));
    }

    #[test]
    fn long_list_items_wrap_under_their_text() {
        let pal = Palette::new();
        let lines = markdown_lines(
            "- um item comprido que precisa quebrar em mais de uma linha",
            &pal,
            24,
        );
        let plain: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(plain.len() > 1, "{plain:?}");
        assert!(plain[0].starts_with("  • um"), "{plain:?}");
        for line in &plain[1..] {
            assert!(
                line.starts_with("    ") && !line.starts_with("     "),
                "{plain:?}"
            );
            assert!(line.trim_end().chars().count() <= 24, "{plain:?}");
        }
    }

    #[test]
    fn without_truecolor_the_colors_go_to_the_256_palette() {
        assert_eq!(xterm256(0, 0, 0), 16);
        assert_eq!(xterm256(255, 255, 255), 231);
        assert_eq!(xterm256(128, 128, 128), 244);
        // O azul do ATLAS fica no (0, 135, 175) do cubo.
        assert_eq!(xterm256(11, 128, 195), 31);
        assert_eq!(Palette::with(Colors::Indexed).accent, Color::Indexed(31));
        assert_eq!(Palette::with(Colors::None).accent, Color::Reset);
    }

    #[test]
    fn the_prompt_markdown_gets_titles_tables_and_code() {
        let pal = Palette::new();
        let lines = markdown_lines(
            "Texto com `codigo` e **negrito**.\n\n## Portas\n\n| Porta | Bits |\n|---|---|\n| `y` | 1 |\n\n- um item\n  que continua\n\n```verilog\nassign y = a; // fio\n```",
            &pal,
            80,
        );
        let plain: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(
            plain.contains(&"Texto com codigo e negrito.".to_owned()),
            "{plain:?}"
        );
        assert!(plain.contains(&"Portas".to_owned()), "{plain:?}");
        assert!(
            plain.iter().any(|l| l.contains("Porta   Bits")),
            "{plain:?}"
        );
        assert!(
            plain.iter().any(|l| l.contains("• um item que continua")),
            "{plain:?}"
        );
        assert!(
            plain.iter().any(|l| l.contains("assign y = a; // fio")),
            "{plain:?}"
        );
    }

    /// Com o bundle, a correção de verdade aparece na tela.
    #[test]
    fn a_wrong_answer_shows_the_outputs_and_the_first_mismatch() {
        let Ok(bundle) = std::env::var("LACE_TEST_BUNDLE") else {
            eprintln!("PULADO: defina LACE_TEST_BUNDLE");
            return;
        };
        let (_tmp, mut app) = app();
        app.toolchain = Some(Toolchain::open(&bundle).unwrap());
        let file = app.workspace.student_file(&app.exercise);
        std::fs::write(
            &file,
            "module mux2(input a, input b, input sel, output y);\n    assign y = sel ? a : b;\nendmodule\n",
        )
        .unwrap();
        app.handle(key(KeyCode::Char('r')));
        let started = Instant::now();
        while matches!(app.check, Check::Running { .. })
            && started.elapsed() < Duration::from_secs(60)
        {
            std::thread::sleep(Duration::from_millis(50));
            app.poll();
        }
        let text = screen(&app, 140, 36);
        assert!(text.contains("WRONG OUTPUT"), "{text}");
        assert!(text.contains("25 ns"), "{text}");
    }
}
