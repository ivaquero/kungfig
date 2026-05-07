use std::env;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use crate::config::{Config, load_config_or_empty};
use crate::diff::diff_item;
use crate::doctor::run_doctor;
use crate::plan::{Action, build_plan, resolve_items};
use crate::state::{StateStore, collect_status};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Status,
    Plan,
    Diff,
    Doctor,
}

#[derive(Debug, Clone)]
pub struct Launch {
    pub manifest: PathBuf,
    pub initial_view: View,
    pub name: Option<String>,
    pub tag: Option<String>,
    pub diff_summary: bool,
}

#[derive(Debug, Clone)]
struct Entry {
    title: String,
    status: String,
    detail: Vec<String>,
    color: Color,
}

struct App {
    launch: Launch,
    view: View,
    entries: Vec<Entry>,
    selected: usize,
    scroll: u16,
    message: String,
}

impl App {
    fn new(launch: Launch) -> Self {
        Self {
            view: launch.initial_view,
            launch,
            entries: Vec::new(),
            selected: 0,
            scroll: 0,
            message: String::new(),
        }
    }

    fn refresh(&mut self) -> Result<()> {
        let entries = build_entries(&self.launch, self.view)?;
        self.entries = if entries.is_empty() {
            vec![Entry {
                title: "no items".to_string(),
                status: "empty".to_string(),
                detail: vec!["no matching items".to_string()],
                color: Color::DarkGray,
            }]
        } else {
            entries
        };
        self.selected = 0;
        self.scroll = 0;
        self.message = self.view.label().to_string();
        Ok(())
    }

    fn set_view(&mut self, view: View) -> Result<()> {
        self.view = view;
        self.refresh()
    }

