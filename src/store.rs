use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{Local, NaiveDateTime};

const END_MARKER: &str = "<!-- end -->";
const HEADER_PREFIX: &str = "jotty:";
const STAMP_FORMAT: &str = "%Y-%m-%d %H:%M";

pub struct Jot {
    pub stamp: String,
    pub body: String,
    /// Index into `Store::sources` of the file this jot lives in.
    source: usize,
}

impl Jot {
    /// First line with text, minus heading markers, for one-line listings.
    pub fn title(&self) -> &str {
        self.body
            .lines()
            .map(|l| l.trim().trim_start_matches('#').trim_start())
            .find(|l| !l.is_empty() && !l.starts_with("```"))
            .unwrap_or("")
    }
}

/// One jot file and the project it belongs to.
struct Source {
    file: PathBuf,
    /// From the file's header; `None` for files written before headers existed.
    project: Option<PathBuf>,
}

/// The current project's jots, or with `global` every project's, oldest first.
pub struct Store {
    pub jots: Vec<Jot>,
    pub global: bool,
    sources: Vec<Source>,
    /// The current project's source; new jots always go here.
    home: usize,
}

/// Summary of one jot file, for `jotty all`.
pub struct Project {
    pub file: PathBuf,
    pub project: Option<PathBuf>,
    pub count: usize,
    pub latest: Option<String>,
}

impl Store {
    /// Jots of the enclosing git repository (or the current directory outside of one),
    /// or of all projects when `global` is set.
    pub fn for_cwd(global: bool) -> Result<Store> {
        let cwd = std::env::current_dir().context("cannot determine current directory")?;
        let project = project_root(&cwd);
        let dir = config_dir()?;
        let home = dir.join(format!("{}.md", kebab(&project.to_string_lossy())));
        let mut files = if global { jot_files(&dir)? } else { Vec::new() };
        if !files.contains(&home) {
            files.push(home.clone());
        }
        Store::load(files, &home, project, global)
    }

    fn load(files: Vec<PathBuf>, home: &Path, project: PathBuf, global: bool) -> Result<Store> {
        let mut store = Store {
            jots: Vec::new(),
            global,
            sources: Vec::new(),
            home: 0,
        };
        for file in files {
            let parsed = read(&file)?.map(|t| parse(&t)).unwrap_or_default();
            let source = store.sources.len();
            let project = if file == home {
                store.home = source;
                Some(project.clone())
            } else {
                parsed.project
            };
            store.sources.push(Source { file, project });
            store
                .jots
                .extend(parsed.jots.into_iter().map(|jot| Jot { source, ..jot }));
        }
        if global {
            // Stable, so jots from the same minute keep their order within a file.
            store.jots.sort_by(|a, b| a.stamp.cmp(&b.stamp));
        }
        Ok(store)
    }

    /// The current project's jot file.
    pub fn file(&self) -> &Path {
        &self.sources[self.home].file
    }

    /// The current project's directory.
    pub fn project(&self) -> &Path {
        self.sources[self.home]
            .project
            .as_deref()
            .unwrap_or(Path::new(""))
    }

    /// Short name of the project jot `i` belongs to.
    pub fn project_name(&self, i: usize) -> String {
        let source = &self.sources[self.jots[i].source];
        let name = match &source.project {
            Some(project) => project.file_name(),
            None => source.file.file_stem(),
        };
        name.map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    }

    /// Append a new jot to the current project, leaving the rest of its file untouched.
    /// Returns the new jot's index.
    pub fn add(&mut self, body: &str) -> Result<usize> {
        let jot = Jot {
            stamp: Local::now().format(STAMP_FORMAT).to_string(),
            body: body.trim().to_string(),
            source: self.home,
        };
        let source = &self.sources[self.home];
        let existing = read(&source.file)?.unwrap_or_default();
        let prefix = if existing.is_empty() {
            header(source)
        } else if !existing.ends_with('\n') {
            "\n\n".to_string()
        } else if !existing.ends_with("\n\n") {
            "\n".to_string()
        } else {
            String::new()
        };

        create_dir(&source.file)?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&source.file)
            .with_context(|| format!("opening {}", source.file.display()))?;
        write!(f, "{prefix}{}", entry(&jot))?;
        self.jots.push(jot);
        Ok(self.jots.len() - 1)
    }

    /// Replace the body of jot `i` and rewrite its file.
    pub fn update(&mut self, i: usize, body: String) -> Result<()> {
        self.jots[i].body = body;
        self.save(self.jots[i].source)
    }

    /// Delete jot `i` and rewrite its file.
    pub fn remove(&mut self, i: usize) -> Result<Jot> {
        let jot = self.jots.remove(i);
        self.save(jot.source)?;
        Ok(jot)
    }

    /// Rewrite one source file from `self.jots`; removes it once its last jot is gone.
    fn save(&self, source: usize) -> Result<()> {
        let Source { file, .. } = &self.sources[source];
        let mut jots = self.jots.iter().filter(|j| j.source == source).peekable();
        if jots.peek().is_none() {
            return match fs::remove_file(file) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => {
                    Err(e).with_context(|| format!("removing {}", file.display()))
                }
                _ => Ok(()),
            };
        }
        let mut text = header(&self.sources[source]);
        for jot in jots {
            text.push_str(&entry(jot));
        }
        create_dir(file)?;
        let tmp = file.with_extension("md.tmp");
        fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
        fs::rename(&tmp, file).with_context(|| format!("writing {}", file.display()))
    }

    /// Make sure the current project's file exists, e.g. before handing it to `$EDITOR`.
    pub fn ensure_file(&self) -> Result<()> {
        let source = &self.sources[self.home];
        if !source.file.exists() {
            create_dir(&source.file)?;
            fs::write(&source.file, header(source))
                .with_context(|| format!("writing {}", source.file.display()))?;
        }
        Ok(())
    }

    /// Resolve a user-facing jot number to an index: `3` (1-based), `-1` (from the end) or `last`.
    pub fn resolve(&self, spec: &str) -> Result<usize> {
        let n = self.jots.len();
        if n == 0 {
            bail!("{}", self.empty_message());
        }
        let index = match spec {
            "last" => Some(n - 1),
            _ => match spec.parse::<isize>() {
                Ok(k) if k > 0 => Some(k as usize - 1),
                Ok(k) if k < 0 => n.checked_sub(k.unsigned_abs()),
                Ok(_) => None,
                Err(_) => bail!("not a jot number: {spec}"),
            },
        };
        index
            .filter(|&i| i < n)
            .with_context(|| format!("no jot {spec} (there are {n})"))
    }

    pub fn empty_message(&self) -> String {
        if self.global {
            "no jots anywhere yet".into()
        } else {
            format!("no jots yet ({})", self.file().display())
        }
    }
}

