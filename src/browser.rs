use ansi_to_tui::IntoText;
use anyhow::Result;
use chrono::{Datelike, Local};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, List, ListState, Paragraph, Wrap};

use crate::clipboard;
use crate::editor::{self, Session};
use crate::store::Store;

const HELP: &str =
    " ↑↓ move · e edit · n new · d delete · y copy · / filter · PgUp/PgDn scroll · q quit ";

enum Mode {
    Normal,
    Filter,
    ConfirmDelete,
}

enum Action {
    None,
    Quit,
    Edit,
    New,
}

struct Browser {
    store: Store,
    /// Indices into `store.jots` that match `filter`, newest first.
    visible: Vec<usize>,
    list: ListState,
    filter: String,
    mode: Mode,
    scroll: u16,
    status: Option<String>,
}

pub fn run(store: Store) -> Result<()> {
    let mut session = Session::start();
    let mut browser = Browser::new(store);
    loop {
        session.terminal.draw(|frame| browser.draw(frame))?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        browser.status = None;
        match browser.handle(key)? {
            Action::None => {}
            Action::Quit => return Ok(()),
            Action::Edit => browser.edit(&mut session.terminal)?,
            Action::New => browser.new_jot(&mut session.terminal)?,
        }
    }
}

impl Browser {
    fn new(store: Store) -> Browser {
        let mut browser = Browser {
            store,
            visible: Vec::new(),
            list: ListState::default(),
            filter: String::new(),
            mode: Mode::Normal,
            scroll: 0,
            status: None,
        };
        browser.refilter();
        browser
    }

    /// Index into `store.jots` of the highlighted jot.
    fn selected(&self) -> Option<usize> {
        self.list
            .selected()
            .and_then(|i| self.visible.get(i))
            .copied()
    }

    fn select(&mut self, row: usize) {
        let row = row.min(self.visible.len().saturating_sub(1));
        self.list.select((!self.visible.is_empty()).then_some(row));
        self.scroll = 0;
    }

    /// Recompute `visible` from `filter` and select the newest match.
    fn refilter(&mut self) {
        let needle = self.filter.to_lowercase();
        self.visible = (0..self.store.jots.len())
            .rev()
            .filter(|&i| self.store.jots[i].body.to_lowercase().contains(&needle))
            .collect();
        self.select(0);
    }

