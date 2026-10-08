mod browser;
mod clipboard;
mod editor;
mod store;

use std::io::{self, IsTerminal, Read, Write};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};

use editor::Session;
use store::Store;

const USAGE: &str = "\
jotty - project-scoped jots, stored outside the project

Jots belong to the enclosing git repository (or the current directory outside one).

usage:
  jotty                browse, edit, delete, copy and filter jots interactively
  jotty new            write a new jot (Esc / Ctrl+S save & quit, Ctrl+C discard)
  jotty <text...>      save <text> as a jot right away (use `jotty -- <text>` if it
                       starts with a command name or a dash)
  cmd | jotty          save piped input as a jot
  jotty ls             list jots
  jotty show [i]       render all jots, or the i-th one
  jotty edit [i]       edit the i-th jot (default: the newest)
  jotty rm <i>         delete the i-th jot
  jotty open           open the jot file in $VISUAL / $EDITOR
  jotty all            list all projects that have jots
  jotty path           print the jot file path

  -g, --global         alone or with ls, show, edit, rm or path: use the jots of all projects
                       (new jots still go to the current project)

  <i> is 1-based as shown by `ls`; negative counts from the end (-1, or `last`, is the newest).";

const COMMANDS: &[&str] = &[
    "new", "ls", "show", "edit", "rm", "open", "all", "path", "help",
];
const GLOBAL_COMMANDS: &[&str] = &["ls", "show", "edit", "rm", "path"];

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        // The reader went away (`jotty ls | head -1`); nothing left to do.
        Err(e)
            if e.downcast_ref::<io::Error>()
                .is_some_and(|e| e.kind() == io::ErrorKind::BrokenPipe) =>
        {
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {e:?}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (global, args) = split_global(&args);
    if global && !args.first().is_none_or(|cmd| GLOBAL_COMMANDS.contains(cmd)) {
        bail!(
            "-g works alone or with ls, show, edit, rm and path; new jots always go to the current project"
        );
    }
    let mut store = Store::for_cwd(global)?;

    match args.as_slice() {
        [] if !global && !io::stdin().is_terminal() => {
            let mut text = String::new();
            io::stdin()
                .read_to_string(&mut text)
                .context("reading stdin")?;
            quick(&mut store, &text)
        }
        [] => {
            require_terminal("jotty")?;
            browser::run(store)
        }
        ["new"] => {
            require_terminal("jotty new")?;
            new_jot(&mut store)
        }
        ["ls"] => list(&store),
        ["show"] => print(&store, None),
        ["show", i] => print(&store, Some(i)),
        ["edit"] => {
            require_terminal("jotty edit")?;
            edit(&mut store, "last")
        }
        ["edit", i] => {
            require_terminal("jotty edit")?;
            edit(&mut store, i)
        }
        ["rm", i] => remove(&mut store, i),
        ["open"] => {
            require_terminal("jotty open")?;
            open(&store)
        }
        ["all"] => all(&store),
        ["path"] => {
            let path = if global {
                store::config_dir()?
            } else {
                store.file().to_path_buf()
            };
            writeln!(io::stdout(), "{}", path.display())?;
            Ok(())
        }
        ["-h" | "--help" | "help"] => {
            writeln!(io::stdout(), "{USAGE}")?;
            Ok(())
        }
        ["--", text @ ..] => quick(&mut store, &text.join(" ")),
        [cmd, ..] if COMMANDS.contains(cmd) || cmd.starts_with('-') => {
            bail!("unknown arguments: {} (see `jotty help`)", args.join(" "))
        }
        text => quick(&mut store, &text.join(" ")),
    }
}

/// Pull out `-g`/`--global`: as the first argument, or anywhere after a command.
/// Quick-capture text is left alone, so `jotty fix the -g flag` saves that text.
fn split_global<'a>(args: &[&'a str]) -> (bool, Vec<&'a str>) {
    let is_flag = |arg: &&str| matches!(*arg, "-g" | "--global");
    match args.first() {
        Some(first) if is_flag(first) => (true, args[1..].to_vec()),
        Some(cmd) if COMMANDS.contains(cmd) => {
            let rest: Vec<&str> = args.iter().copied().filter(|a| !is_flag(a)).collect();
            (rest.len() != args.len(), rest)
        }
        _ => (false, args.to_vec()),
    }
}

/// The editor and browser need a real terminal; fail fast instead of hanging in a pipe or agent shell.
fn require_terminal(cmd: &str) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("`{cmd}` needs an interactive terminal");
    }
    Ok(())
}

/// `#3 2026-10-02 14:57`, plus the project name when showing all projects.
fn label(store: &Store, i: usize) -> String {
    let jot = &store.jots[i];
    if store.global {
        format!("#{} · {} · {}", i + 1, jot.stamp, store.project_name(i))
    } else {
        format!("#{} · {}", i + 1, jot.stamp)
    }
}

fn quick(store: &mut Store, text: &str) -> Result<()> {
    if text.trim().is_empty() {
        eprintln!("empty jot, nothing saved");
        return Ok(());
    }
    let i = store.add(text)?;
    eprintln!("saved #{}", i + 1);
    Ok(())
}