fn header(source: &Source) -> String {
    match &source.project {
        Some(project) => format!("<!-- {HEADER_PREFIX} {} -->\n\n", project.display()),
        None => String::new(),
    }
}

fn create_dir(file: &Path) -> Result<()> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    Ok(())
}

/// The `*.md` jot files in `dir`, sorted by name.
fn jot_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };
    let mut files = Vec::new();
    for entry in entries {
        let file = entry?.path();
        if file.extension().is_some_and(|ext| ext == "md") {
            files.push(file);
        }
    }
    files.sort();
    Ok(files)
}

/// All projects with jots, most recently jotted first.
pub fn projects() -> Result<Vec<Project>> {
    let mut projects = Vec::new();
    for file in jot_files(&config_dir()?)? {
        let Some(text) = read(&file)? else { continue };
        let parsed = parse(&text);
        if parsed.jots.is_empty() {
            continue;
        }
        projects.push(Project {
            latest: parsed.jots.iter().map(|j| j.stamp.clone()).max(),
            count: parsed.jots.len(),
            project: parsed.project,
            file,
        });
    }
    projects.sort_by(|a, b| b.latest.cmp(&a.latest).then_with(|| a.file.cmp(&b.file)));
    Ok(projects)
}

/// `$XDG_CONFIG_HOME/jotty`, defaulting to `~/.config/jotty`.
pub fn config_dir() -> Result<PathBuf> {
    let config = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => {
            let home = std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .context("$HOME is not set")?;
            PathBuf::from(home).join(".config")
        }
    };
    Ok(config.join("jotty"))
}

/// The closest ancestor containing `.git`, so jots are shared across a repo's subdirectories.
fn project_root(cwd: &Path) -> PathBuf {
    cwd.ancestors()
        .find(|dir| dir.join(".git").exists())
        .unwrap_or(cwd)
        .to_path_buf()
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

fn read(file: &Path) -> Result<Option<String>> {
    match fs::read_to_string(file) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", file.display())),
    }
}

#[derive(Default)]
struct Parsed {
    project: Option<PathBuf>,
    jots: Vec<Jot>,
}

fn parse(text: &str) -> Parsed {
    let mut project = None;
    let mut jots = Vec::new();
    let mut current: Option<Jot> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if current.is_some() && trimmed == END_MARKER {
            if let Some(mut jot) = current.take() {
                jot.body.truncate(jot.body.trim_end().len());
                jots.push(jot);
            }
        } else if let Some(jot) = current.as_mut() {
            jot.body.push_str(line);
            jot.body.push('\n');
        } else if let Some(inner) = comment(trimmed) {
            if let Some(path) = inner.strip_prefix(HEADER_PREFIX) {
                project.get_or_insert_with(|| PathBuf::from(path.trim()));
            } else if NaiveDateTime::parse_from_str(inner, STAMP_FORMAT).is_ok() {
                current = Some(Jot {
                    stamp: inner.to_string(),
                    body: String::new(),
                    source: 0,
                });
            }
        }
    }
    Parsed { project, jots }
}

fn comment(line: &str) -> Option<&str> {
    line.strip_prefix("<!--")?
        .strip_suffix("-->")
        .map(str::trim)
}

