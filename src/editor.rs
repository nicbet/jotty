use std::io;

use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use ratatui::DefaultTerminal;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use tui_textarea::{CursorMove, TextArea};

/// A full-screen terminal session; the terminal is restored when it is dropped.
pub struct Session {
    pub terminal: DefaultTerminal,
}

impl Session {
    pub fn start() -> Session {
        let terminal = ratatui::init();
        // Deliver pastes as a single Event::Paste instead of a stream of keys, where a
        // bare LF arrives as Ctrl+J and pasted control sequences would trigger shortcuts.
        let _ = execute!(io::stdout(), EnableBracketedPaste);
        Session { terminal }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableBracketedPaste);
        ratatui::restore();
    }
}

/// Edit `initial` in a full-screen textarea. Returns the trimmed text on save, `None` if discarded.
pub fn edit(
    terminal: &mut DefaultTerminal,
    title: &str,
    initial: &str,
) -> io::Result<Option<String>> {
    let mut textarea = TextArea::new(initial.lines().map(String::from).collect());
    textarea.move_cursor(CursorMove::Bottom);
    textarea.move_cursor(CursorMove::End);
    textarea.set_cursor_line_style(Style::default());
    textarea.set_block(Block::default().borders(Borders::ALL).title(title.bold()));
    let help = Paragraph::new(Line::from(" Esc/Ctrl+S save & quit · Ctrl+C discard ").dim());

    loop {
        terminal.draw(|frame| {
            let [main, footer] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());
            frame.render_widget(&textarea, main);
            frame.render_widget(&help, footer);
        })?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Esc => break,
                    KeyCode::Char('s') if ctrl => break,
                    KeyCode::Char('c') if ctrl => return Ok(None),
                    // LF from terminals without bracketed paste
                    KeyCode::Char('j') if ctrl => textarea.insert_newline(),
                    _ => {
                        textarea.input(key);
                    }
                }
            }
            Event::Paste(text) => {
                textarea.insert_str(text.replace("\r\n", "\n").replace('\r', "\n"));
            }
            ev => {
                textarea.input(ev);
            }
        }
    }
    Ok(Some(textarea.lines().join("\n").trim().to_string()))
}
