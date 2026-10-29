//! A instalação guiada no terminal.
//!
//! Telas, nesta ordem: boas-vindas, perfil (um dos do índice, ou Custom),
//! componentes (só na Custom), destino, resumo, progresso e fim. `Esc` volta uma tela; `Ctrl+C` sai (menos durante a extração).
//!
//! Para acrescentar aplicativos do bundle a uma instalação
//! ([`App::for_adding`], o que `lace install` abre), a TUI começa na lista,
//! com os instalados marcados e travados, e vai direto ao resumo. Só os
//! aplicativos novos são baixados e extraídos ([`crate::add`]); o `lace`
//! não é reinstalado.
//!
//! [`App`] guarda o estado e trata as teclas ([`App::handle`]); o desenho é
//! [`App::draw`]. Os dois são separados para testar as telas sem terminal.

use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use lace_core::SystemCompiler;
use ratatui::crossterm::event::{
    self, Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Gauge, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use crate::add::{self, AddReport, ChunkSource};
use crate::brand;
use crate::install::{self, Event, LinkOutcome, Receipt, Report, Target};
use crate::payload::Index;
use crate::plan::{self, Profile, Selection};

/// As telas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Boas-vindas.
    Welcome,
    /// Um perfil, ou Custom.
    Profile,
    /// A lista de componentes (instalação Custom).
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

/// Como terminou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finished {
    /// Uma instalação do Lace.
    Installed(Report),
    /// Aplicativos acrescentados a uma instalação.
    Added(AddReport),
}

enum Msg {
    Progress(Event),
    // Na Box: o Report é grande (no Windows, bem maior que o Progress).
    Finished(Box<Result<Finished, String>>),
}

