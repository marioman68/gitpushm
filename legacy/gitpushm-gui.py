#!/usr/bin/env python3
import os
import subprocess
import threading
import tkinter as tk
from tkinter import ttk, filedialog, simpledialog, messagebox
from pathlib import Path

VERSION = "2.2.0-GUI"
CONFIG_FILE = Path.home() / ".config" / "gitpushm.conf"

# Sleek Dark Purple / Magenta Palette
BG_DARK = "#12111a"
BG_PANEL = "#1a1826"
BG_HOVER = "#262338"
BG_INPUT = "#0d0c14"
ACCENT_PURPLE = "#9d65ff"
ACCENT_MAGENTA = "#ff66c4"
ACCENT_CYAN = "#38bdf8"
ACCENT_GREEN = "#4ade80"
ACCENT_YELLOW = "#facc15"
ACCENT_RED = "#f87171"
TEXT_MAIN = "#f1f5f9"
TEXT_MUTED = "#94a3b8"
BORDER_COLOR = "#312e47"


class GitPushManagerGUI(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title(f"❖ Git Project Push Manager (v{VERSION})")
        self.geometry("1040x680")
        self.minsize(860, 560)
        self.configure(bg=BG_DARK)

        self.projects_dir = self.load_config()
        self.projects = []
        self.selected_project = None

        self.setup_styles()
        self.build_ui()
        self.refresh_projects()

    # ------------------------------------------------------------------
    # Configuration & Data Helpers
    # ------------------------------------------------------------------
    def load_config(self) -> Path:
        if CONFIG_FILE.exists():
            raw = CONFIG_FILE.read_text().strip()
            if raw:
                return Path(os.path.expanduser(raw))
        default_dir = Path.home() / "Projects"
        self.save_config(default_dir)
        return default_dir

    def save_config(self, path: Path):
        CONFIG_FILE.parent.mkdir(parents=True, exist_ok=True)
        CONFIG_FILE.write_text(str(path))
        self.projects_dir = path

    def run_git(self, repo_path: Path, args: list[str]) -> tuple[int, str, str]:
        try:
            res = subprocess.run(
                ["git", "-C", str(repo_path), *args],
                capture_output=True,
                text=True,
                timeout=30,
            )
            return res.returncode, res.stdout.rstrip('\r\n'), res.stderr.rstrip('\r\n')
        except Exception as e:
            return 1, "", str(e)

    def generate_smart_commit_message(self, repo_path: Path) -> str:
        """Inspects git status and builds a clean, descriptive commit message."""
        _, status_out, _ = self.run_git(repo_path, ["status", "--porcelain"])
        if not status_out.strip():
            return "Sync repository state via gitpushm"

        added, modified, deleted, renamed = [], [], [], []
        for raw_line in status_out.splitlines():
            if len(raw_line) < 3:
                continue
            xy = raw_line[:2]
            filepath = raw_line[2:].lstrip()
            if " -> " in filepath:
                filepath = filepath.split(" -> ", 1)[1].strip()
            filepath = filepath.strip('"')
            fname = os.path.basename(filepath)
            if not fname:
                continue

            if "?" in xy or "A" in xy:
                added.append(fname)
            elif "D" in xy:
                deleted.append(fname)
            elif "R" in xy:
                renamed.append(fname)
            else:
                modified.append(fname)

        parts = []
        if modified:
            sample = ", ".join(modified[:3])
            extra = f" (+{len(modified) - 3} more)" if len(modified) > 3 else ""
            parts.append(f"Update {sample}{extra}")
        if added:
            sample = ", ".join(added[:3])
            extra = f" (+{len(added) - 3} more)" if len(added) > 3 else ""
            parts.append(f"Add {sample}{extra}")
        if deleted:
            sample = ", ".join(deleted[:2])
            extra = f" (+{len(deleted) - 2} more)" if len(deleted) > 2 else ""
            parts.append(f"Remove {sample}{extra}")
        if renamed:
            parts.append(f"Rename {len(renamed)} file(s)")

        return "; ".join(parts) if parts else "Update project files"

    def save_and_sync_files(self, repo_path: Path):
        """Flushes OS filesystem buffers before staging/committing."""
        try:
            os.sync()
        except AttributeError:
            pass

        if self.save_open_editors_var.get():
            try:
                subprocess.run(
                    ["wmctrl", "-l"], capture_output=True, text=True, timeout=2
                )
            except Exception:
                pass

    # ------------------------------------------------------------------
    # Styling & Layout
    # ------------------------------------------------------------------
    def setup_styles(self):
        style = ttk.Style(self)
        style.theme_use("clam")

        style.configure(
            "Treeview",
            background=BG_PANEL,
            foreground=TEXT_MAIN,
            fieldbackground=BG_PANEL,
            borderwidth=0,
            rowheight=32,
            font=("Sans", 10),
        )
        style.configure(
            "Treeview.Heading",
            background=BG_DARK,
            foreground=ACCENT_CYAN,
            borderwidth=0,
            font=("Sans", 10, "bold"),
        )
        style.map(
            "Treeview",
            background=[("selected", BG_HOVER)],
            foreground=[("selected", ACCENT_MAGENTA)],
        )

    def build_ui(self):
        # --- Top Header Bar ---
        header = tk.Frame(self, bg=BG_PANEL, height=58, padx=16)
        header.pack(fill="x", side="top")
        header.pack_propagate(False)

        tk.Label(
            header,
            text="◆ GIT PROJECT PUSH MANAGER",
            bg=BG_PANEL,
            fg=ACCENT_MAGENTA,
            font=("Sans", 13, "bold"),
        ).pack(side="left")

        btn_cfg = tk.Button(
            header,
            text="📁 Change Path",
            bg=BG_HOVER,
            fg=ACCENT_PURPLE,
            activebackground=ACCENT_PURPLE,
            activeforeground=BG_DARK,
            relief="flat",
            padx=12,
            pady=5,
            font=("Sans", 9, "bold"),
            cursor="hand2",
            command=self.change_projects_dir,
        )
        btn_cfg.pack(side="right", padx=(8, 0))

        btn_ref = tk.Button(
            header,
            text="↻ Refresh",
            bg=BG_HOVER,
            fg=ACCENT_CYAN,
            activebackground=ACCENT_CYAN,
            activeforeground=BG_DARK,
            relief="flat",
            padx=12,
            pady=5,
            font=("Sans", 9, "bold"),
            cursor="hand2",
            command=self.refresh_projects,
        )
        btn_ref.pack(side="right", padx=6)

        self.lbl_path = tk.Label(
            header,
            text="",
            bg=BG_PANEL,
            fg=TEXT_MUTED,
            font=("Monospace", 9),
        )
        self.lbl_path.pack(side="right", padx=12)

        tk.Frame(self, bg=ACCENT_PURPLE, height=2).pack(fill="x")

        # --- Main Split Container ---
        main_pane = tk.PanedWindow(
            self,
            orient="horizontal",
            bg=BG_DARK,
            sashwidth=6,
            sashrelief="flat",
        )
        main_pane.pack(fill="both", expand=True, padx=12, pady=12)

        # ================= LEFT PANE: PROJECT LIST =================
        left_frame = tk.Frame(
            main_pane,
            bg=BG_PANEL,
            highlightbackground=BORDER_COLOR,
            highlightthickness=1,
        )
        main_pane.add(left_frame, width=370)

        left_top = tk.Frame(left_frame, bg=BG_PANEL, padx=12, pady=10)
        left_top.pack(fill="x")

        self.lbl_dir_count = tk.Label(
            left_top,
            text="DETECTED DIRECTORIES (0)",
            bg=BG_PANEL,
            fg=ACCENT_CYAN,
            font=("Sans", 10, "bold"),
        )
        self.lbl_dir_count.pack(side="left")

        # Search Filter Box
        search_frame = tk.Frame(left_frame, bg=BG_PANEL, padx=12)
        search_frame.pack(fill="x", pady=(0, 8))
        self.search_var = tk.StringVar()
        self.search_var.trace_add("write", lambda *_: self.populate_tree())

        search_entry = tk.Entry(
            search_frame,
            textvariable=self.search_var,
            bg=BG_INPUT,
            fg=TEXT_MAIN,
            insertbackground=ACCENT_MAGENTA,
            relief="flat",
            font=("Sans", 10),
        )
        search_entry.pack(fill="x", ipady=5, padx=2)

        # Treeview for Projects
        tree_container = tk.Frame(left_frame, bg=BG_PANEL, padx=8)
        tree_container.pack(fill="both", expand=True)

        self.tree = ttk.Treeview(
            tree_container,
            columns=("status", "project", "branch"),
            show="headings",
            selectmode="browse",
        )
        self.tree.heading("status", text="State")
        self.tree.heading("project", text="Project Folder")
        self.tree.heading("branch", text="Branch")

        self.tree.column("status", width=55, anchor="center")
        self.tree.column("project", width=190, anchor="w")
        self.tree.column("branch", width=95, anchor="e")

        self.tree.tag_configure("dirty", foreground=ACCENT_GREEN)
        self.tree.tag_configure("clean", foreground=TEXT_MAIN)
        self.tree.tag_configure("nogit", foreground=ACCENT_RED)

        self.tree.pack(side="left", fill="both", expand=True)
        self.tree.bind("<<TreeviewSelect>>", self.on_project_select)

        scrollbar = ttk.Scrollbar(tree_container, orient="vertical", command=self.tree.yview)
        self.tree.configure(yscrollcommand=scrollbar.set)
        scrollbar.pack(side="right", fill="y")

        # Legend Footer
        legend = tk.Frame(left_frame, bg=BG_DARK, padx=12, pady=8)
        legend.pack(fill="x", side="bottom")
        tk.Label(
            legend,
            text="● Uncommitted    ○ Clean    ✕ Not a Git Repo",
            bg=BG_DARK,
            fg=TEXT_MUTED,
            font=("Sans", 9),
        ).pack(side="left")

        # ================= RIGHT PANE: ACTION CENTER =================
        right_frame = tk.Frame(
            main_pane,
            bg=BG_PANEL,
            highlightbackground=BORDER_COLOR,
            highlightthickness=1,
        )
        main_pane.add(right_frame)

        # Project Info Header
        info_box = tk.Frame(right_frame, bg=BG_PANEL, padx=16, pady=12)
        info_box.pack(fill="x")

        self.lbl_selected_title = tk.Label(
            info_box,
            text="Select a project to manage",
            bg=BG_PANEL,
            fg=TEXT_MAIN,
            font=("Sans", 13, "bold"),
        )
        self.lbl_selected_title.pack(anchor="w")

        self.lbl_selected_meta = tk.Label(
            info_box,
            text="Remote: —   |   Path: —",
            bg=BG_PANEL,
            fg=TEXT_MUTED,
            font=("Monospace", 9),
        )
        self.lbl_selected_meta.pack(anchor="w", pady=(4, 0))

        # Quick Action Toolbar
        toolbar = tk.Frame(right_frame, bg=BG_PANEL, padx=16, pady=4)
        toolbar.pack(fill="x")

        self.btn_open_fm = self.make_action_btn(toolbar, "📂 Open Folder", ACCENT_CYAN, self.open_in_file_manager)
        self.btn_open_fm.pack(side="left", padx=(0, 8))

        self.btn_pull = self.make_action_btn(toolbar, "⬇ Git Pull", ACCENT_PURPLE, self.pull_project)
        self.btn_pull.pack(side="left", padx=(0, 8))

        self.btn_init = self.make_action_btn(toolbar, "✨ Init & Link Remote", ACCENT_YELLOW, self.init_git_repo)
        self.btn_init.pack(side="left", padx=(0, 8))

        # Working Tree Status Box
        tk.Label(
            right_frame,
            text="GIT STATUS / MODIFIED FILES",
            bg=BG_PANEL,
            fg=ACCENT_CYAN,
            font=("Sans", 9, "bold"),
        ).pack(anchor="w", padx=16, pady=(12, 4))

        self.status_text = tk.Text(
            right_frame,
            height=6,
            bg=BG_INPUT,
            fg=TEXT_MAIN,
            relief="flat",
            font=("Monospace", 9),
            padx=10,
            pady=8,
            state="disabled",
        )
        self.status_text.pack(fill="x", padx=16)

        # Commit & Push Section
        commit_box = tk.Frame(right_frame, bg=BG_PANEL, padx=16, pady=12)
        commit_box.pack(fill="x")

        msg_header = tk.Frame(commit_box, bg=BG_PANEL)
        msg_header.pack(fill="x", pady=(0, 4))

        tk.Label(
            msg_header,
            text="COMMIT MESSAGE (leave blank to auto-generate on push)",
            bg=BG_PANEL,
            fg=ACCENT_MAGENTA,
            font=("Sans", 9, "bold"),
        ).pack(side="left")

        self.btn_autogen = tk.Button(
            msg_header,
            text="✨ Auto-Generate",
            bg=BG_HOVER,
            fg=ACCENT_PURPLE,
            activebackground=ACCENT_PURPLE,
            activeforeground=BG_DARK,
            relief="flat",
            font=("Sans", 8, "bold"),
            padx=8,
            pady=2,
            cursor="hand2",
            command=self.fill_auto_commit,
        )
        self.btn_autogen.pack(side="right")

        self.commit_entry = tk.Entry(
            commit_box,
            bg=BG_INPUT,
            fg=TEXT_MAIN,
            insertbackground=ACCENT_MAGENTA,
            relief="flat",
            font=("Sans", 10),
        )
        self.commit_entry.pack(fill="x", ipady=7)
        self.commit_entry.bind("<Return>", lambda _: self.push_project())

        push_row = tk.Frame(commit_box, bg=BG_PANEL)
        push_row.pack(fill="x", pady=(10, 0))

        self.btn_push = tk.Button(
            push_row,
            text="💾 Save, Auto-Commit & Push",
            bg=ACCENT_GREEN,
            fg="#09150e",
            activebackground="#22c55e",
            relief="flat",
            font=("Sans", 10, "bold"),
            padx=14,
            pady=8,
            cursor="hand2",
            command=self.push_project,
        )
        self.btn_push.pack(side="left")

        self.btn_force_push = tk.Button(
            push_row,
            text="⚡ Force Push",
            bg=BG_HOVER,
            fg=ACCENT_YELLOW,
            activebackground=ACCENT_YELLOW,
            activeforeground=BG_DARK,
            relief="flat",
            font=("Sans", 9, "bold"),
            padx=12,
            pady=8,
            cursor="hand2",
            command=self.force_push_project,
        )
        self.btn_force_push.pack(side="left", padx=(8, 0))

        options_row = tk.Frame(commit_box, bg=BG_PANEL)
        options_row.pack(fill="x", pady=(8, 0))

        self.auto_msg_var = tk.BooleanVar(value=True)
        tk.Checkbutton(
            options_row,
            text="Auto-fill message on select",
            variable=self.auto_msg_var,
            bg=BG_PANEL,
            fg=TEXT_MUTED,
            selectcolor=BG_INPUT,
            activebackground=BG_PANEL,
            activeforeground=TEXT_MAIN,
            font=("Sans", 9),
        ).pack(side="left", padx=(0, 12))

        self.force_push_var = tk.BooleanVar(value=False)
        tk.Checkbutton(
            options_row,
            text="Push even if working tree is clean",
            variable=self.force_push_var,
            bg=BG_PANEL,
            fg=TEXT_MUTED,
            selectcolor=BG_INPUT,
            activebackground=BG_PANEL,
            activeforeground=TEXT_MAIN,
            font=("Sans", 9),
        ).pack(side="left")

        self.save_open_editors_var = tk.BooleanVar(value=True)

        # Live Command Output Console
        tk.Label(
            right_frame,
            text="LIVE CONSOLE OUTPUT",
            bg=BG_PANEL,
            fg=TEXT_MUTED,
            font=("Sans", 9, "bold"),
        ).pack(anchor="w", padx=16, pady=(4, 4))

        self.console = tk.Text(
            right_frame,
            bg=BG_INPUT,
            fg=ACCENT_GREEN,
            relief="flat",
            font=("Monospace", 9),
            padx=10,
            pady=8,
            state="disabled",
        )
        self.console.pack(fill="both", expand=True, padx=16, pady=(0, 14))

    def make_action_btn(self, parent, label, color, cmd):
        return tk.Button(
            parent,
            text=label,
            bg=BG_HOVER,
            fg=color,
            activebackground=color,
            activeforeground=BG_DARK,
            relief="flat",
            font=("Sans", 9, "bold"),
            padx=10,
            pady=5,
            cursor="hand2",
            command=cmd,
        )

    # ------------------------------------------------------------------
    # Core Actions & Logic
    # ------------------------------------------------------------------
    def log(self, message: str):
        def _append():
            self.console.configure(state="normal")
            self.console.insert("end", message + "\n")
            self.console.see("end")
            self.console.configure(state="disabled")

        self.after(0, _append)

    def clear_console(self):
        self.console.configure(state="normal")
        self.console.delete("1.0", "end")
        self.console.configure(state="disabled")

    def fill_auto_commit(self):
        if not self.selected_project or not self.selected_project["is_git"]:
            return
        msg = self.generate_smart_commit_message(self.selected_project["path"])
        self.commit_entry.delete(0, "end")
        self.commit_entry.insert(0, msg)

    def change_projects_dir(self):
        chosen = filedialog.askdirectory(
            initialdir=str(self.projects_dir),
            title="Select Projects Folder",
        )
        if chosen:
            self.save_config(Path(chosen))
            self.refresh_projects()

    def refresh_projects(self):
        display_p = str(self.projects_dir).replace(str(Path.home()), "~")
        self.lbl_path.config(text=f"Path: {display_p}")

        self.projects.clear()
        if self.projects_dir.is_dir():
            for child in sorted(self.projects_dir.iterdir(), key=lambda p: p.name.lower()):
                if not child.is_dir() or child.name.startswith("."):
                    continue

                is_git = (child / ".git").is_dir()
                dirty = False
                branch = "—"
                remote = "None"

                if is_git:
                    _, status_out, _ = self.run_git(child, ["status", "--porcelain"])
                    dirty = bool(status_out)

                    _, br_out, _ = self.run_git(child, ["rev-parse", "--abbrev-ref", "HEAD"])
                    branch = br_out if br_out else "init"

                    _, rem_out, _ = self.run_git(child, ["remote", "get-url", "origin"])
                    remote = rem_out if rem_out else "No remote origin"

                self.projects.append(
                    {
                        "name": child.name,
                        "path": child,
                        "is_git": is_git,
                        "dirty": dirty,
                        "branch": branch,
                        "remote": remote,
                    }
                )

        self.lbl_dir_count.config(text=f"DETECTED DIRECTORIES ({len(self.projects)})")
        self.populate_tree()

    def populate_tree(self):
        query = self.search_var.get().strip().lower()
        for item in self.tree.get_children():
            self.tree.delete(item)

        for idx, proj in enumerate(self.projects):
            if query and query not in proj["name"].lower():
                continue

            if not proj["is_git"]:
                icon, tag = "✕", "nogit"
            elif proj["dirty"]:
                icon, tag = "●", "dirty"
            else:
                icon, tag = "○", "clean"

            self.tree.insert(
                "",
                "end",
                iid=str(idx),
                values=(icon, proj["name"], proj["branch"]),
                tags=(tag,),
            )

        if self.selected_project:
            for idx, proj in enumerate(self.projects):
                if proj["path"] == self.selected_project["path"] and self.tree.exists(str(idx)):
                    self.tree.selection_set(str(idx))
                    self.update_detail_pane(proj)
                    return

    def on_project_select(self, _event=None):
        sel = self.tree.selection()
        if not sel:
            return
        idx = int(sel[0])
        proj = self.projects[idx]
        self.selected_project = proj
        self.update_detail_pane(proj)

    def update_detail_pane(self, proj: dict):
        self.lbl_selected_title.config(text=f"◆ {proj['name']}")
        self.lbl_selected_meta.config(text=f"Branch: {proj['branch']}   |   Remote: {proj['remote']}")

        self.status_text.configure(state="normal")
        self.status_text.delete("1.0", "end")

        if not proj["is_git"]:
            self.status_text.insert(
                "end",
                "[WARNING] This directory is not a Git repository yet.\n"
                "Click '✨ Init & Link Remote' above to initialize Git and connect GitHub.",
            )
            self.btn_init.config(state="normal")
            self.btn_push.config(state="disabled")
            self.btn_force_push.config(state="disabled")
            self.btn_pull.config(state="disabled")
            self.commit_entry.delete(0, "end")
        else:
            self.btn_init.config(
                state="normal" if proj["remote"] == "No remote origin" else "disabled"
            )
            self.btn_push.config(state="normal")
            self.btn_force_push.config(state="normal")
            self.btn_pull.config(state="normal")

            _, status_out, _ = self.run_git(proj["path"], ["status", "-s"])
            if status_out:
                self.status_text.insert("end", status_out)
                if self.auto_msg_var.get():
                    self.fill_auto_commit()
            else:
                self.status_text.insert("end", "✔ Working tree clean — no uncommitted local changes.")
                if self.auto_msg_var.get():
                    self.commit_entry.delete(0, "end")

        self.status_text.configure(state="disabled")

    def open_in_file_manager(self):
        if not self.selected_project:
            messagebox.showinfo("No Selection", "Please select a project first.")
            return
        path = self.selected_project["path"]
        subprocess.Popen(["xdg-open", str(path)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.log(f"[ OK ] Opened '{path.name}' in system file manager.")

    def init_git_repo(self):
        if not self.selected_project:
            return
        proj = self.selected_project
        path = proj["path"]

        if not proj["is_git"]:
            if not messagebox.askyesno("Initialize Git", f"Initialize a new Git repository in '{proj['name']}'?"):
                return
            code, _, err = self.run_git(path, ["init"])
            if code != 0:
                messagebox.showerror("Git Init Failed", err)
                return
            self.log(f"[ OK ] Initialized Git repository in {proj['name']}")

        remote_url = simpledialog.askstring(
            "Remote Origin URL",
            f"Enter GitHub remote URL for '{proj['name']}'\n(e.g., git@github.com:user/{proj['name']}.git):",
            parent=self,
        )
        if remote_url and remote_url.strip():
            self.run_git(path, ["remote", "remove", "origin"])
            code, _, err = self.run_git(path, ["remote", "add", "origin", remote_url.strip()])
            if code == 0:
                self.log(f"[ OK ] Remote origin set to: {remote_url.strip()}")
            else:
                self.log(f"[ FAIL ] Could not add remote: {err}")

        self.refresh_projects()

    def pull_project(self):
        if not self.selected_project or not self.selected_project["is_git"]:
            return
        proj = self.selected_project
        self.clear_console()
        self.log(f"◆ Pulling latest changes for {proj['name']}...")

        def _worker():
            code, out, err = self.run_git(proj["path"], ["pull"])
            if out:
                self.log(out)
            if err:
                self.log(err)
            if code == 0:
                self.log(f"[ OK ] Pull completed for {proj['name']}!")
            else:
                self.log("[ FAIL ] Pull failed.")
            self.after(0, self.refresh_projects)

        threading.Thread(target=_worker, daemon=True).start()

    def _execute_push(self, force: bool = False):
        if not self.selected_project or not self.selected_project["is_git"]:
            return
        proj = self.selected_project
        path = proj["path"]

        if force:
            confirmed = messagebox.askyesno(
                "Confirm Force Push",
                f"Force-push '{proj['name']}' on branch '{proj['branch']}' using --force-with-lease?\n\n"
                "This can overwrite remote branch history if commits differ.",
                icon="warning",
            )
            if not confirmed:
                return

        user_msg = self.commit_entry.get().strip()
        self.commit_entry.delete(0, "end")
        self.clear_console()
        self.btn_push.config(state="disabled")
        self.btn_force_push.config(state="disabled")

        def _worker():
            mode_label = "Force-pushing" if force else "Managing"
            self.log(f"◆ {mode_label} project: {proj['name']}")
            self.log("[ INFO ] Flushing and saving local file buffers...")
            self.save_and_sync_files(path)

            _, status_out, _ = self.run_git(path, ["status", "--porcelain"])
            is_dirty = bool(status_out)

            if not is_dirty and not force and not self.force_push_var.get():
                self.log("[ INFO ] Working tree is clean — nothing to commit or push.")
                self.after(0, lambda: self.btn_push.config(state="normal"))
                self.after(0, lambda: self.btn_force_push.config(state="normal"))
                return

            if is_dirty:
                commit_msg = user_msg if user_msg else self.generate_smart_commit_message(path)
                self.log("[ INFO ] Staging all files (git add .)...")
                code, _, err = self.run_git(path, ["add", "."])
                if code != 0:
                    self.log(f"[ FAIL ] git add failed: {err}")
                    self.after(0, lambda: self.btn_push.config(state="normal"))
                    self.after(0, lambda: self.btn_force_push.config(state="normal"))
                    return

                self.log(f"[ INFO ] Committing: \"{commit_msg}\"")
                code, out, err = self.run_git(path, ["commit", "-m", commit_msg])
                if out:
                    self.log(out)
                if code != 0:
                    self.log(f"[ FAIL ] git commit failed: {err}")
                    self.after(0, lambda: self.btn_push.config(state="normal"))
                    self.after(0, lambda: self.btn_force_push.config(state="normal"))
                    return

            push_args = ["push", "--force-with-lease", "-u", "origin", "HEAD"] if force else ["push", "-u", "origin", "HEAD"]
            self.log(f"[ INFO ] Running git {' '.join(push_args)}...")
            code, out, err = self.run_git(path, push_args)
            if out:
                self.log(out)
            if err:
                self.log(err)

            if code == 0:
                done_msg = f"Force push completed for {proj['name']}! ⚡" if force else f"Saved, committed, and pushed {proj['name']}! 🚀"
                self.log(f"\n[ OK ] {done_msg}")
            else:
                self.log("\n[ FAIL ] Push failed. Verify your remote URL or SSH/token auth.")

            self.after(0, self.refresh_projects)
            self.after(0, lambda: self.btn_push.config(state="normal"))
            self.after(0, lambda: self.btn_force_push.config(state="normal"))

        threading.Thread(target=_worker, daemon=True).start()

    def push_project(self):
        self._execute_push(force=False)

    def force_push_project(self):
        self._execute_push(force=True)


if __name__ == "__main__":
    app = GitPushManagerGUI()
    app.mainloop()