mod browser;
mod clipboard;
mod editor;
mod store;

use std::io::{self, IsTerminal, Read, Write};
use std::process::Command;

use anyhow::{Context, Result, bail};

use editor::Session;
use store::Store;

const USAGE: &str = "\
jotty - project-scoped jots, stored outside the project

Jots belong to the enclosing git repository (or the current directory outside one).

usage:
  jotty                write a new jot (Esc / Ctrl+S save & quit, Ctrl+C discard)
  jotty <text...>      save <text> as a jot right away (use `jotty -- <text>` if it
                       starts with a command name or a dash)
  cmd | jotty          save piped input as a jot
  jotty b              browse, edit, delete, copy and filter jots interactively
  jotty ls             list jots
  jotty p [i]          render all jots, or the i-th one
  jotty e [i]          edit the i-th jot (default: the newest)
  jotty rm <i>         delete the i-th jot
  jotty open           open the jot file in $VISUAL / $EDITOR
  jotty all            list all projects that have jots
  jotty path           print the jot file path

  <i> is 1-based as shown by `ls`; negative counts from the end (-1, or `last`, is the newest).";

const COMMANDS: &[&str] = &["b", "ls", "p", "e", "rm", "open", "all", "path", "help"];

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut store = Store::for_cwd()?;

    match args.as_slice() {
        [] if io::stdin().is_terminal() => new_jot(&mut store),
        [] => {
            let mut text = String::new();
            io::stdin()
                .read_to_string(&mut text)
                .context("reading stdin")?;
            quick(&mut store, &text)
        }
        ["b"] => browser::run(store),
        ["ls"] => list(&store),
        ["p"] => print(&store, None),
        ["p", i] => print(&store, Some(i)),
        ["e"] => edit(&mut store, "last"),
        ["e", i] => edit(&mut store, i),
        ["rm", i] => remove(&mut store, i),
        ["open"] => open(&store),
        ["all"] => all(&store),
        ["path"] => {
            println!("{}", store.file.display());
            Ok(())
        }
        ["-h" | "--help" | "help"] => {
            println!("{USAGE}");
            Ok(())
        }
        ["--", text @ ..] => quick(&mut store, &text.join(" ")),
        [cmd, ..] if COMMANDS.contains(cmd) || cmd.starts_with('-') => {
            bail!("unknown arguments: {} (see `jotty help`)", args.join(" "))
        }
        text => quick(&mut store, &text.join(" ")),
    }
}

fn quick(store: &mut Store, text: &str) -> Result<()> {
    if text.trim().is_empty() {
        eprintln!("empty jot, nothing saved");
        return Ok(());
    }
    store.add(text)?;
    eprintln!("saved #{}", store.jots.len());
    Ok(())
}

fn new_jot(store: &mut Store) -> Result<()> {
    let title = format!(" jotty · {} ", store.project.display());
    let mut session = Session::start();
    let result = editor::edit(&mut session.terminal, &title, "");
    drop(session);

    match result? {
        None => eprintln!("discarded"),
        Some(body) if body.is_empty() => eprintln!("empty jot, nothing saved"),
        Some(body) => {
            store.add(&body)?;
            eprintln!("saved #{}", store.jots.len());
        }
    }
    Ok(())
}

fn edit(store: &mut Store, spec: &str) -> Result<()> {
    let i = store.resolve(spec)?;
    let jot = &store.jots[i];
    let title = format!(" jotty · #{} · {} ", i + 1, jot.stamp);
    let mut session = Session::start();
    let result = editor::edit(&mut session.terminal, &title, &jot.body);
    drop(session);

    match result? {
        None => eprintln!("discarded changes"),
        Some(body) if body.is_empty() => eprintln!("empty jot, #{} left unchanged", i + 1),
        Some(body) if body == jot.body => eprintln!("no changes"),
        Some(body) => {
            store.jots[i].body = body;
            store.save()?;
            eprintln!("saved #{}", i + 1);
        }
    }
    Ok(())
}

fn remove(store: &mut Store, spec: &str) -> Result<()> {
    let i = store.resolve(spec)?;
    let jot = &store.jots[i];
    let label = format!("#{} {}  {}", i + 1, jot.stamp, jot.title());
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
    store.jots.remove(i);
    store.save()?;
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
        .arg(&store.file)
        .status()
        .with_context(|| format!("running {editor}"))?;
    if !status.success() {
        bail!("{editor} exited with {status}");
    }
    Ok(())
}

fn list(store: &Store) -> Result<()> {
    if store.jots.is_empty() {
        eprintln!("no jots yet ({})", store.file.display());
        return Ok(());
    }
    let width = store.jots.len().to_string().len();
    for (i, jot) in store.jots.iter().enumerate() {
        let title = jot.title();
        let preview: String = title.chars().take(70).collect();
        let ellipsis = if title.chars().count() > 70 {
            "…"
        } else {
            ""
        };
        println!("{:>width$}  {}  {preview}{ellipsis}", i + 1, jot.stamp);
    }
    Ok(())
}

fn print(store: &Store, spec: Option<&str>) -> Result<()> {
    let selected: Vec<usize> = match spec {
        None => (0..store.jots.len()).collect(),
        Some(spec) => vec![store.resolve(spec)?],
    };
    if selected.is_empty() {
        eprintln!("no jots yet ({})", store.file.display());
        return Ok(());
    }

    let skin = termimad::MadSkin::default();
    for (n, &i) in selected.iter().enumerate() {
        if n > 0 {
            println!();
        }
        let jot = &store.jots[i];
        skin.print_text(&format!("### #{} · {}\n\n{}\n", i + 1, jot.stamp, jot.body));
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
    for p in projects {
        let marker = if p.file == current.file { "*" } else { " " };
        let name = match &p.project {
            Some(path) => path.display().to_string(),
            None => p.file.display().to_string(),
        };
        let latest = p.latest.unwrap_or_default();
        println!("{marker} {:>width$}  {latest}  {name}", p.count);
    }
    Ok(())
}
