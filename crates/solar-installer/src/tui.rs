//! A instalação guiada no terminal.
//!
//! Telas, nesta ordem: boas-vindas, tipo de instalação (Recomendada ou
//! Avançada), componentes (só na Avançada), destino, resumo, progresso e
//! fim. `Esc` volta uma tela; `Ctrl+C` sai (menos durante a extração).
//!
//! [`App`] guarda o estado e trata as teclas ([`App::handle`]); o desenho é
//! [`App::draw`]. Os dois são separados para testar as telas sem terminal.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use ratatui::crossterm::event::{
    self, Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Gauge, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use solar_core::SystemCompiler;

use crate::install::{self, Event, LinkOutcome, Receipt, Report, Target};
use crate::payload::Index;
use crate::plan::{self, Profile, Selection};

/// As telas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Boas-vindas.
    Welcome,
    /// Recomendada ou Avançada.
    Profile,
    /// A lista de componentes (instalação avançada).
    Components,
    /// Pasta e atalho.
    Destination,
    /// O que vai ser feito.
    Summary,
    /// Extraindo.
    Installing,
    /// Terminou (bem ou mal).
    Done,
}

/// O que o laço principal precisa fazer depois de uma tecla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Nada.
    None,
    /// Começar a instalação numa thread.
    Install,
    /// Sair.
    Quit,
}

enum Msg {
    Progress(Event),
    // Na Box: o Report é grande (no Windows, bem maior que o Progress).
    Finished(Box<Result<Report, String>>),
}

#[derive(Debug, Default, Clone)]
struct Progress {
    total: u64,
    done: u64,
    label: String,
}

/// O estado da instalação guiada.
pub struct App {
    /// A tela atual.
    pub screen: Screen,
    index: Index,
    payload_dir: PathBuf,
    /// O tipo de instalação escolhido.
    pub profile: Profile,
    profile_cursor: usize,
    /// Os componentes escolhidos.
    pub selection: Selection,
    component_cursor: usize,
    /// O texto do campo da pasta de instalação.
    pub prefix: String,
    /// Criar o atalho?
    pub link: bool,
    link_path: Option<PathBuf>,
    focus_link: bool,
    existing: Option<Receipt>,
    compiler: Option<SystemCompiler>,
    error: Option<String>,
    progress: Progress,
    /// O resultado, quando terminar.
    pub outcome: Option<Result<Report, String>>,
    rx: Option<mpsc::Receiver<Msg>>,
}

impl App {
    /// Estado inicial, com o destino padrão e o compilador do sistema (para
    /// avisar sobre o Verilator).
    pub fn new(
        index: Index,
        payload_dir: PathBuf,
        target: Target,
        compiler: Option<SystemCompiler>,
    ) -> App {
        let existing = Receipt::load(&target.prefix);
        let selection = plan::recommended(&index);
        App {
            screen: Screen::Welcome,
            index,
            payload_dir,
            profile: Profile::Recommended,
            profile_cursor: 0,
            selection,
            component_cursor: 0,
            prefix: target.prefix.to_string_lossy().into_owned(),
            link: target.link.is_some(),
            link_path: target.link,
            focus_link: false,
            existing,
            compiler,
            error: None,
            progress: Progress::default(),
            outcome: None,
            rx: None,
        }
    }

    /// O destino que o usuário escolheu.
    pub fn target(&self) -> Target {
        Target {
            prefix: install::expand_home(&self.prefix),
            link: if self.link {
                self.link_path.clone()
            } else {
                None
            },
        }
    }