#[derive(Debug, Default, Clone)]
struct Progress {
    total: u64,
    done: u64,
    label: String,
    /// O pedaço atual, sem o que está sendo baixado.
    chunk: String,
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
    /// Acrescentando a uma instalação: os componentes que ela já tem, que
    /// não podem ser desmarcados.
    adding: Option<Selection>,
    /// Acrescentando: de onde vêm os pedaços.
    source: Option<Arc<dyn ChunkSource>>,
    compiler: Option<SystemCompiler>,
    error: Option<String>,
    progress: Progress,
    /// O resultado, quando terminar.
    pub outcome: Option<Result<Finished, String>>,
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
        let selection = plan::default_selection(&index);
        let profile = Profile::Preset(plan::default_profile(&index));
        App {
            screen: Screen::Welcome,
            index,
            payload_dir,
            profile,
            profile_cursor: 0,
            selection,
            component_cursor: 0,
            prefix: target.prefix.to_string_lossy().into_owned(),
            link: target.link.is_some(),
            link_path: target.link,
            focus_link: false,
            existing,
            adding: None,
            source: None,
            compiler,
            error: None,
            progress: Progress::default(),
            outcome: None,
            rx: None,
        }
    }

    /// Para acrescentar aplicativos do bundle à instalação em `prefix`, que
    /// tem `installed`: começa na lista, com os instalados marcados e
    /// travados. Os pedaços vêm de `source`.
    pub fn for_adding(
        index: Index,
        source: Arc<dyn ChunkSource>,
        prefix: PathBuf,
        compiler: Option<SystemCompiler>,
        installed: Selection,
    ) -> App {
        let installed: Selection = installed
            .into_iter()
            .filter(|c| index.component(c).is_some())
            .collect();
        let target = Target { prefix, link: None };
        let mut app = App::new(index, PathBuf::new(), target, compiler);
        app.existing = None;
        app.screen = Screen::Components;
        app.profile = Profile::Custom;
        app.selection = installed.clone();
        app.adding = Some(installed);
        app.source = Some(source);
        app
    }

    /// Os componentes marcados que a instalação ainda não tem.
    fn new_components(&self) -> Selection {
        match &self.adding {
            Some(installed) => self.selection.difference(installed).cloned().collect(),
            None => self.selection.clone(),
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
                self.error = Some("Please wait: the extraction cannot be interrupted".into());
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
            Screen::Profile => {
                let profiles = self.index.profiles();
                // Os perfis e, por último, o Custom.
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.profile_cursor = self.profile_cursor.saturating_sub(1)
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.profile_cursor = (self.profile_cursor + 1).min(profiles.len())
                    }
                    KeyCode::Enter => match profiles.get(self.profile_cursor) {
                        Some(p) => {
                            self.profile = Profile::Preset(p.name.clone());
                            // Numa instalação que já existe, o que ela tem fica.
                            let installed = self
                                .existing
                                .as_ref()
                                .map_or(&[][..], |r| r.components.as_slice());
                            self.selection = plan::profile_keeping(&self.index, &p.name, installed)
                                .unwrap_or_default();
                            self.screen = Screen::Destination;
                        }
                        None => {
                            self.profile = Profile::Custom;
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
                    },
                    KeyCode::Esc => self.screen = Screen::Welcome,
                    KeyCode::Char('q') => return Command::Quit,
                    _ => {}
                }
            }
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
                        match &self.adding {
                            Some(installed) if installed.contains(&name) => {
                                self.error = Some(format!(
                                    "{} is already installed; here you can only add components",
                                    self.label(&name)
                                ));
                            }
                            Some(installed) => {
                                let installed = installed.clone();
                                plan::toggle(&self.index, &mut self.selection, &name);
                                self.selection.extend(installed);
                            }
                            None => plan::toggle(&self.index, &mut self.selection, &name),
                        }
                    }
                    KeyCode::Enter if self.adding.is_some() => {
                        if self.new_components().is_empty() {
                            self.error = Some(
                                "Mark a component that is not installed yet with Space".into(),
                            );
                        } else {
                            self.screen = Screen::Summary;
                        }
                    }
                    KeyCode::Enter => self.screen = Screen::Destination,
                    KeyCode::Esc if self.adding.is_some() => return Command::Quit,
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
                        Profile::Custom => Screen::Components,
                        Profile::Preset(_) => Screen::Profile,
                    }
                }
                _ => {}
            },
            Screen::Summary => match key.code {
                KeyCode::Enter => {
                    self.screen = Screen::Installing;
                    return Command::Install;
                }
                KeyCode::Esc if self.adding.is_some() => self.screen = Screen::Components,
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
        let index = self.index.clone();
        let target = self.target();
        if let Some(source) = self.source.clone() {
            let new = self.new_components();
            std::thread::spawn(move || {
                let progress = tx.clone();
                let result = add::add(&*source, &index, &target.prefix, &new, |e| {
                    let _ = progress.send(Msg::Progress(e));
                });
                let _ = tx.send(Msg::Finished(Box::new(
                    result.map(Finished::Added).map_err(|e| format!("{e:#}")),
                )));
            });
        } else {
            let payload_dir = self.payload_dir.clone();
            let selection = self.selection.clone();
            std::thread::spawn(move || {
                let progress = tx.clone();
                let result = install::install(&payload_dir, &index, &selection, &target, |e| {
                    let _ = progress.send(Msg::Progress(e));
                });
                let _ = tx.send(Msg::Finished(Box::new(
                    result
                        .map(Finished::Installed)
                        .map_err(|e| format!("{e:#}")),
                )));
            });
        }
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
                    ..
                }) => {
                    let what = if components.is_empty() {
                        "Lace".to_owned()
                    } else {
                        components
                            .iter()
                            .map(|c| self.label(c))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    self.progress.chunk = format!("[{index}/{count}] {what}");
                    self.progress.label = self.progress.chunk.clone();
                }
                Msg::Progress(Event::Downloading { bytes }) => {
                    self.progress.label = format!(
                        "{}: downloading, {}",
                        self.progress.chunk,
                        crate::mib(bytes)
                    );
                }
                Msg::Progress(Event::Progress { done }) => {
                    self.progress.done = done;
                    self.progress.label = self.progress.chunk.clone();
                }
                Msg::Progress(Event::Replacing { files }) => {
                    self.progress.label = format!("Replacing {files} files")
                }
                Msg::Progress(Event::Verifying) => {
                    self.progress.label = "Verifying the executables".into()
                }
                // Os eventos do `lace update` não passam por aqui.
                Msg::Progress(_) => {}
                Msg::Finished(result) => {
                    self.outcome = Some(*result);
                    self.screen = Screen::Done;
                }
            }
        }
    }

    /// O nome do perfil escolhido, para o resumo.
    fn profile_label(&self) -> String {
        match &self.profile {
            Profile::Custom => "Custom".to_owned(),
            Profile::Preset(name) => self
                .index
                .profile(name)
                .map_or_else(|| name.clone(), |p| p.label),
        }
    }

    fn label(&self, name: &str) -> String {
        self.index
            .component(name)
            .map_or_else(|| name.to_owned(), |c| c.label.clone())
    }

    fn selection_labels(&self) -> String {
        self.labels(&self.selection)
    }

    fn labels(&self, selection: &Selection) -> String {
        let labels: Vec<String> = self
            .index
            .components
            .iter()
            .filter(|c| selection.contains(&c.name))
            .map(|c| c.label.clone())
            .collect();
        if labels.is_empty() {
            "none".into()
        } else {
            labels.join(", ")
        }
    }

    /// Desenha a tela atual.
    pub fn draw(&self, frame: &mut Frame) {
        let area = centered(frame.area(), 92, 30);
        let title = if self.adding.is_some() {
            format!(
                " Lace · apps from bundle {} ({}) ",
                self.index.bundle, self.index.platform
            )
        } else {
            format!(
                " Lace {} · installer ({}) ",
                self.index.lace_version, self.index.platform
            )
        };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .title(Span::styled(title, bold().fg(brand::accent())));
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
            Screen::Welcome => "Enter continue   Esc quit",
            Screen::Profile => "↑↓ choose   Enter continue   Esc back   q quit",
            Screen::Components if self.adding.is_some() => {
                "↑↓ move   Space toggle   Enter continue   Esc quit"
            }
            Screen::Components => "↑↓ move   Space toggle   Enter continue   Esc back",
            Screen::Destination => "Type the folder   Tab link   Enter continue   Esc back",
            Screen::Summary => "Enter install   Esc back   q quit",
            Screen::Installing => "Extracting...",
            Screen::Done => "Enter quit",
        };
        frame.render_widget(
            Paragraph::new(Span::styled(hint, Style::default().fg(Color::DarkGray))),
            keys.inner(ratatui::layout::Margin::new(2, 0)),
        );
    }

    fn draw_welcome(&self, frame: &mut Frame, area: Rect) {
        let labels: Vec<&str> = self
            .index
            .components
            .iter()
            .map(|c| c.label.as_str())
            .collect();
        let tools = match labels.as_slice() {
            [] => String::new(),
            [one] => (*one).to_owned(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        };
        // A marca, com o nome ao lado, na altura dos fios.
        let mark = brand::mark(MARK_COLS);
        let beside = [
            Span::raw(""),
            Span::styled("Lace", bold().fg(brand::cream())),
            Span::styled("NIPS-CERN · UFJF", dim()),
        ];
        let mut lines: Vec<Line> = mark
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let pad = MARK_COLS.saturating_sub(row.chars().count());
                let mut spans = vec![Span::styled(
                    format!("{row}{}   ", " ".repeat(pad)),
                    Style::default().fg(brand::accent()),
                )];
                spans.extend(beside.get(i).cloned());
                Line::from(spans)
            })
            .collect();
        lines.extend([
            Line::from(""),
            Line::from(Span::styled("Welcome to the Lace installer", bold())),
            Line::from(""),
            Line::from(format!(
                "Lace drives the SAPHO development tools. This installer has: {tools}."
            )),
            Line::from(""),
            Line::from(format!(
                "It only uses the tools of the bundle installed with it, at the exact versions \
                 of bundle {}. No system tool is used, with one exception: Verilator builds \
                 with the system C++ compiler, make and Perl.",
                self.index.bundle
            )),
            Line::from(""),
            Line::from("Next: choose a profile and the folder, then confirm."),
        ]);
        if let Some(r) = &self.existing {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!(
                    "There is already a Lace {} installation in {} ({}). It will be replaced \
                     by Lace {}; with a profile, the apps it has stay.",
                    r.lace_version,
                    self.prefix,
                    r.components.join(", "),
                    self.index.lace_version
                ),
                Style::default().fg(Color::Yellow),
            )));
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
    }

    fn draw_profile(&self, frame: &mut Frame, area: Rect) {
        let profiles = self.index.profiles();
        let mut lines = vec![
            Line::from(Span::styled("Installation profile", bold())),
            Line::from(""),
        ];
        // A lista: um perfil por linha, com o tamanho, e o Custom.
        for i in 0..=profiles.len() {
            let selected = i == self.profile_cursor;
            let marker = if selected { "› " } else { "  " };
            let style = if selected {
                bold().fg(brand::accent())
            } else {
                Style::default()
            };
            let (name, detail) = match profiles.get(i) {
                Some(p) => {
                    let selection = plan::profile(&self.index, &p.name).unwrap_or_default();
                    // O padrão é o recomendado, e diz isso no nome.
                    let name = if i == 0 {
                        format!("{} (Recommended)", p.label)
                    } else {
                        p.label.clone()
                    };
                    (
                        name,
                        format!(
                            "{} ({})",
                            p.description,
                            crate::mib(self.index.size_of(&selection))
                        ),
                    )
                }
                None => (
                    "Custom (advanced)".to_owned(),
                    "For users who know which components they need".to_owned(),
                ),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{marker}{name:<22}"), style),
                Span::styled(detail, dim()),
            ]));
        }
        lines.push(Line::from(""));
        // O perfil sob o cursor: para quem é, e todos os aplicativos, os que
        // ele instala marcados e os que deixa de fora, não.
        match profiles.get(self.profile_cursor) {
            Some(p) => {
                let selection = plan::profile(&self.index, &p.name).unwrap_or_default();
                if !p.audience.is_empty() {
                    let text = format!("For {}.", crate::lowercase_first(&p.audience));
                    lines.extend(
                        crate::wrap(&text, area.width as usize)
                            .into_iter()
                            .map(Line::from),
                    );
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(Span::styled(
                    format!(
                        "{} installs {LACE_LABEL} and {} of the {} apps:",
                        p.label,
                        selection.len(),
                        self.index.components.len()
                    ),
                    bold(),
                )));
                lines.extend(self.app_lines(&selection, area.width));
            }
            None => {
                let text = format!(
                    "The Custom installation is for advanced use, for users who know which \
                     components they need. The next screen lists every app; Space marks and \
                     unmarks. {LACE_LABEL} is always installed."
                );
                lines.extend(
                    crate::wrap(&text, area.width as usize)
                        .into_iter()
                        .map(|l| Line::from(Span::styled(l, dim()))),
                );
            }
        }
        // Sem quebrar as linhas da lista: cada aplicativo numa linha só.
        frame.render_widget(Paragraph::new(lines), area);
    }

    /// Todos os aplicativos, um por linha, marcados se estão em `selection`;
    /// a descrição é cortada para caber em `width`.
    fn app_lines(&self, selection: &Selection, width: u16) -> Vec<Line<'static>> {
        let label_width = self
            .index
            .components
            .iter()
            .map(|c| c.label.chars().count())
            .max()
            .unwrap_or(0)
            .max(LACE_LABEL.len());
        let room = (width as usize).saturating_sub(label_width + 8);
        let row = |on: bool, label: &str, description: &str| {
            let mark = if on { "[x]" } else { "[ ]" };
            let description: String = if description.chars().count() > room {
                let mut cut: String = description.chars().take(room.saturating_sub(1)).collect();
                cut.push('…');
                cut
            } else {
                description.to_owned()
            };
            let style = if on { Style::default() } else { dim() };
            Line::from(vec![
                Span::styled(format!("  {mark} {label:<label_width$}  "), style),
                Span::styled(description, dim()),
            ])
        };
        std::iter::once(row(true, LACE_LABEL, "Always installed"))
            .chain(
                self.index
                    .components
                    .iter()
                    .map(|c| row(selection.contains(&c.name), &c.label, &c.description)),
            )
            .collect()
    }

    fn draw_components(&self, frame: &mut Frame, area: Rect) {
        let mut lines = match &self.adding {
            Some(_) => vec![
                Line::from(Span::styled("Bundle apps", bold())),
                Line::from(Span::styled(
                    "Installed apps stay. Mark the ones you want to install; only those are downloaded.",
                    dim(),
                )),
                Line::from(""),
            ],
            None => vec![
                Line::from(Span::styled("Components", bold())),
                Line::from(""),
                Line::from(vec![
                    Span::styled(format!("[x] {LACE_LABEL}"), dim()),
                    Span::styled("  Always installed, with any profile", dim()),
                ]),
            ],
        };
        for (i, c) in self.index.components.iter().enumerate() {
            let on = self.selection.contains(&c.name);
            let installed = self.adding.as_ref().is_some_and(|s| s.contains(&c.name));
            let cursor = i == self.component_cursor;
            let style = if cursor {
                bold().fg(brand::accent())
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
                if installed {
                    Span::styled("Installed", Style::default().fg(Color::Green))
                } else {
                    Span::styled(c.description.clone(), dim())
                },
            ]));
            // No Windows o compilador vem com o Verilator (a descrição diz);
            // nos outros, ele vem do sistema, e a nota diz se foi achado.
            if c.name == lace_core::component::VERILATOR && self.index.platform != "windows-x64" {
                let note = match &self.compiler {
                    Some(sc) => format!("      System compiler found: {}", sc.cxx),
                    None => "      System compiler not found: Verilator does not run without g++/clang++, make and Perl".into(),
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
                    format!("      Requires {}", reqs.join(", ")),
                    dim(),
                )));
            }
        }
        lines.push(Line::from(""));
        let total = match &self.adding {
            Some(_) => vec![
                Span::raw("To install: "),
                Span::styled(
                    crate::mib(self.index.chunks_size_of(&self.new_components())),
                    bold(),
                ),
            ],
            None => vec![
                Span::raw("Total: "),
                Span::styled(crate::mib(self.index.size_of(&self.selection)), bold()),
            ],
        };
        lines.push(Line::from(total));
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
            Paragraph::new(Span::styled("Installation folder", bold())),
            title,
        );
        let input_style = if self.focus_link {
            dim()
        } else {
            Style::default().fg(brand::accent())
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
                bold().fg(brand::accent())
            } else {
                Style::default()
            };
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        format!("{} Create the link ", if self.link { "[x]" } else { "[ ]" }),
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
                    "A new folder, an empty one, or one with a Lace installation. Avoid spaces in the path: Verilator's make does not accept them.",
                    dim(),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("Disk space required: {}", crate::mib(self.index.size_of(&self.selection))),
                    dim(),
                )),
            ])
            .wrap(Wrap { trim: true }),
            notes,
        );
    }

    fn draw_summary(&self, frame: &mut Frame, area: Rect) {
        let target = self.target();
        if let Some(installed) = &self.adding {
            let lines = vec![
                Line::from(Span::styled("Ready to install", bold())),
                Line::from(""),
                kv("Install", &self.labels(&self.new_components())),
                kv("Installed", &self.labels(installed)),
                kv("Folder", &target.prefix.join("toolchain").to_string_lossy()),
                kv(
                    "Disk space",
                    &crate::mib(self.index.chunks_size_of(&self.new_components())),
                ),
                Line::from(""),
                Line::from(Span::styled(
                    "Only the marked apps are downloaded and extracted; Lace itself does not change.",
                    dim(),
                )),
            ];
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
            return;
        }
        let mut lines = vec![
            Line::from(Span::styled("Ready to install", bold())),
            Line::from(""),
            kv("Profile", &self.profile_label()),
            kv(
                "Components",
                &format!("{LACE_LABEL}, {}", self.selection_labels()),
            ),
            kv("Folder", &target.prefix.to_string_lossy()),
            kv(
                "Link",
                &target
                    .link
                    .as_ref()
                    .map_or("none".into(), |l| l.to_string_lossy().into_owned()),
            ),
            kv(
                "Disk space",
                &crate::mib(self.index.size_of(&self.selection)),
            ),
        ];
        if let Some(r) = &self.existing {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!(
                    "Replaces the Lace {} installed there ({}).",
                    r.lace_version,
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
        frame.render_widget(Paragraph::new(Span::styled("Installing", bold())), title);
        let ratio = if self.progress.total == 0 {
            0.0
        } else {
            (self.progress.done as f64 / self.progress.total as f64).min(1.0)
        };
        frame.render_widget(
            Gauge::default()
                .block(Block::bordered().border_type(BorderType::Rounded))
                .gauge_style(Style::default().fg(brand::accent()))
                .ratio(ratio)
                .label(format!(
                    "{} of {}",
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
            Some(Ok(Finished::Installed(report))) => report_lines(report, &self.index),
            Some(Ok(Finished::Added(report))) => added_lines(report, &self.index),
            Some(Err(e)) => vec![
                Line::from(Span::styled("Installation failed", bold().fg(Color::Red))),
                Line::from(""),
                Line::from(e.clone()),
                Line::from(""),
                Line::from(if self.adding.is_some() {
                    "Nothing was added: the installation is unchanged."
                } else {
                    "The previous installation, if any, was not touched."
                }),
            ],
            None => vec![],
        };
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
    }
}

/// As linhas do fim de aplicativos acrescentados (também impressas depois que
/// a TUI fecha).
pub fn added_lines(report: &AddReport, index: &Index) -> Vec<Line<'static>> {
    let labels = |names: &[String]| -> String {
        let labels: Vec<String> = index
            .components
            .iter()
            .filter(|c| names.contains(&c.name))
            .map(|c| c.label.clone())
            .collect();
        if labels.is_empty() {
            "none".into()
        } else {
            labels.join(", ")
        }
    };
    vec![
        Line::from(Span::styled(
            format!("Installed: {}", labels(&report.added)),
            bold().fg(Color::Green),
        )),
        Line::from(""),
        kv("In bundle", &labels(&report.components)),
        kv("Folder", &report.toolchain.to_string_lossy()),
        kv(
            "Verified",
            &format!("{} executables match the manifest", report.verified),
        ),
    ]
    .into_iter()
    .chain(studio_lines(
        report.shortcut.as_deref(),
        report
            .added
            .iter()
            .any(|c| c == lace_core::component::STUDIO),
    ))
    .chain([Line::from(""), Line::from("To check: lace tools")])
    .collect()
}

/// O atalho do Studio no menu e o aviso de WebView faltando, quando o Studio
/// foi instalado.
fn studio_lines(shortcut: Option<&Path>, installed: bool) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(shortcut) = shortcut {
        lines.push(kv("AURORA IDE", &shortcut.to_string_lossy()));
    }
    if installed && let Some(missing) = crate::desktop::missing_runtime() {
        lines.push(Line::from(Span::styled(
            missing.message(),
            Style::default().fg(Color::Yellow),
        )));
    }
    lines
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
                "Lace {} installed in {}",
                index.lace_version,
                report.prefix.display()
            ),
            bold().fg(Color::Green),
        )),
        Line::from(""),
        kv(
            "Components",
            &if labels.is_empty() {
                "none".into()
            } else {
                labels.join(", ")
            },
        ),
        kv(
            "Verified",
            &format!("{} executables match the manifest", report.verified),
        ),
    ];
    match &report.link {
        LinkOutcome::Created(link) => {
            lines.push(kv("Link", &link.to_string_lossy()));
            if !report.link_on_path() {
                let dir = link
                    .parent()
                    .map(|d| d.display().to_string())
                    .unwrap_or_default();
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!(
                        "{dir} is not on PATH. In bash or zsh, add this to ~/.bashrc or ~/.zshrc:"
                    ),
                    Style::default().fg(Color::Yellow),
                )));
                lines.push(Line::from(format!("  export PATH=\"{dir}:$PATH\"")));
                lines.push(Line::from(Span::styled(
                    "In fish, run once:",
                    Style::default().fg(Color::Yellow),
                )));
                lines.push(Line::from(format!("  fish_add_path {dir}")));
            }
        }
        LinkOutcome::Blocked(link) => lines.push(Line::from(Span::styled(
            format!(
                "Link not created: {} already exists and is not a link.",
                link.display()
            ),
            Style::default().fg(Color::Yellow),
        ))),
        LinkOutcome::NotRequested => lines.push(kv("Executable", &report.lace.to_string_lossy())),
    }
    lines.extend(studio_lines(
        report.shortcut.as_deref(),
        report
            .components
            .iter()
            .any(|c| c == lace_core::component::STUDIO),
    ));
    lines.push(Line::from(""));
    lines.push(Line::from("To verify: lace tools --verify"));
    lines.push(Line::from(format!(
        "To remove it, with the bundle: lace uninstall (or {})",
        report.prefix.join(install::UNINSTALL_SCRIPT).display()
    )));
    lines
}

