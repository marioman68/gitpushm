# ❖ Git Project Push Manager (`gitpushm`)

A lightweight, mouse-friendly Terminal User Interface (TUI) for managing local Git repositories, staging changes, committing updates, and pushing directly to GitHub. Built entirely in Bash, it features a sleek purple/magenta design and dual-pane layout.

## Features

* **Interactive TUI:** Clean dual-pane dashboard to browse your `~/Projects` directory instantly.
* **Full Mouse Support:** Click directly on any project row to select it, or click action buttons in the right pane.
* **Repository Health Status:** Real-time indicators showing uncommitted changes (**`●`**), clean working trees (**`○`**), or uninitialized directories (**`○`** in red).
* **Automated Git Initialization:** Automatically detects non-Git folders and prompts you to run `git init` and link your remote origin URL on the spot.
* **Universal File Manager Integration:** Open any project folder directly in your system's default graphical file manager (`xdg-open`).
* **Smart Commit & Push Workflows:** Custom commit message prompts with automatic branch tracking (`git push -u origin HEAD`).

## Prerequisites

* **Bash** (v4.0+)
* **Git** client installed and configured with your GitHub credentials (SSH keys or personal access tokens).
* **Xdg-utils:** Required for opening project folders in your desktop file manager.

## Installation

To install or update `gitpushm` locally in your user bin path:

```bash
# Clone the repository
git clone [https://github.com/marioman68/gitpushm.git](https://github.com/marioman68/gitpushm.git)
cd gitpushm

# Make executable and copy to local bin
chmod +x gitpushm
cp gitpushm ~/.local/bin/gitpushm

# Usage

Simply run the command from any terminal:
Bash

gitpushm

    Keyboard: Type the corresponding project number, or press a letter shortcut (p, o, s, r, q) in the Action Center.

    Mouse: Click on a project row in the left pane to manage it, or click any option on the right.

# Configuration

gitpushm automatically scans your local user directory located at:~/Projects/

Note: This project was collaboratively built and refined using AI assistance.