    /// Trata uma tecla.
    pub fn handle(&mut self, key: KeyEvent) -> Command {
        if key.kind != KeyEventKind::Press {
            return Command::None;
        }
        let ctrl_c =
            key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
        if ctrl_c {
            if self.screen == Screen::Installing {
                self.error = Some("aguarde: a extração não pode ser interrompida no meio".into());
                return Command::None;
            }
            return Command::Quit;
        }
        self.error = None;
        match self.screen {
            Screen::Welcome => match key.code {
                KeyCode::Enter => self.screen = Screen::Profile,
                KeyCode::Esc | KeyCode::Char('q') => return Command::Quit,
                _ => {}
            },
            Screen::Profile => match key.code {
                KeyCode::Up | KeyCode::Char('k') => self.profile_cursor = 0,
                KeyCode::Down | KeyCode::Char('j') => self.profile_cursor = 1,
                KeyCode::Enter => {
                    if self.profile_cursor == 0 {
                        self.profile = Profile::Recommended;
                        self.selection = plan::recommended(&self.index);
                        self.screen = Screen::Destination;
                    } else {
                        self.profile = Profile::Advanced;
                        // Parte do que já está instalado, se houver.
                        if let Some(r) = &self.existing {
                            self.selection = r
                                .components
                                .iter()
                                .filter(|c| self.index.component(c).is_some())
                                .cloned()
                                .collect();
                        }
                        self.screen = Screen::Components;
                    }
                }
                KeyCode::Esc => self.screen = Screen::Welcome,
                KeyCode::Char('q') => return Command::Quit,
                _ => {}
            },
            Screen::Components => {
                let count = self.index.components.len();
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.component_cursor = self.component_cursor.saturating_sub(1)
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.component_cursor =
                            (self.component_cursor + 1).min(count.saturating_sub(1))
                    }
                    KeyCode::Char(' ') => {
                        let name = self.index.components[self.component_cursor].name.clone();
                        plan::toggle(&self.index, &mut self.selection, &name);
                    }
                    KeyCode::Enter => self.screen = Screen::Destination,
                    KeyCode::Esc => self.screen = Screen::Profile,
                    KeyCode::Char('q') => return Command::Quit,
                    _ => {}
                }
            }
            Screen::Destination => match key.code {
                KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down => {
                    if self.link_path.is_some() {
                        self.focus_link = !self.focus_link;
                    }
                }
                KeyCode::Char(' ') if self.focus_link => self.link = !self.link,
                KeyCode::Char(c) if !self.focus_link => self.prefix.push(c),
                KeyCode::Backspace if !self.focus_link => {
                    self.prefix.pop();
                }
                KeyCode::Enter => {
                    match install::check_prefix(&install::expand_home(&self.prefix)) {
                        Ok(existing) => {
                            self.existing = existing;
                            self.screen = Screen::Summary;
                        }
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
                KeyCode::Esc => {
                    self.screen = match self.profile {
                        Profile::Advanced => Screen::Components,
                        Profile::Recommended => Screen::Profile,
                    }
                }
                _ => {}
            },
            Screen::Summary => match key.code {
                KeyCode::Enter => {
                    self.screen = Screen::Installing;
                    return Command::Install;
                }
                KeyCode::Esc => self.screen = Screen::Destination,
                KeyCode::Char('q') => return Command::Quit,
                _ => {}
            },
            Screen::Installing => {}
            Screen::Done => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q')) {
                    return Command::Quit;
                }
            }
        }
        Command::None
    }

    /// Começa a extração numa thread; o progresso chega por [`App::poll`].
    pub fn start_install(&mut self) {
        let (tx, rx) = mpsc::channel();
        let payload_dir = self.payload_dir.clone();
        let index = self.index.clone();
        let selection = self.selection.clone();
        let target = self.target();
        std::thread::spawn(move || {
            let progress = tx.clone();
            let result = install::install(&payload_dir, &index, &selection, &target, |e| {
                let _ = progress.send(Msg::Progress(e));
            });
            let _ = tx.send(Msg::Finished(Box::new(
                result.map_err(|e| format!("{e:#}")),
            )));
        });
        self.rx = Some(rx);
    }

    /// Recebe o que a thread de instalação mandou.
    pub fn poll(&mut self) {
        let Some(rx) = &self.rx else { return };
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Msg::Progress(Event::Start { total }) => self.progress.total = total,
                Msg::Progress(Event::Chunk {
                    index,
                    count,
                    components,
                }) => {
                    let what = if components.is_empty() {
                        "Solar".to_owned()
                    } else {
                        components
                            .iter()
                            .map(|c| self.label(c))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    self.progress.label = format!("[{index}/{count}] {what}");
                }
                Msg::Progress(Event::Progress { done }) => self.progress.done = done,
                Msg::Progress(Event::Verifying) => {
                    self.progress.label = "conferindo os executáveis".into()
                }
                Msg::Finished(result) => {
                    self.outcome = Some(*result);
                    self.screen = Screen::Done;
                }
            }
        }
    }

    fn label(&self, name: &str) -> String {
        self.index
            .component(name)
            .map_or_else(|| name.to_owned(), |c| c.label.clone())
    }

    fn selection_labels(&self) -> String {
        let labels: Vec<String> = self
            .index
            .components
            .iter()
            .filter(|c| self.selection.contains(&c.name))
            .map(|c| c.label.clone())
            .collect();
        if labels.is_empty() {
            "nenhum componente".into()
        } else {
            labels.join(", ")
        }
    }

    /// Desenha a tela atual.
    pub fn draw(&self, frame: &mut Frame) {
        let area = centered(frame.area(), 92, 30);
        let title = format!(
            " Solar {} · instalador ({}) ",
            self.index.solar_version, self.index.platform
        );
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .title(Span::styled(title, bold().fg(Color::Cyan)));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let [body, message, keys] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .areas(inner);
        let body = body.inner(ratatui::layout::Margin::new(2, 1));

        match self.screen {
            Screen::Welcome => self.draw_welcome(frame, body),
            Screen::Profile => self.draw_profile(frame, body),
            Screen::Components => self.draw_components(frame, body),
            Screen::Destination => self.draw_destination(frame, body),
            Screen::Summary => self.draw_summary(frame, body),
            Screen::Installing => self.draw_installing(frame, body),
            Screen::Done => self.draw_done(frame, body),
        }
        if let Some(error) = &self.error {
            frame.render_widget(
                Paragraph::new(error.as_str())
                    .style(Style::default().fg(Color::Red))
                    .wrap(Wrap { trim: true }),
                message.inner(ratatui::layout::Margin::new(2, 0)),
            );
        }
        let hint = match self.screen {
            Screen::Welcome => "Enter continuar   Esc sair",
            Screen::Profile => "↑↓ escolher   Enter continuar   Esc voltar   q sair",
            Screen::Components => "↑↓ mover   Espaço marcar   Enter continuar   Esc voltar",
            Screen::Destination => "digite a pasta   Tab atalho   Enter continuar   Esc voltar",
            Screen::Summary => "Enter instalar   Esc voltar   q sair",
            Screen::Installing => "extraindo...",
            Screen::Done => "Enter sair",
        };
        frame.render_widget(
            Paragraph::new(Span::styled(hint, Style::default().fg(Color::DarkGray))),
            keys.inner(ratatui::layout::Margin::new(2, 0)),
        );
    }

    fn draw_welcome(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![
            Line::from(Span::styled("Bem-vindo ao instalador do Solar", bold())),
            Line::from(""),
            Line::from(
                "O Solar orquestra as ferramentas de desenvolvimento do SAPHO: o YANC, o Icarus \
                 Verilog, o Verilator, o Yosys, o Graphviz e o surfer-aurora.",
            ),
            Line::from(""),
            Line::from(format!(
                "Ele só usa as ferramentas do bundle instalado junto com ele, nas versões exatas \
                 do bundle {}. Nenhuma ferramenta do sistema é usada, com uma exceção: o \
                 Verilator compila com o compilador C++, o make e o Perl do sistema.",
                self.index.bundle
            )),
            Line::from(""),
            Line::from("A seguir: escolher o tipo de instalação, a pasta e confirmar."),
        ];
        if let Some(r) = &self.existing {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!(
                    "Já há uma instalação do Solar {} em {} ({}). Ela será substituída.",
                    r.solar_version,
                    self.prefix,
                    r.components.join(", ")
                ),
                Style::default().fg(Color::Yellow),
            )));
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
    }

    fn draw_profile(&self, frame: &mut Frame, area: Rect) {
        let rec = plan::recommended(&self.index);
        let rec_labels: Vec<&str> = self
            .index
            .components
            .iter()
            .filter(|c| c.recommended)
            .map(|c| c.label.as_str())
            .collect();
        let options = [
            (
                "Recomendada",
                format!(
                    "{} ({})",
                    rec_labels.join(", "),
                    crate::mib(self.index.size_of(&rec))
                ),
            ),
            (
                "Avançada",
                "escolher exatamente os componentes a instalar".to_owned(),
            ),
        ];
        let mut lines = vec![
            Line::from(Span::styled("Tipo de instalação", bold())),
            Line::from(""),
        ];
        for (i, (name, detail)) in options.iter().enumerate() {
            let selected = i == self.profile_cursor;
            let marker = if selected { "› " } else { "  " };
            let style = if selected {
                bold().fg(Color::Cyan)
            } else {
                Style::default()
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{marker}{name:<12}"), style),
                Span::styled(detail.clone(), dim()),
            ]));
            lines.push(Line::from(""));
        }
        let not_rec: Vec<&str> = self
            .index
            .components
            .iter()
            .filter(|c| !c.recommended)
            .map(|c| c.label.as_str())
            .collect();
        if !not_rec.is_empty() {
            lines.push(Line::from(Span::styled(
                format!(
                    "Fora da recomendada: {}. Para instalá-los, escolha a Avançada.",
                    not_rec.join(", ")
                ),
                dim(),
            )));
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
    }

    fn draw_components(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![
            Line::from(Span::styled("Componentes", bold())),
            Line::from(""),
            Line::from(vec![
                Span::styled("[x] Solar", dim()),
                Span::styled("  a linha de comando, sempre instalada", dim()),
            ]),
        ];
        for (i, c) in self.index.components.iter().enumerate() {
            let on = self.selection.contains(&c.name);
            let cursor = i == self.component_cursor;
            let style = if cursor {
                bold().fg(Color::Cyan)
            } else {
                Style::default()
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} {:<16}", if on { "[x]" } else { "[ ]" }, c.label),
                    style,
                ),
                Span::styled(
                    format!("{:>9}  ", crate::mib(self.index.component_size(&c.name))),
                    dim(),
                ),
                Span::styled(c.description.clone(), dim()),
            ]));
            if c.name == solar_core::component::VERILATOR {
                let note = match &self.compiler {
                    Some(sc) => format!("      compilador do sistema encontrado: {}", sc.cxx),
                    None => "      compilador do sistema não encontrado: o Verilator não roda sem g++/clang++, make e Perl".into(),
                };
                let color = if self.compiler.is_some() {
                    Color::DarkGray
                } else {
                    Color::Yellow
                };
                lines.push(Line::from(Span::styled(note, Style::default().fg(color))));
            }
            if !c.requires.is_empty() {
                let reqs: Vec<String> = c.requires.iter().map(|r| self.label(r)).collect();
                lines.push(Line::from(Span::styled(
                    format!("      precisa de {}", reqs.join(", ")),
                    dim(),
                )));
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::raw("Total: "),
            Span::styled(crate::mib(self.index.size_of(&self.selection)), bold()),
        ]));
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn draw_destination(&self, frame: &mut Frame, area: Rect) {
        let [title, input, link, notes] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(1),
        ])
        .areas(area);
        frame.render_widget(
            Paragraph::new(Span::styled("Pasta da instalação", bold())),
            title,
        );
        let input_style = if self.focus_link {
            dim()
        } else {
            Style::default().fg(Color::Cyan)
        };
        let cursor = if self.focus_link { "" } else { "▏" };
        frame.render_widget(
            Paragraph::new(format!("{}{cursor}", self.prefix)).block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(input_style),
            ),
            input,
        );
        if let Some(path) = &self.link_path {
            let style = if self.focus_link {
                bold().fg(Color::Cyan)
            } else {
                Style::default()
            };
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        format!("{} criar o atalho ", if self.link { "[x]" } else { "[ ]" }),
                        style,
                    ),
                    Span::styled(path.to_string_lossy().into_owned(), dim()),
                ])),
                link,
            );
        }
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "Uma pasta nova, vazia ou com uma instalação do Solar. Evite espaços no caminho: o make do Verilator não os aceita.",
                    dim(),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("Espaço necessário: {}", crate::mib(self.index.size_of(&self.selection))),
                    dim(),
                )),
            ])
            .wrap(Wrap { trim: true }),
            notes,
        );
    }

    fn draw_summary(&self, frame: &mut Frame, area: Rect) {
        let target = self.target();
        let mut lines = vec![
            Line::from(Span::styled("Pronto para instalar", bold())),
            Line::from(""),
            kv(
                "Tipo",
                match self.profile {
                    Profile::Recommended => "Recomendada",
                    Profile::Advanced => "Avançada",
                },
            ),
            kv(
                "Componentes",
                &format!("Solar, {}", self.selection_labels()),
            ),
            kv("Pasta", &target.prefix.to_string_lossy()),
            kv(
                "Atalho",
                &target
                    .link
                    .as_ref()
                    .map_or("nenhum".into(), |l| l.to_string_lossy().into_owned()),
            ),
            kv("Espaço", &crate::mib(self.index.size_of(&self.selection))),
        ];
        if let Some(r) = &self.existing {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!(
                    "Substitui o Solar {} instalado aí ({}).",
                    r.solar_version,
                    r.components.join(", ")
                ),
                Style::default().fg(Color::Yellow),
            )));
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
    }

    fn draw_installing(&self, frame: &mut Frame, area: Rect) {
        let [title, gauge, label] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .areas(area);
        frame.render_widget(Paragraph::new(Span::styled("Instalando", bold())), title);
        let ratio = if self.progress.total == 0 {
            0.0
        } else {
            (self.progress.done as f64 / self.progress.total as f64).min(1.0)
        };
        frame.render_widget(
            Gauge::default()
                .block(Block::bordered().border_type(BorderType::Rounded))
                .gauge_style(Style::default().fg(Color::Cyan))
                .ratio(ratio)
                .label(format!(
                    "{} de {}",
                    crate::mib(self.progress.done.min(self.progress.total)),
                    crate::mib(self.progress.total)
                )),
            gauge,
        );
        frame.render_widget(
            Paragraph::new(Span::styled(self.progress.label.clone(), dim())),
            label,
        );
    }

    fn draw_done(&self, frame: &mut Frame, area: Rect) {
        let lines = match &self.outcome {
            Some(Ok(report)) => report_lines(report, &self.index),
            Some(Err(e)) => vec![
                Line::from(Span::styled("A instalação falhou", bold().fg(Color::Red))),
                Line::from(""),
                Line::from(e.clone()),
                Line::from(""),
                Line::from("A instalação anterior, se havia, não foi mexida."),
            ],
            None => vec![],
        };
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
    }
}

