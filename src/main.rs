use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::Local;
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use tui_textarea::TextArea;

const END_MARKER: &str = "<!-- end -->";

const USAGE: &str = "\
jotty - project-scoped jots, stored outside the project

usage:
  jotty          write a new jot (Esc / Ctrl+S save & quit, Ctrl+C discard)
  jotty ls       list jots for the current directory
  jotty p        render all jots
  jotty p <i>    render the i-th jot (1-based, as shown by `ls`)
  jotty path     print the jot file path for the current directory";

struct Jot {
    stamp: String,
    body: String,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let file = jot_file()?;

    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => write_jot(&file),
        ["ls"] => list(&file),
        ["p"] => print(&file, None),
        ["p", i] => {
            let i: usize = i
                .parse()
                .with_context(|| format!("not a jot number: {i}"))?;
            print(&file, Some(i))
        }
        ["path"] => {
            println!("{}", file.display());
            Ok(())
        }
        ["-h" | "--help" | "help"] => {
            println!("{USAGE}");
            Ok(())
        }
        _ => bail!("unknown arguments\n\n{USAGE}"),
    }
}

/// `~/.config/jotty/-kebab-case-of-cwd.md` (honours `$XDG_CONFIG_HOME`).
fn jot_file() -> Result<PathBuf> {
    let config = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => {
            let home = std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .context("$HOME is not set")?;
            PathBuf::from(home).join(".config")
        }
    };
    let cwd = std::env::current_dir().context("cannot determine current directory")?;
    Ok(config
        .join("jotty")
        .join(format!("{}.md", kebab(&cwd.to_string_lossy()))))
}

fn kebab(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    if out.len() > 1 {
        out.truncate(out.trim_end_matches('-').len());
    }
    out
}

fn load(file: &PathBuf) -> Result<Vec<Jot>> {
    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", file.display())),
    };

    let mut jots = Vec::new();
    let mut current: Option<Jot> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == END_MARKER {
            if let Some(mut jot) = current.take() {
                jot.body.truncate(jot.body.trim_end().len());
                jots.push(jot);
            }
        } else if let Some(stamp) = start_marker(trimmed).filter(|_| current.is_none()) {
            current = Some(Jot {
                stamp: stamp.to_string(),
                body: String::new(),
            });
        } else if let Some(jot) = current.as_mut() {
            jot.body.push_str(line);
            jot.body.push('\n');
        }
    }
    Ok(jots)
}

fn start_marker(line: &str) -> Option<&str> {
    line.strip_prefix("<!--")?
        .strip_suffix("-->")
        .map(str::trim)
}

fn append(file: &PathBuf, body: &str) -> Result<()> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let needs_gap = fs::read_to_string(file).is_ok_and(|t| !t.is_empty() && !t.ends_with("\n\n"));
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .with_context(|| format!("opening {}", file.display()))?;
    let stamp = Local::now().format("%Y-%m-%d %H:%M");
    let gap = if needs_gap { "\n" } else { "" };
    write!(f, "{gap}<!-- {stamp} -->\n{body}\n{END_MARKER}\n\n")?;
    Ok(())
}

fn write_jot(file: &PathBuf) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let mut textarea = TextArea::default();
    textarea.set_cursor_line_style(Style::default());
    textarea.set_block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" jotty · {} ", cwd.display()).bold()),
    );
    let help = Paragraph::new(Line::from(" Esc/Ctrl+S save & quit · Ctrl+C discard ").dim());

    let mut terminal = ratatui::init();
    // Deliver pastes as a single Event::Paste instead of a stream of keys, where a
    // bare LF arrives as Ctrl+J and pasted control sequences would trigger shortcuts.
    let _ = execute!(std::io::stdout(), EnableBracketedPaste);
    let save = loop {
        let drawn = terminal.draw(|frame| {
            let [main, footer] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());
            frame.render_widget(&textarea, main);
            frame.render_widget(&help, footer);
        });
        if let Err(e) = drawn {
            restore_terminal();
            return Err(e.into());
        }

        match event::read() {
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Esc => break true,
                    KeyCode::Char('s') if ctrl => break true,
                    KeyCode::Char('c') if ctrl => break false,
                    // LF from terminals without bracketed paste
                    KeyCode::Char('j') if ctrl => textarea.insert_newline(),
                    _ => {
                        textarea.input(key);
                    }
                }
            }
            Ok(Event::Paste(text)) => {
                textarea.insert_str(text.replace("\r\n", "\n").replace('\r', "\n"));
            }
            Ok(ev) => {
                textarea.input(ev);
            }
            Err(e) => {
                restore_terminal();
                return Err(e.into());
            }
        }
    };
    restore_terminal();

    let body = textarea.lines().join("\n");
    let body = body.trim();
    if !save {
        eprintln!("discarded");
    } else if body.is_empty() {
        eprintln!("empty jot, nothing saved");
    } else {
        append(file, body)?;
        eprintln!("saved to {}", file.display());
    }
    Ok(())
}

fn restore_terminal() {
    let _ = execute!(std::io::stdout(), DisableBracketedPaste);
    ratatui::restore();
}

fn list(file: &PathBuf) -> Result<()> {
    let jots = load(file)?;
    if jots.is_empty() {
        eprintln!("no jots yet ({})", file.display());
        return Ok(());
    }
    let width = jots.len().to_string().len();
    for (i, jot) in jots.iter().enumerate() {
        let first = jot
            .body
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim();
        let preview: String = first.chars().take(70).collect();
        let ellipsis = if first.chars().count() > 70 {
            "…"
        } else {
            ""
        };
        println!("{:>width$}  {}  {preview}{ellipsis}", i + 1, jot.stamp);
    }
    Ok(())
}

fn print(file: &PathBuf, index: Option<usize>) -> Result<()> {
    let jots = load(file)?;
    let selected: Vec<(usize, &Jot)> = match index {
        None => jots.iter().enumerate().collect(),
        Some(i) => match i.checked_sub(1).and_then(|i| jots.get(i)) {
            Some(jot) => vec![(i - 1, jot)],
            None => bail!("no jot #{i} (there are {})", jots.len()),
        },
    };
    if selected.is_empty() {
        eprintln!("no jots yet ({})", file.display());
        return Ok(());
    }

    let skin = termimad::MadSkin::default();
    for (n, (i, jot)) in selected.iter().enumerate() {
        if n > 0 {
            println!();
        }
        skin.print_text(&format!("### #{} · {}\n\n{}\n", i + 1, jot.stamp, jot.body));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kebab_paths() {
        assert_eq!(
            kebab("/Users/me/Work/My Project"),
            "-users-me-work-my-project"
        );
        assert_eq!(kebab("/home/me/foo_bar.rs/"), "-home-me-foo-bar-rs");
        assert_eq!(kebab("/"), "-");
    }

    #[test]
    fn parses_entries() {
        let dir = std::env::temp_dir().join(format!("jotty-test-{}", std::process::id()));
        let file = dir.join("t.md");
        let _ = fs::remove_dir_all(&dir);
        append(&file, "first\n\n- a\n- b").unwrap();
        append(&file, "second").unwrap();
        let jots = load(&file).unwrap();
        assert_eq!(jots.len(), 2);
        assert_eq!(jots[0].body, "first\n\n- a\n- b");
        assert_eq!(jots[1].body, "second");
        fs::remove_dir_all(&dir).unwrap();
    }
}
