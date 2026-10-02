# jotty

Quickly jot down thoughts scoped to a project — without leaving any files behind in it.

Jots belong to the enclosing git repository (or the current directory outside one), so they are shared across all of a repo's subdirectories. They are stored in `~/.config/jotty/`, one markdown file per project, named after the kebab-cased path:

```
~/Work/my-project  →  ~/.config/jotty/-users-me-work-my-project.md
```

## Usage

```
jotty                write a new jot in the editor
jotty <text...>      save <text> as a jot right away
cmd | jotty          save piped input as a jot (e.g. `pbpaste | jotty`)
jotty b              browse jots interactively
jotty ls             list jots
jotty p [i]          render all jots, or the i-th one
jotty e [i]          edit the i-th jot (default: the newest)
jotty rm <i>         delete the i-th jot
jotty open           open the jot file in $VISUAL / $EDITOR
jotty all            list all projects that have jots
jotty path           print the jot file path
```

`<i>` is 1-based as shown by `ls`; negative numbers count from the end, so `-1` (or `last`) is the newest jot.

Quick capture takes everything after `jotty` as the jot. If it starts with a command name or a dash, put `--` in front: `jotty -- ls is broken`.

### Editor keys

| Key            | Action           |
|----------------|------------------|
| `Esc`/`Ctrl+S` | Save and quit    |
| `Ctrl+C`       | Discard and quit |

Empty jots are not saved.

### Browser keys

| Key                | Action                                    |
|--------------------|-------------------------------------------|
| `↑`/`↓`, `j`/`k`   | Move (`g`/`G` first/last)                 |
| `Enter`/`e`        | Edit the selected jot                     |
| `n`                | Write a new jot                           |
| `d`                | Delete the selected jot (asks to confirm) |
| `y`                | Copy the selected jot to the clipboard    |
| `/`                | Filter (`Enter` keep, `Esc` clear)        |
| `PgUp`/`PgDn`      | Scroll the preview                        |
| `q`/`Esc`          | Quit                                      |

Copying uses `pbcopy`, `wl-copy`, `xclip`, `xsel` or `clip.exe`, falling back to the OSC 52 terminal escape sequence (which also works over SSH in most terminals).

## File format

Each jot is plain markdown wrapped in timestamp markers, so the file stays readable and editable by hand (`jotty open`):

```markdown
<!-- jotty: /Users/me/Work/my-project -->

<!-- 2026-10-02 14:57 -->
# Idea

Try **bold** and a list:
- one
- two
<!-- end -->
```

The header line records the project path for `jotty all`. Editing or deleting jots rewrites the file, which drops any text outside the markers.

`$XDG_CONFIG_HOME` is honoured if set; otherwise `~/.config` is used.

## Install

Prebuilt binaries for Linux (x86_64, arm64), macOS (Intel, Apple Silicon) and Windows are attached to each [GitHub release](https://github.com/nicbet/jotty/releases). Or build from source:

```sh
make install   # builds release and copies to /usr/local/bin (requires sudo)
```

Optionally alias it:

```sh
echo 'alias j=jotty' >> ~/.zshrc
```

Run `make help` for the other targets.

## Releasing

Push a version tag; CI tests on Linux/macOS/Windows, builds all targets and publishes a release:

```sh
git tag v0.1.0 && git push origin v0.1.0
```
