---
name: jotty
description: Write and read project-scoped notes ("jots") with the jotty CLI. Use when the user asks to jot down, note, remember or save a thought, idea, todo or finding for this project, or asks what they jotted, to show, find or list their jots or notes, or to delete one. Jots are stored outside the repository (in ~/.config/jotty), scoped to the current git repository.
compatibility: Requires the jotty CLI on PATH (https://github.com/nicbet/jotty).
---

# jotty

`jotty` keeps markdown notes ("jots") per project without adding files to the project. All commands act on the jots of the git repository containing the current directory (or the current directory itself outside a repository), so run them from inside the project.

If `command -v jotty` finds nothing, tell the user jotty is not installed instead of improvising another notes file.

## Writing a jot

One line — always put `--` before the text, so text starting with a command name or a dash is not mistaken for a command:

```sh
jotty -- "Retry logic in the sync worker swallows timeouts"
```

Multiple lines — pipe markdown in on stdin with a quoted heredoc:

```sh
jotty <<'EOF'
## Release checklist
- bump the version in Cargo.toml
- tag and push
EOF
```

`jotty` prints `saved #N` on success. Guidelines:

- The first line is the jot's title in listings, so make it a short summary or a `##` heading.
- One jot per thought. Write it the way the user said it; do not add a signature or mark it as written by an agent.
- Never include a line that is exactly `<!-- end -->`; it terminates the jot.

## Reading jots

```sh
jotty ls        # one line per jot: number, timestamp, title (oldest first)
jotty p 3       # jot #3 as raw markdown
jotty p -1      # the newest jot (-2 the one before, ...)
jotty p         # every jot; can be long, prefer `ls` then `p <i>`
jotty all       # every project that has jots, with counts and paths
```

Jot numbers are 1-based and oldest first, exactly as `jotty ls` shows them. To search, grep the output: `jotty p | grep -n -i -B2 "timeout"`.

## Changing or deleting jots

Only delete when the user explicitly asks:

```sh
jotty rm 3
```

Deleting renumbers the jots after it, so run `jotty ls` again before deleting another.

There is no non-interactive edit command. To change a jot, edit the file printed by `jotty path` with your file editing tools. Keep its structure intact:

```markdown
<!-- jotty: /path/to/project -->

<!-- 2026-10-02 14:57 -->
markdown body of the jot
<!-- end -->
```

## Do not run

`jotty b`, `jotty e` and `jotty open` are interactive terminal UIs for the user; they exit with an error in a non-interactive shell. Plain `jotty` without arguments reads a jot from stdin, so only run it with input piped in.