    fn next(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.entries.len().saturating_sub(1));
        self.scroll = 0;
    }

    fn previous(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = self.selected.saturating_sub(1);
        self.scroll = 0;
    }

    fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    fn current_entry(&self) -> &Entry {
        &self.entries[self.selected.min(self.entries.len().saturating_sub(1))]
    }

    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<i32> {
        self.refresh()?;

        loop {
            terminal.draw(|frame| self.draw(frame))?;

            if event::poll(Duration::from_millis(200))? {
                let Event::Key(key) = event::read()? else {
                    continue;
                };

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(0),
                    KeyCode::Down | KeyCode::Char('j') => self.next(),
                    KeyCode::Up | KeyCode::Char('k') => self.previous(),
                    KeyCode::PageDown => self.scroll_down(),
                    KeyCode::PageUp => self.scroll_up(),
                    KeyCode::Char('1') => self.set_view(View::Status)?,
                    KeyCode::Char('2') => self.set_view(View::Plan)?,
                    KeyCode::Char('3') => self.set_view(View::Diff)?,
                    KeyCode::Char('4') => self.set_view(View::Doctor)?,
                    KeyCode::Char('r') => self.refresh()?,
                    _ => {}
                }
            }
        }
    }

    fn draw(&self, frame: &mut Frame<'_>) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(frame.area());

        self.draw_header(frame, layout[0]);

        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(36), Constraint::Min(0)])
            .split(layout[1]);

        self.draw_sidebar(frame, body[0]);
        self.draw_detail(frame, body[1]);
        self.draw_footer(frame, layout[2]);
    }

    fn draw_header(&self, frame: &mut Frame<'_>, area: Rect) {
        let title = Line::from(vec![
            Span::styled(
                "KungFig",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("view: {}", self.view.label()),
                Style::default().fg(Color::White),
            ),
            Span::raw("  "),
            Span::styled(
                self.launch.manifest.display().to_string(),
                Style::default().fg(Color::DarkGray),
            ),
        ]);

        let block =
            Paragraph::new(title).block(Block::default().borders(Borders::ALL).title("Dashboard"));
        frame.render_widget(block, area);
    }

    fn draw_sidebar(&self, frame: &mut Frame<'_>, area: Rect) {
        let items: Vec<ListItem<'_>> = self
            .entries
            .iter()
            .map(|entry| {
                let line = Line::from(vec![
                    Span::styled(
                        format!("{:<10}", entry.status),
                        Style::default()
                            .fg(entry.color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::raw(&entry.title),
                ]);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Items"))
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> ");

        let mut state = ListState::default();
        state.select(Some(
            self.selected.min(self.entries.len().saturating_sub(1)),
        ));
        frame.render_stateful_widget(list, area, &mut state);
    }

    fn draw_detail(&self, frame: &mut Frame<'_>, area: Rect) {
        let entry = self.current_entry();
        let mut lines = Vec::with_capacity(entry.detail.len() + 2);
        lines.push(Line::from(vec![Span::styled(
            &entry.title,
            Style::default()
                .fg(entry.color)
                .add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(vec![Span::styled(
            &entry.status,
            Style::default().fg(entry.color),
        )]));
        lines.extend(
            entry
                .detail
                .iter()
                .map(|line| Line::from(Span::raw(line.as_str()))),
        );

        let text = Text::from(lines);
        let paragraph = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title("Details"))
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0));
        frame.render_widget(paragraph, area);
    }

    fn draw_footer(&self, frame: &mut Frame<'_>, area: Rect) {
        let help = Line::from(vec![
            Span::styled("1", Style::default().fg(Color::Cyan)),
            Span::raw(" status  "),
            Span::styled("2", Style::default().fg(Color::Cyan)),
            Span::raw(" plan  "),
            Span::styled("3", Style::default().fg(Color::Cyan)),
            Span::raw(" diff  "),
            Span::styled("4", Style::default().fg(Color::Cyan)),
            Span::raw(" doctor  "),
            Span::styled("r", Style::default().fg(Color::Cyan)),
            Span::raw(" refresh  "),
            Span::styled("q", Style::default().fg(Color::Cyan)),
            Span::raw(" quit"),
        ]);

        let block = Paragraph::new(help).block(
            Block::default()
                .borders(Borders::ALL)
                .title(self.message.as_str()),
        );
        frame.render_widget(block, area);
    }
}

pub fn run_tui(launch: Launch) -> Result<i32> {
    if env::var_os("KUNGFIG_TUI_TEST_MODE").is_some() {
        let _ = load_config_or_empty(&launch.manifest)?;
        return Ok(0);
    }

    if !io::stdout().is_terminal() {
        bail!("`tui` requires an interactive terminal");
    }

    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("failed to enter alternate screen")?;

    let mut terminal =
        Terminal::new(CrosstermBackend::new(stdout)).context("failed to initialize terminal")?;
    let result = (|| -> Result<i32> {
        let mut app = App::new(launch);
        app.run(&mut terminal)
    })();

    restore_terminal(terminal.backend_mut())?;
    disable_raw_mode().context("failed to disable raw mode")?;
    terminal.show_cursor().context("failed to show cursor")?;

    result
}

fn restore_terminal<W: Write>(backend: &mut W) -> Result<()> {
    execute!(backend, LeaveAlternateScreen, DisableMouseCapture)
        .context("failed to restore terminal")?;
    Ok(())
}

fn build_entries(launch: &Launch, view: View) -> Result<Vec<Entry>> {
    match view {
        View::Status => build_status_entries(launch),
        View::Plan => build_plan_entries(launch),
        View::Diff => build_diff_entries(launch),
        View::Doctor => build_doctor_entries(launch),
    }
}

fn build_status_entries(launch: &Launch) -> Result<Vec<Entry>> {
    let (manifest_path, config) = load_config_or_empty(&launch.manifest)?;
    let selected = select_config(&config, launch.name.as_deref(), launch.tag.as_deref())?;
    if selected.items.is_empty() {
        return Ok(Vec::new());
    }

    let store = StateStore::open()?;
    let statuses = collect_status(&resolve_items(&selected, &manifest_path)?, &store)?;
    Ok(statuses
        .into_iter()
        .map(|status| Entry {
            title: status.name,
            status: status.status.clone(),
            detail: vec![status.detail],
            color: color_for_status(&status.status),
        })
        .collect())
}

fn build_plan_entries(launch: &Launch) -> Result<Vec<Entry>> {
    let (manifest_path, config) = load_config_or_empty(&launch.manifest)?;
    let selected = select_config(&config, launch.name.as_deref(), launch.tag.as_deref())?;
    if selected.items.is_empty() {
        return Ok(Vec::new());
    }

    let store = StateStore::open()?;
    let plan = build_plan(&selected, &manifest_path, Some(&store))?;
    Ok(plan
        .actions
        .into_iter()
        .map(|action| match action {
            Action::Create {
                name,
                source,
                target,
                mode,
            } => Entry {
                title: name,
                status: "create".to_string(),
                detail: vec![
                    format!("mode: {mode}"),
                    format!("source: {}", source.display()),
                    format!("target: {}", target.display()),
                ],
                color: Color::Green,
            },
            Action::Update {
                name,
                source,
                target,
                mode,
                change_state,
            } => Entry {
                title: name,
                status: "update".to_string(),
                detail: vec![
                    format!("mode: {mode}"),
                    format!("state: {change_state}"),
                    format!("source: {}", source.display()),
                    format!("target: {}", target.display()),
                ],
                color: color_for_change_state(change_state),
            },
            Action::Skip { name, reason } => Entry {
                title: name,
                status: "skip".to_string(),
                detail: vec![reason],
                color: Color::DarkGray,
            },
            Action::Conflict {
                name,
                target,
                reason,
            } => Entry {
                title: name,
                status: "conflict".to_string(),
                detail: vec![format!("target: {}", target.display()), reason],
                color: Color::Red,
            },
        })
        .collect())
}

fn build_diff_entries(launch: &Launch) -> Result<Vec<Entry>> {
    let (manifest_path, config) = load_config_or_empty(&launch.manifest)?;
    let selected = select_config(&config, launch.name.as_deref(), launch.tag.as_deref())?;
    if selected.items.is_empty() {
        return Ok(Vec::new());
    }

    if launch.diff_summary {
        let store = StateStore::open()?;
        let plan = build_plan(&selected, &manifest_path, Some(&store))?;
        return Ok(plan
            .actions
            .into_iter()
            .map(|action| match action {
                Action::Create { name, target, .. } => Entry {
                    title: name,
                    status: "create".to_string(),
                    detail: vec![format!("target: {}", target.display())],
                    color: Color::Green,
                },
                Action::Update {
                    name,
                    target,
                    change_state,
                    ..
                } => Entry {
                    title: name,
                    status: "update".to_string(),
                    detail: vec![
                        format!("target: {}", target.display()),
                        format!("state: {change_state}"),
                    ],
                    color: color_for_change_state(change_state),
                },
                Action::Skip { name, reason } => Entry {
                    title: name,
                    status: "skip".to_string(),
                    detail: vec![reason],
                    color: Color::DarkGray,
                },
                Action::Conflict {
                    name,
                    target,
                    reason,
                } => Entry {
                    title: name,
                    status: "conflict".to_string(),
                    detail: vec![format!("target: {}", target.display()), reason],
                    color: Color::Red,
                },
            })
            .collect());
    }

    let items = resolve_items(&selected, &manifest_path)?;
    let mut entries = Vec::with_capacity(items.len());
    for item in items {
        let detail = match diff_item(&item) {
            Ok(text) => text,
            Err(err) => format!("error: {err}"),
        };
        entries.push(Entry {
            title: item.name,
            status: "diff".to_string(),
            detail: detail.lines().map(|line| line.to_string()).collect(),
            color: Color::Blue,
        });
    }
    Ok(entries)
}

fn build_doctor_entries(launch: &Launch) -> Result<Vec<Entry>> {
    let checks = run_doctor(&launch.manifest)?;
    Ok(checks
        .into_iter()
        .map(|check| Entry {
            title: check.name,
            status: check.status.clone(),
            detail: vec![check.detail],
            color: color_for_status(&check.status),
        })
        .collect())
}

fn select_config(config: &Config, name: Option<&str>, tag: Option<&str>) -> Result<Config> {
    if let Some(name) = name {
        if !config.contains_identifier(name) {
            bail!("item `{name}` not found");
        }
    }

    config.filtered(name, tag)
}

fn color_for_status(status: &str) -> Color {
    match status {
        "ok" | "synced" | "created" | "updated" | "restored" => Color::Green,
        "pending" | "modified" | "skip" | "skipped" | "would-create" | "would-update" => {
            Color::Yellow
        }
        "conflict" | "error" => Color::Red,
        _ => Color::White,
    }
}

fn color_for_change_state(change_state: crate::state::ChangeState) -> Color {
    match change_state {
        crate::state::ChangeState::Clean => Color::Green,
        crate::state::ChangeState::Modified => Color::Yellow,
        crate::state::ChangeState::Conflict => Color::Red,
        crate::state::ChangeState::Untracked => Color::Blue,
    }
}

impl View {
    fn label(self) -> &'static str {
        match self {
            View::Status => "status",
            View::Plan => "plan",
            View::Diff => "diff",
            View::Doctor => "doctor",
        }
    }
}
