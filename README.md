# jotty

[![CI](https://github.com/nicbet/jotty/actions/workflows/ci.yml/badge.svg)](https://github.com/nicbet/jotty/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/nicbet/jotty)](https://github.com/nicbet/jotty/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Quickly jot down thoughts scoped to a project — without leaving any files behind in it.

![jotty demo: quick capture, listing, and browsing jots](demo/demo.gif)

Jots belong to the enclosing git repository (or the current directory outside one), so they are shared across all of a repo's subdirectories. They are stored in `~/.config/jotty/`, one markdown file per project, named after the kebab-cased path:

```
~/Work/my-project  →  ~/.config/jotty/-users-me-work-my-project.md
```

## Usage

```
jotty                browse jots interactively
jotty new            write a new jot in the editor
jotty <text...>      save <text> as a jot right away
cmd | jotty          save piped input as a jot (e.g. `pbpaste | jotty`)
jotty ls             list jots
jotty show [i]       render all jots, or the i-th one
jotty edit [i]       edit the i-th jot (default: the newest)
jotty rm <i>         delete the i-th jot
jotty open           open the jot file in $VISUAL / $EDITOR
jotty all            list all projects that have jots
jotty path           print the jot file path
```

Run `jotty -g`, or add `-g` (`--global`) to `ls`, `show`, `edit`, `rm` or `path` to work with the jots of **all** projects at once, oldest first, with the project shown next to each jot: `jotty -g`, `jotty ls -g`, `jotty show -g -1`. Numbers then refer to that combined list. New jots always go to the current project, also when written from `jotty -g`.

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

When its output is piped, `jotty show` prints raw markdown instead of rendering it.

## Agent skill

`skills/jotty` is an [Agent Skills](https://agentskills.io) skill that teaches coding agents to write and read jots with the CLI ("jot that down", "what did I note about the sync worker?"). Install jotty, then link the skill into your agent's skills folder, e.g. for Claude Code:

```sh
ln -s "$PWD/skills/jotty" ~/.claude/skills/jotty
```

Agents use the non-interactive commands only (`jotty -- <text>`, piped input, `ls`, `show`, `rm`, `all`); `jotty` without arguments, `new`, `edit` and `open` refuse to run without a terminal.

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

Prebuilt binaries for Linux (x86_64, arm64), macOS (Intel, Apple Silicon) and Windows are attached to each [GitHub release](https://github.com/nicbet/jotty/releases). Or build from source (Rust 1.88+):

```sh
cargo install --git https://github.com/nicbet/jotty   # into ~/.cargo/bin
make install   # or, from a clone: builds release and copies to /usr/local/bin (requires sudo)
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

## License

[MIT](LICENSE)
