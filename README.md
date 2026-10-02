# jotty

Quickly jot down thoughts scoped to the current directory (e.g. a project) — without leaving any files behind in it.

Jots are stored in `~/.config/jotty/`, one markdown file per directory, named after the kebab-cased path:

```
~/Work/my-project  →  ~/.config/jotty/-users-me-work-my-project.md
```

## Usage

```
jotty          write a new jot
jotty ls       list jots for the current directory
jotty p        render all jots
jotty p <i>    render the i-th jot (1-based, as shown by `ls`)
jotty path     print the jot file path for the current directory
```

### Editor keys

| Key            | Action           |
|----------------|------------------|
| `Esc`/`Ctrl+S` | Save and quit    |
| `Ctrl+C`       | Discard and quit |

Empty jots are not saved.

## File format

Each jot is plain markdown wrapped in timestamp markers, so the file stays readable and editable by hand:

```markdown
<!-- 2026-10-02 14:57 -->
# Idea

Try **bold** and a list:
- one
- two
<!-- end -->
```

`$XDG_CONFIG_HOME` is honoured if set; otherwise `~/.config` is used.

## Install

```sh
make install   # builds release and copies to /usr/local/bin (requires sudo)
```

Optionally alias it:

```sh
echo 'alias j=jotty' >> ~/.zshrc
```

Run `make help` for the other targets.