fn entry(jot: &Jot) -> String {
    format!("<!-- {} -->\n{}\n{END_MARKER}\n\n", jot.stamp, jot.body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jotty-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn kebab_paths() {
        assert_eq!(
            kebab("/Users/me/Work/My Project"),
            "-users-me-work-my-project"
        );
        assert_eq!(kebab("/home/me/foo_bar.rs/"), "-home-me-foo-bar-rs");
        assert_eq!(kebab("/"), "-");
    }

    fn bodies(store: &Store) -> Vec<&str> {
        store.jots.iter().map(|j| j.body.as_str()).collect()
    }

    #[test]
    fn add_update_remove_and_reload() {
        let dir = temp_dir("store");
        let file = dir.join("t.md");
        let project = PathBuf::from("/some/project");
        let open = || Store::load(vec![file.clone()], &file, project.clone(), false).unwrap();
        let mut store = open();
        assert_eq!(store.add("first\n\n- a\n- b\n").unwrap(), 0);
        store.add("second").unwrap();
        assert_eq!(store.add("third").unwrap(), 2);

        let text = fs::read_to_string(&file).unwrap();
        assert!(text.starts_with("<!-- jotty: /some/project -->\n\n<!-- "));
        assert_eq!(parse(&text).project, Some(project.clone()));
        assert_eq!(bodies(&open()), ["first\n\n- a\n- b", "second", "third"]);

        assert_eq!(store.remove(1).unwrap().body, "second");
        store.update(1, "third, edited".into()).unwrap();
        assert_eq!(bodies(&open()), ["first\n\n- a\n- b", "third, edited"]);

        store.remove(0).unwrap();
        store.remove(0).unwrap();
        assert!(!file.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn global_store_merges_files_and_writes_back_to_each() {
        let dir = temp_dir("global");
        let (a, b) = (dir.join("a.md"), dir.join("b.md"));
        fs::write(&a, "<!-- jotty: /p/alpha -->\n\n<!-- 2026-01-01 10:00 -->\na1\n<!-- end -->\n\n<!-- 2026-01-03 10:00 -->\na2\n<!-- end -->\n").unwrap();
        // A legacy file without header.
        fs::write(&b, "<!-- 2026-01-02 10:00 -->\nb1\n<!-- end -->\n").unwrap();

        let open = || {
            Store::load(
                vec![a.clone(), b.clone()],
                &a,
                PathBuf::from("/p/alpha"),
                true,
            )
            .unwrap()
        };
        let mut store = open();
        assert_eq!(bodies(&store), ["a1", "b1", "a2"]);
        assert_eq!(store.project_name(0), "alpha");
        assert_eq!(store.project_name(1), "b");

        store.update(1, "b1, edited".into()).unwrap();
        let i = store.add("a3").unwrap();
        assert_eq!(store.project_name(i), "alpha");
        assert_eq!(
            fs::read_to_string(&b).unwrap(),
            "<!-- 2026-01-02 10:00 -->\nb1, edited\n<!-- end -->\n\n"
        );
        assert_eq!(bodies(&open())[..3], ["a1", "b1, edited", "a2"]);

        store.remove(1).unwrap();
        assert!(!b.exists());
        assert_eq!(bodies(&open()), ["a1", "a2", "a3"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parses_legacy_files_without_header() {
        let parsed =
            parse("<!-- 2026-10-02 14:57 -->\nhello\n<!-- end -->\n\n<!-- not a stamp -->\n");
        assert_eq!(parsed.project, None);
        assert_eq!(parsed.jots.len(), 1);
        assert_eq!(parsed.jots[0].stamp, "2026-10-02 14:57");
        assert_eq!(parsed.jots[0].body, "hello");
    }

    #[test]
    fn titles_skip_markup() {
        let jot = |body: &str| Jot {
            stamp: String::new(),
            body: body.into(),
            source: 0,
        };
        assert_eq!(
            jot("\n## Release checklist\n- a").title(),
            "Release checklist"
        );
        assert_eq!(jot("```rust\nfn main() {}\n```").title(), "fn main() {}");
        assert_eq!(jot("").title(), "");
    }

    #[test]
    fn resolves_jot_numbers() {
        let file = PathBuf::from("/nonexistent/x.md");
        let mut store = Store::load(vec![file.clone()], &file, PathBuf::new(), false).unwrap();
        assert!(store.resolve("1").is_err());
        for body in ["a", "b", "c"] {
            store.jots.push(Jot {
                stamp: String::new(),
                body: body.into(),
                source: 0,
            });
        }
        assert_eq!(store.resolve("1").unwrap(), 0);
        assert_eq!(store.resolve("3").unwrap(), 2);
        assert_eq!(store.resolve("-1").unwrap(), 2);
        assert_eq!(store.resolve("-3").unwrap(), 0);
        assert_eq!(store.resolve("last").unwrap(), 2);
        for bad in ["0", "4", "-4", "x"] {
            assert!(store.resolve(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn project_root_is_git_root() {
        let dir = temp_dir("root");
        fs::create_dir_all(dir.join("repo/.git")).unwrap();
        fs::create_dir_all(dir.join("repo/src/deep")).unwrap();
        assert_eq!(project_root(&dir.join("repo/src/deep")), dir.join("repo"));
        assert_eq!(project_root(&dir), dir);
        fs::remove_dir_all(&dir).unwrap();
    }
}
