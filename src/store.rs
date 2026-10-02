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

/// The jots of one project, backed by a single markdown file.
pub struct Store {
    pub file: PathBuf,
    pub project: PathBuf,
    pub jots: Vec<Jot>,
}

/// Summary of one jot file, for `jotty all`.
pub struct Project {
    pub file: PathBuf,
    pub project: Option<PathBuf>,
    pub count: usize,
    pub latest: Option<String>,
}

impl Store {
    /// The store for the enclosing git repository, or the current directory outside of one.
    pub fn for_cwd() -> Result<Store> {
        let cwd = std::env::current_dir().context("cannot determine current directory")?;
        let project = project_root(&cwd);
        let file = config_dir()?.join(format!("{}.md", kebab(&project.to_string_lossy())));
        Store::open(file, project)
    }

    fn open(file: PathBuf, project: PathBuf) -> Result<Store> {
        let jots = read(&file)?.map(|t| parse(&t).jots).unwrap_or_default();
        Ok(Store {
            file,
            project,
            jots,
        })
    }

    /// Append a new jot, leaving the rest of the file untouched.
    pub fn add(&mut self, body: &str) -> Result<()> {
        let jot = Jot {
            stamp: Local::now().format(STAMP_FORMAT).to_string(),
            body: body.trim().to_string(),
        };
        let existing = read(&self.file)?.unwrap_or_default();
        let prefix = if existing.is_empty() {
            self.header()
        } else if !existing.ends_with('\n') {
            "\n\n".to_string()
        } else if !existing.ends_with("\n\n") {
            "\n".to_string()
        } else {
            String::new()
        };

        self.create_dir()?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file)
            .with_context(|| format!("opening {}", self.file.display()))?;
        write!(f, "{prefix}{}", entry(&jot))?;
        self.jots.push(jot);
        Ok(())
    }

    /// Rewrite the whole file from `self.jots`; removes it once the last jot is gone.
    pub fn save(&self) -> Result<()> {
        if self.jots.is_empty() {
            return match fs::remove_file(&self.file) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => {
                    Err(e).with_context(|| format!("removing {}", self.file.display()))
                }
                _ => Ok(()),
            };
        }
        let mut text = self.header();
        for jot in &self.jots {
            text.push_str(&entry(jot));
        }
        self.create_dir()?;
        let tmp = self.file.with_extension("md.tmp");
        fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
        fs::rename(&tmp, &self.file).with_context(|| format!("writing {}", self.file.display()))
    }

    /// Make sure the file exists, e.g. before handing it to `$EDITOR`.
    pub fn ensure_file(&self) -> Result<()> {
        if !self.file.exists() {
            self.create_dir()?;
            fs::write(&self.file, self.header())
                .with_context(|| format!("writing {}", self.file.display()))?;
        }
        Ok(())
    }

    /// Resolve a user-facing jot number to an index: `3` (1-based), `-1` (from the end) or `last`.
    pub fn resolve(&self, spec: &str) -> Result<usize> {
        let n = self.jots.len();
        if n == 0 {
            bail!("no jots yet ({})", self.file.display());
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

    fn header(&self) -> String {
        format!("<!-- {HEADER_PREFIX} {} -->\n\n", self.project.display())
    }

    fn create_dir(&self) -> Result<()> {
        if let Some(dir) = self.file.parent() {
            fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        Ok(())
    }
}

/// All projects with jots, most recently jotted first.
pub fn projects() -> Result<Vec<Project>> {
    let dir = config_dir()?;
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };
    let mut projects = Vec::new();
    for entry in entries {
        let file = entry?.path();
        if file.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
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
fn config_dir() -> Result<PathBuf> {
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

    #[test]
    fn add_save_and_reload() {
        let dir = temp_dir("store");
        let file = dir.join("t.md");
        let mut store = Store::open(file.clone(), PathBuf::from("/some/project")).unwrap();
        store.add("first\n\n- a\n- b\n").unwrap();
        store.add("second").unwrap();
        store.add("third").unwrap();

        let text = fs::read_to_string(&file).unwrap();
        assert!(text.starts_with("<!-- jotty: /some/project -->\n\n<!-- "));
        let parsed = parse(&text);
        assert_eq!(parsed.project, Some(PathBuf::from("/some/project")));
        let bodies: Vec<_> = parsed.jots.iter().map(|j| j.body.as_str()).collect();
        assert_eq!(bodies, ["first\n\n- a\n- b", "second", "third"]);

        store.jots.remove(1);
        store.jots[1].body = "third, edited".into();
        store.save().unwrap();
        let reloaded = Store::open(file.clone(), PathBuf::new()).unwrap();
        let bodies: Vec<_> = reloaded.jots.iter().map(|j| j.body.as_str()).collect();
        assert_eq!(bodies, ["first\n\n- a\n- b", "third, edited"]);

        store.jots.clear();
        store.save().unwrap();
        assert!(!file.exists());
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
        let mut store = Store::open(PathBuf::from("/nonexistent/x.md"), PathBuf::new()).unwrap();
        assert!(store.resolve("1").is_err());
        for body in ["a", "b", "c"] {
            store.jots.push(Jot {
                stamp: String::new(),
                body: body.into(),
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