    fn handle(&mut self, key: KeyEvent) -> Result<Action> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            return Ok(Action::Quit);
        }
        match self.mode {
            Mode::Filter => self.handle_filter(key),
            Mode::ConfirmDelete => self.handle_confirm(key)?,
            Mode::Normal => {
                let row = self.list.selected().unwrap_or(0);
                match key.code {
                    KeyCode::Esc if !self.filter.is_empty() => {
                        self.filter.clear();
                        self.refilter();
                    }
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(Action::Quit),
                    KeyCode::Down | KeyCode::Char('j') => self.select(row + 1),
                    KeyCode::Up | KeyCode::Char('k') => self.select(row.saturating_sub(1)),
                    KeyCode::Home | KeyCode::Char('g') => self.select(0),
                    KeyCode::End | KeyCode::Char('G') => self.select(usize::MAX),
                    KeyCode::PageDown | KeyCode::Char(' ') => {
                        self.scroll = self.scroll.saturating_add(10)
                    }
                    KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
                    KeyCode::Enter | KeyCode::Char('e') if self.selected().is_some() => {
                        return Ok(Action::Edit);
                    }
                    KeyCode::Char('n') => return Ok(Action::New),
                    KeyCode::Char('d') if self.selected().is_some() => {
                        self.mode = Mode::ConfirmDelete
                    }
                    KeyCode::Char('y') => {
                        if let Some(i) = self.selected() {
                            let via = clipboard::copy(&self.store.jots[i].body);
                            self.status = Some(format!(" copied #{} ({via}) ", i + 1));
                        }
                    }
                    KeyCode::Char('/') => self.mode = Mode::Filter,
                    _ => {}
                }
            }
        }
        Ok(Action::None)
    }

    fn handle_filter(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.mode = Mode::Normal,
            KeyCode::Esc => {
                self.filter.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.filter.pop();
            }
            KeyCode::Char(c) => self.filter.push(c),
            _ => return,
        }
        self.refilter();
    }

    fn handle_confirm(&mut self, key: KeyEvent) -> Result<()> {
        self.mode = Mode::Normal;
        let Some(i) = self.selected() else {
            return Ok(());
        };
        if key.code != KeyCode::Char('y') {
            self.status = Some(" kept ".into());
            return Ok(());
        }
        let row = self.list.selected().unwrap_or(0);
        self.store.remove(i)?;
        self.refilter();
        self.select(row);
        self.status = Some(format!(" deleted #{} ", i + 1));
        Ok(())
    }

    fn edit(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let Some(i) = self.selected() else {
            return Ok(());
        };
        let title = format!(
            " {} · #{} · {} ",
            self.store.project_name(i),
            i + 1,
            self.store.jots[i].stamp
        );
        self.status = Some(
            match editor::edit(terminal, &title, &self.store.jots[i].body)? {
                None => " discarded changes ".into(),
                Some(body) if body.is_empty() => format!(" empty jot, #{} left unchanged ", i + 1),
                Some(body) if body == self.store.jots[i].body => " no changes ".into(),
                Some(body) => {
                    self.store.update(i, body)?;
                    format!(" saved #{} ", i + 1)
                }
            },
        );
        Ok(())
    }

    fn new_jot(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        // New jots always go to the current project, also when browsing all projects.
        let title = format!(" jotty · {} ", self.store.project_short_name());
        self.status = Some(match editor::edit(terminal, &title, "")? {
            None => " discarded ".into(),
            Some(body) if body.is_empty() => " empty jot, nothing saved ".into(),
            Some(body) => {
                let i = self.store.add(&body)?;
                self.filter.clear();
                self.refilter();
                format!(" saved #{} ", i + 1)
            }
        });
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        let [body, footer] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());
        // A narrow column of dates; the preview pane shows the jot itself.
        let this_year = format!("{}-", Local::now().year());
        let dates: Vec<&str> = self
            .visible
            .iter()
            .map(|&i| {
                let stamp = &self.store.jots[i].stamp;
                stamp.strip_prefix(&this_year).unwrap_or(stamp)
            })
            .collect();
        let column = dates.iter().map(|d| d.chars().count()).max().unwrap_or(11) as u16 + 4;
        let [left, right] =
            Layout::horizontal([Constraint::Length(column), Constraint::Min(1)]).areas(body);

        let items: Vec<Line> = dates.into_iter().map(|d| Line::from(d).cyan()).collect();
        let mut list_block = Block::bordered();
        if !self.filter.is_empty() {
            list_block = list_block.title_bottom(format!(" /{} ", self.filter).yellow());
        }
        let list = List::new(items)
            .block(list_block)
            .highlight_style(Style::default().reversed());
        frame.render_stateful_widget(list, left, &mut self.list);

        let preview = match self.selected() {
            Some(i) => {
                let jot = &self.store.jots[i];
                Paragraph::new(markdown(&jot.body, right.width.saturating_sub(2)))
                    .block(
                        Block::bordered().title(
                            format!(
                                " {} · #{} · {} ",
                                self.store.project_name(i),
                                i + 1,
                                jot.stamp
                            )
                            .bold(),
                        ),
                    )
                    .wrap(Wrap { trim: false })
                    .scroll((self.scroll, 0))
            }
            None => {
                let msg = if self.store.jots.is_empty() {
                    " no jots yet, press n to write one"
                } else {
                    " no jots match the filter"
                };
                let scope = if self.store.global {
                    "all projects".to_string()
                } else {
                    self.store.project_short_name()
                };
                Paragraph::new(msg.dim())
                    .block(Block::bordered().title(format!(" {scope} ").bold()))
            }
        };
        frame.render_widget(preview, right);

        let footer_line = match self.mode {
            Mode::Filter => Line::from(vec![
                Span::raw(format!(" /{}▏", self.filter)).yellow(),
                Span::raw("  Enter keep · Esc clear").dim(),
            ]),
            Mode::ConfirmDelete => {
                let i = self.selected().unwrap_or(0);
                Line::from(format!(
                    " delete #{}: {}? y/N ",
                    i + 1,
                    self.store.jots[i].title()
                ))
                .red()
                .bold()
            }
            Mode::Normal => match &self.status {
                Some(status) => Line::from(status.as_str()).green(),
                None => Line::from(HELP).dim(),
            },
        };
        frame.render_widget(Paragraph::new(footer_line), footer);
    }
}

/// Render markdown for the preview pane with termimad, the same renderer as `jotty show`.
fn markdown(body: &str, width: u16) -> Text<'static> {
    let ansi = termimad::MadSkin::default()
        .text(body, Some(usize::from(width)))
        .to_string();
    ansi.into_text()
        .unwrap_or_else(|_| Text::raw(body.to_string()))
}