/// As linhas do fim de uma instalação bem-sucedida (também impressas depois
/// que a TUI fecha).
pub fn report_lines(report: &Report, index: &Index) -> Vec<Line<'static>> {
    let labels: Vec<String> = index
        .components
        .iter()
        .filter(|c| report.components.contains(&c.name))
        .map(|c| c.label.clone())
        .collect();
    let mut lines = vec![
        Line::from(Span::styled(
            format!(
                "Solar {} instalado em {}",
                index.solar_version,
                report.prefix.display()
            ),
            bold().fg(Color::Green),
        )),
        Line::from(""),
        kv(
            "Componentes",
            &if labels.is_empty() {
                "nenhum".into()
            } else {
                labels.join(", ")
            },
        ),
        kv(
            "Conferidos",
            &format!("{} executáveis batem com o manifesto", report.verified),
        ),
    ];
    match &report.link {
        LinkOutcome::Created(link) => {
            lines.push(kv("Atalho", &link.to_string_lossy()));
            if !report.link_on_path() {
                let dir = link
                    .parent()
                    .map(|d| d.display().to_string())
                    .unwrap_or_default();
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("{dir} não está no PATH. No bash ou no zsh, acrescente ao ~/.bashrc ou ~/.zshrc:"),
                    Style::default().fg(Color::Yellow),
                )));
                lines.push(Line::from(format!("  export PATH=\"{dir}:$PATH\"")));
                lines.push(Line::from(Span::styled(
                    "No fish, rode uma vez:",
                    Style::default().fg(Color::Yellow),
                )));
                lines.push(Line::from(format!("  fish_add_path {dir}")));
            }
        }
        LinkOutcome::Blocked(link) => lines.push(Line::from(Span::styled(
            format!(
                "Atalho não criado: {} já existe e não é um atalho.",
                link.display()
            ),
            Style::default().fg(Color::Yellow),
        ))),
        LinkOutcome::NotRequested => lines.push(kv("Executável", &report.solar.to_string_lossy())),
    }
    lines.push(Line::from(""));
    lines.push(Line::from("Para conferir: solar tools --verify"));
    lines.push(Line::from(format!(
        "Para remover: {}",
        report.prefix.join(install::UNINSTALL_SCRIPT).display()
    )));
    lines
}