/// O nome do Lace nas listas de aplicativos: sempre instalado, em qualquer
/// perfil.
const LACE_LABEL: &str = "Lace (CLI & Core)";

/// A largura da marca na tela de boas-vindas, em colunas.
const MARK_COLS: usize = 20;

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
        let base = std::env::temp_dir().join(format!("lace-tui-{}", std::process::id()));
        let target = Target {
            prefix: base.join("lace"),
            link: Some(base.join("bin").join("lace")),
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
    fn a_profile_skips_the_component_list() {
        let mut app = app();
        let text = screen_text(&app);
        assert!(text.contains("Welcome"), "{text}");
        assert!(
            text.chars().any(|c| ('\u{2801}'..='\u{28ff}').contains(&c)),
            "a marca em braille:\n{text}"
        );
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Profile);
        // Os perfis do índice e o Custom; o do cursor mostra o público e os
        // aplicativos.
        let text = screen_text(&app);
        assert!(
            text.contains("SAPHO") && text.contains("VERILOG") && text.contains("Custom"),
            "{text}"
        );
        assert!(text.contains("SAPHO (Recommended)"), "{text}");
        assert!(text.contains("Custom (advanced)"), "{text}");
        assert!(text.contains("For quem usa sapho."), "{text}");
        // Todos os aplicativos, um por linha, marcados pelo perfil.
        assert!(
            text.contains("SAPHO installs Lace (CLI & Core) and 5 of the 5 apps:"),
            "{text}"
        );
        assert!(text.contains("[x] Lace (CLI & Core)"), "{text}");
        assert!(
            text.contains("[x] YANC ") && text.contains("[x] VERILATOR "),
            "{text}"
        );
        app.handle(key(KeyCode::Down));
        let text = screen_text(&app);
        assert!(text.contains("For quem usa verilog."), "{text}");
        assert!(
            text.contains("VERILOG installs Lace (CLI & Core) and 3 of the 5 apps:"),
            "{text}"
        );
        assert!(
            text.contains("[ ] YANC ") && text.contains("[ ] VERILATOR "),
            "{text}"
        );
        assert!(
            text.contains("[x] ICARUS ") && text.contains("[x] YOSYS "),
            "{text}"
        );
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Destination);
        assert_eq!(app.profile, Profile::Preset("verilog".into()));
        assert_eq!(
            app.selection,
            crate::plan::profile(&app.index, "verilog").unwrap()
        );
        assert_eq!(app.handle(key(KeyCode::Enter)), Command::None);
        assert_eq!(app.screen, Screen::Summary);
        assert!(screen_text(&app).contains("VERILOG"));
        assert_eq!(app.handle(key(KeyCode::Enter)), Command::Install);
    }

    #[test]
    fn a_profile_over_an_installation_keeps_its_apps() {
        let mut app = app();
        app.existing = Some(Receipt {
            lace_version: "0.1.0".into(),
            bundle: "teste".into(),
            platform: "linux-x64".into(),
            components: vec!["yanc".into(), "saiu".into()],
            link: None,
        });
        let text = screen_text(&app);
        assert!(text.contains("the apps it has stay"), "{text}");
        app.handle(key(KeyCode::Enter));
        app.handle(key(KeyCode::Down));
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.profile, Profile::Preset("verilog".into()));
        let mut expected = crate::plan::profile(&app.index, "verilog").unwrap();
        expected.insert("yanc".into());
        assert_eq!(app.selection, expected);
    }

    #[test]
    fn custom_toggles_components_with_their_requirements() {
        let mut app = app();
        app.handle(key(KeyCode::Enter));
        // O Custom vem depois dos dois perfis; o cursor não passa dele.
        for _ in 0..5 {
            app.handle(key(KeyCode::Down));
        }
        let text = screen_text(&app);
        assert!(
            text.contains("lists every app") && text.contains("advanced use"),
            "{text}"
        );
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Components);
        assert_eq!(app.profile, Profile::Custom);
        assert!(screen_text(&app).contains("System compiler not found"));
        // Cursor no Yosys (4º): desmarcar leva o Graphviz junto.
        for _ in 0..3 {
            app.handle(key(KeyCode::Down));
        }
        app.handle(key(KeyCode::Char(' ')));
        assert!(!app.selection.contains("yosys") && !app.selection.contains("graphviz"));
        // O Verilator (3º), que o perfil padrão trazia, sai.
        app.handle(key(KeyCode::Up));
        app.handle(key(KeyCode::Char(' ')));
        assert!(!app.selection.contains("verilator"));
        let text = screen_text(&app);
        assert!(
            text.contains("[ ] VERILATOR") && text.contains("[ ] YOSYS"),
            "{text}"
        );
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Destination);
        app.handle(key(KeyCode::Esc));
        assert_eq!(
            app.screen,
            Screen::Components,
            "Esc volta para a lista no Custom"
        );
    }

    #[test]
    fn adding_starts_on_the_list_and_keeps_what_is_installed() {
        let prefix = std::env::temp_dir().join(format!("lace-tui-add-{}", std::process::id()));
        let source: Arc<dyn ChunkSource> = Arc::new(add::LocalPayload(prefix.clone()));
        let installed: Selection = ["icarus".to_owned(), "yanc".to_owned()].into();
        let new_app = || {
            App::for_adding(
                crate::plan::tests::index(),
                source.clone(),
                prefix.clone(),
                None,
                installed.clone(),
            )
        };
        let mut app = new_app();
        assert_eq!(app.screen, Screen::Components);
        let text = screen_text(&app);
        assert!(text.contains("apps from bundle teste"), "{text}");
        assert!(text.contains("Installed"), "{text}");
        assert!(
            !text.contains("[x] Lace"),
            "o Lace não é um aplicativo do bundle:\n{text}"
        );

        // Nada novo marcado: Enter não segue.
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Components);
        assert!(screen_text(&app).contains("not installed yet"));

        // O YANC (1º) está instalado: Espaço não o tira.
        app.handle(key(KeyCode::Char(' ')));
        assert!(app.selection.contains("yanc"));
        assert!(screen_text(&app).contains("is already installed"));

        // O Graphviz (5º) entra com o Yosys, que ele exige.
        for _ in 0..4 {
            app.handle(key(KeyCode::Down));
        }
        app.handle(key(KeyCode::Char(' ')));
        assert!(app.selection.contains("graphviz") && app.selection.contains("yosys"));
        // Tirá-lo de novo tira só o que é novo.
        app.handle(key(KeyCode::Char(' ')));
        assert!(app.selection.contains("yanc") && app.selection.contains("icarus"));
        app.handle(key(KeyCode::Char(' ')));
        assert_eq!(
            app.new_components(),
            ["graphviz".to_owned(), "yosys".to_owned()].into()
        );

        app.handle(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Summary, "sem a tela da pasta");
        let text = screen_text(&app);
        assert!(text.contains("Ready to install"), "{text}");
        assert!(
            text.contains("YOSYS") && text.contains("GRAPHVIZ"),
            "{text}"
        );
        assert!(text.contains("Lace itself does not change"), "{text}");
        app.handle(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Components);
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.handle(key(KeyCode::Enter)), Command::Install);
        assert_eq!(app.target().prefix, prefix);

        // Esc na lista sai: não há tela antes dela.
        assert_eq!(new_app().handle(key(KeyCode::Esc)), Command::Quit);
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
        assert!(screen_text(&app).contains("absolute path"));
        // Tab passa para o atalho; Espaço o desliga.
        app.handle(key(KeyCode::Tab));
        app.handle(key(KeyCode::Char(' ')));
        assert!(!app.link);
        assert_eq!(app.target().link, None);
        assert_eq!(app.prefix, "relativo");
    }
}