fn new_jot(store: &mut Store) -> Result<()> {
    let title = format!(" jotty · {} ", store.project().display());
    let mut session = Session::start();
    let result = editor::edit(&mut session.terminal, &title, "");
    drop(session);

    match result? {
        None => eprintln!("discarded"),
        Some(body) if body.is_empty() => eprintln!("empty jot, nothing saved"),
        Some(body) => {
            let i = store.add(&body)?;
            eprintln!("saved #{}", i + 1);
        }
    }
    Ok(())
}

fn edit(store: &mut Store, spec: &str) -> Result<()> {
    let i = store.resolve(spec)?;
    let title = format!(" jotty · {} ", label(store, i));
    let mut session = Session::start();
    let result = editor::edit(&mut session.terminal, &title, &store.jots[i].body);
    drop(session);

    match result? {
        None => eprintln!("discarded changes"),
        Some(body) if body.is_empty() => eprintln!("empty jot, #{} left unchanged", i + 1),
        Some(body) if body == store.jots[i].body => eprintln!("no changes"),
        Some(body) => {
            store.update(i, body)?;
            eprintln!("saved #{}", i + 1);
        }
    }
    Ok(())
}

fn remove(store: &mut Store, spec: &str) -> Result<()> {
    let i = store.resolve(spec)?;
    let label = format!("{}  {}", label(store, i), store.jots[i].title());
    if io::stdin().is_terminal() {
        eprint!("delete {label}? [y/N] ");
        io::stderr().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            eprintln!("kept");
            return Ok(());
        }
    }
    store.remove(i)?;
    eprintln!("deleted {label}");
    Ok(())
}

fn open(store: &Store) -> Result<()> {
    store.ensure_file()?;
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| if cfg!(windows) { "notepad" } else { "vi" }.into());
    let mut parts = editor.split_whitespace();
    let program = parts.next().context("$EDITOR is empty")?;
    let status = Command::new(program)
        .args(parts)
        .arg(store.file())
        .status()
        .with_context(|| format!("running {editor}"))?;
    if !status.success() {
        bail!("{editor} exited with {status}");
    }
    Ok(())
}

fn list(store: &Store) -> Result<()> {
    if store.jots.is_empty() {
        eprintln!("{}", store.empty_message());
        return Ok(());
    }
    let width = store.jots.len().to_string().len();
    let names: Vec<String> = (0..store.jots.len())
        .map(|i| {
            if store.global {
                format!("[{}]  ", store.project_name(i))
            } else {
                String::new()
            }
        })
        .collect();
    let name_width = names.iter().map(|n| n.chars().count()).max().unwrap_or(0);
    let mut out = io::stdout().lock();
    for (i, jot) in store.jots.iter().enumerate() {
        let title = jot.title();
        let preview: String = title.chars().take(70).collect();
        let ellipsis = if title.chars().count() > 70 {
            "…"
        } else {
            ""
        };
        writeln!(
            out,
            "{:>width$}  {}  {:<name_width$}{preview}{ellipsis}",
            i + 1,
            jot.stamp,
            names[i]
        )?;
    }
    Ok(())
}

fn print(store: &Store, spec: Option<&str>) -> Result<()> {
    let selected: Vec<usize> = match spec {
        None => (0..store.jots.len()).collect(),
        Some(spec) => vec![store.resolve(spec)?],
    };
    if selected.is_empty() {
        eprintln!("{}", store.empty_message());
        return Ok(());
    }

    // Rendered for people, raw markdown for pipes (and agents).
    let skin = io::stdout().is_terminal().then(termimad::MadSkin::default);
    let mut out = io::stdout().lock();
    for (n, &i) in selected.iter().enumerate() {
        if n > 0 {
            writeln!(out)?;
        }
        let text = format!("### {}\n\n{}\n", label(store, i), store.jots[i].body);
        match &skin {
            Some(skin) => skin.print_text(&text),
            None => write!(out, "{text}")?,
        }
    }
    Ok(())
}

fn all(current: &Store) -> Result<()> {
    let projects = store::projects()?;
    if projects.is_empty() {
        eprintln!("no jots anywhere yet");
        return Ok(());
    }
    let width = projects
        .iter()
        .map(|p| p.count.to_string().len())
        .max()
        .unwrap_or(1);
    let mut out = io::stdout().lock();
    for p in projects {
        let marker = if p.file == current.file() { "*" } else { " " };
        let name = match &p.project {
            Some(path) => path.display().to_string(),
            None => p.file.display().to_string(),
        };
        let latest = p.latest.unwrap_or_default();
        writeln!(out, "{marker} {:>width$}  {latest}  {name}", p.count)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_flag_placement() {
        assert_eq!(split_global(&["-g", "ls"]), (true, vec!["ls"]));
        assert_eq!(split_global(&["ls", "-g"]), (true, vec!["ls"]));
        assert_eq!(
            split_global(&["show", "--global", "-1"]),
            (true, vec!["show", "-1"])
        );
        assert_eq!(split_global(&["ls"]), (false, vec!["ls"]));
        assert_eq!(split_global(&["-g"]), (true, vec![]));
        // Quick-capture text keeps its -g.
        assert_eq!(
            split_global(&["fix", "the", "-g", "flag"]),
            (false, vec!["fix", "the", "-g", "flag"])
        );
        assert_eq!(split_global(&["--", "-g"]), (false, vec!["--", "-g"]));
    }
}