fn kv(key: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:<13}"), dim()),
        Span::raw(value.to_owned()),
    ])
}

fn bold() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// Roda a instalação guiada no terminal. Devolve o estado final (com o
/// resultado, se chegou a instalar).
pub fn run(app: App) -> anyhow::Result<App> {
    let mut terminal = ratatui::try_init()?;
    let result = event_loop(&mut terminal, app);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, mut app: App) -> anyhow::Result<App> {
    loop {
        app.poll();
        terminal.draw(|f| app.draw(f))?;
        if event::poll(Duration::from_millis(50))?
            && let TermEvent::Key(key) = event::read()?
        {
            match app.handle(key) {
                Command::Quit => return Ok(app),
                Command::Install => app.start_install(),
                Command::None => {}
            }
        }
    }
}

/// Converte as linhas em texto simples (para imprimir fora da TUI).
pub fn plain(lines: &[Line]) -> String {
    Text::from(lines.to_vec())
        .lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn app() -> App {
        // Absoluto em qualquer sistema (no Windows, `/nao/existe` não é: falta
        // o drive) e inexistente, para a tela de destino aceitar.
        let base = std::env::temp_dir().join(format!("solar-tui-{}", std::process::id()));
        let target = Target {
            prefix: base.join("solar"),
            link: Some(base.join("bin").join("solar")),
        };
        App::new(crate::plan::tests::index(), base, target, None)
    }

    fn screen_text(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 32)).unwrap();
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

    #[test]
    fn recommended_path_skips_the_component_list() {
        let mut app = app();
        assert!(screen_text(&app).contains("Bem-vindo"));
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Profile);
        let text = screen_text(&app);
        assert!(
            text.contains("Recomendada") && text.contains("Avançada"),
            "{text}"
        );
        assert!(text.contains("Fora da recomendada: VERILATOR"), "{text}");
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Destination);
        assert_eq!(app.profile, Profile::Recommended);
        assert_eq!(app.selection, crate::plan::recommended(&app.index));
        assert_eq!(app.handle(key(KeyCode::Enter)), Command::None);
        assert_eq!(app.screen, Screen::Summary);
        assert!(screen_text(&app).contains("Recomendada"));
        assert_eq!(app.handle(key(KeyCode::Enter)), Command::Install);
    }

    #[test]
    fn advanced_path_toggles_components_with_their_requirements() {
        let mut app = app();
        app.handle(key(KeyCode::Enter));
        app.handle(key(KeyCode::Down));
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Components);
        assert!(screen_text(&app).contains("compilador do sistema não encontrado"));
        // Cursor no Yosys (4º): desmarcar leva o Graphviz junto.
        for _ in 0..3 {
            app.handle(key(KeyCode::Down));
        }
        app.handle(key(KeyCode::Char(' ')));
        assert!(!app.selection.contains("yosys") && !app.selection.contains("graphviz"));
        // Verilator (3º) entra.
        app.handle(key(KeyCode::Up));
        app.handle(key(KeyCode::Char(' ')));
        assert!(app.selection.contains("verilator"));
        let text = screen_text(&app);
        assert!(
            text.contains("[x] VERILATOR") && text.contains("[ ] YOSYS"),
            "{text}"
        );
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Destination);
        app.handle(key(KeyCode::Esc));
        assert_eq!(
            app.screen,
            Screen::Components,
            "Esc volta para a lista na avançada"
        );
    }

    #[test]
    fn destination_is_edited_and_validated() {
        let mut app = app();
        app.screen = Screen::Destination;
        for _ in 0..app.prefix.chars().count() {
            app.handle(key(KeyCode::Backspace));
        }
        for c in "relativo".chars() {
            app.handle(key(KeyCode::Char(c)));
        }
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Destination);
        assert!(screen_text(&app).contains("caminho absoluto"));
        // Tab passa para o atalho; Espaço o desliga.
        app.handle(key(KeyCode::Tab));
        app.handle(key(KeyCode::Char(' ')));
        assert!(!app.link);
        assert_eq!(app.target().link, None);
        assert_eq!(app.prefix, "relativo");
    }
}
