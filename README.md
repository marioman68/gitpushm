# gitpushm: Git Project Push Manager

A small desktop app that shows every project in your projects folder: which ones have uncommitted changes, which have commits waiting to be pushed, and which aren't git repos yet. Pick one, see what changed, get a commit message written for you, and push.

Version 3 is a rewrite in Rust (egui). The original Bash TUI and the Tkinter GUI are kept in `legacy/`.

## What it does

- **Project list:** every folder in `~/Projects` with a status dot (orange = changes, blue = unpushed or behind, green = clean, grey = not a repo or no remote), branch, change count and ↑/↓ counts. It has a filter (Ctrl+F) and loads in parallel.
- **Changes:** tick the files you want in the commit. Click a file to see its diff. Right-click a file to open it, copy its path, add it to `.gitignore`, stop tracking it, or discard its changes.
- **Commit message generator:** the message is written from the ticked files and updates as you tick and untick them.
  - It works out the kind of change (feat, docs, test, ci, build, deps, style, refactor, chore) and a scope from the shared folder.
  - Source files are named before lock files and configs. Renames, deletions, version bumps (`chore(release): bump version to 1.2.0`) and whitespace-only edits (`style:`) are recognised.
  - The subject is kept under 72 characters, with a body that lists every file and its +/- line counts.
  - Conventional style can be turned off in File > Preferences.
- **Safety:**
  - Files that look like secrets (`.env`, keys, `credentials.json`, …) or are too big for GitHub (>50/100 MB) are marked ⚠ and left unticked.
  - Force push uses `--force-with-lease` and asks first.
  - Pull is fast-forward only by default, or rebase if you prefer.
  - Git never stops to prompt for a password or editor: failures show in the Output panel with a plain explanation.
- **Also:**
  - **Branches:** switch branch, create a new one.
  - **Commits:** amend the last commit, undo the last commit (keeps your changes), see the history (unpushed commits in blue).
  - **Remotes and stash:** stash and restore, fetch all projects, set or change the remote, initialize a repository.
  - **Open:** the folder, a terminal, or the project's GitHub/GitLab page.
  - **Look:** dark/light/system theme. The project re-reads itself when you switch back to the window.

## Build and install

```bash
cd ~/Projects/gitpushm
cargo build --release
cp target/release/gitpushm ~/.local/bin/gitpushm
cp packaging/gitpushm.desktop ~/.local/share/applications/   # optional: app menu entry
for s in 32 48 64 128 256 512; do mkdir -p ~/.local/share/icons/hicolor/${s}x${s}/apps; cp assets/icon-$s.png ~/.local/share/icons/hicolor/${s}x${s}/apps/gitpushm.png; done   # app icon
```

## Command line

```
gitpushm                      open the window
gitpushm status               every project: branch, changes, ahead/behind
gitpushm msg <project>        print the commit message it would write
gitpushm push <project> [-m MESSAGE] [--dry-run]
```

`push` from the command line refuses to commit files that look like secrets.

## Keyboard

| Key | Action |
|---|---|
| Ctrl+Enter | Commit & push |
| Ctrl+Shift+Enter | Commit only |
| Ctrl+G | Generate message |
| F5 | Refresh |
| Ctrl+F | Filter projects |
| Alt+↑ / Alt+↓ | Previous / next project |
| Ctrl+1 / Ctrl+2 | Changes / History |

## Settings

The projects folder lives in `~/.config/gitpushm.conf`, the same file the older versions used. The other preferences are in `~/.config/gitpushm/settings`.
