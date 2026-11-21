//! The window: projects on the left, the selected project's changes, diff and commit box on the right.
//! Everyday actions are buttons in view; everything else lives in the menu bar or a right-click menu.
use crate::commitmsg;
use crate::config::Config;
use crate::git::{self, Change, Commit, Kind};
use crate::ops;
use crate::scan::{self, Repo};
use eframe::egui::{self, text::LayoutJob, Align, Align2, Button, Color32, FontId, Key, Layout, Modifiers, RichText, Sense, TextFormat, Vec2};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};

const ACCENT: Color32 = Color32::from_rgb(118, 88, 224);
const ORANGE: Color32 = Color32::from_rgb(226, 146, 58);
const GREEN: Color32 = Color32::from_rgb(84, 176, 104);
const BLUE: Color32 = Color32::from_rgb(78, 146, 228);
const RED: Color32 = Color32::from_rgb(218, 82, 82);
const GREY: Color32 = Color32::from_gray(128);

enum Msg { Scan(Vec<Repo>), Repo(Repo), Done { what: String, ok: bool, log: Vec<String> } }
#[derive(PartialEq, Clone, Copy)]
enum Tab { Changes, History }
enum Dialog { ForcePush, Init, Remote(String), Discard(String, bool), Undo, NewBranch(String), About, Keys }
enum Act {
    Rescan(bool), Select(PathBuf), ShowDiff(String), Generate, Commit(bool), Push, Pull(bool), Fetch, Stash, Pop,
    Checkout(String), Ignore(String, bool), OpenFile(String), OpenFolder, Terminal, Web, PickFolder, Quit, SaveCfg, Open(Dialog),
    DoForcePush, DoInit, DoRemote(String), DoDiscard(String, bool), DoUndo, DoNewBranch(String), Move(i32),
}

pub struct App {
    cfg: Config, repos: Vec<Repo>, sel: Option<PathBuf>, search: String, only_changed: bool, scanning: bool,
    checked: HashSet<String>, known: HashSet<String>, diff_for: Option<String>, diff: String,
    msg: String, generated: String, amend: bool, tab: Tab, history: Vec<Commit>, branches: Vec<String>, stashes: usize,
    log: Vec<(String, u8)>, show_log: bool, status: String, busy: Option<String>,
    tx: Sender<Msg>, rx: Receiver<Msg>, dialog: Option<Dialog>, was_focused: bool, focus_search: bool, regen_needed: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, cfg: Config) -> App {
        let ctx = &cc.egui_ctx;
        for (theme, dark) in [(egui::Theme::Dark, true), (egui::Theme::Light, false)] {
            ctx.style_mut_of(theme, |s| {
                s.spacing.item_spacing = Vec2::new(8.0, 5.0);
                s.spacing.button_padding = Vec2::new(9.0, 3.0);
                s.visuals.selection.bg_fill = if dark { Color32::from_rgb(70, 56, 128) } else { Color32::from_rgb(214, 205, 250) };
                s.visuals.selection.stroke.color = if dark { Color32::from_rgb(236, 232, 255) } else { Color32::from_rgb(36, 22, 90) };
                s.visuals.hyperlink_color = if dark { Color32::from_rgb(160, 140, 255) } else { ACCENT };
                for w in [&mut s.visuals.widgets.inactive, &mut s.visuals.widgets.hovered, &mut s.visuals.widgets.active, &mut s.visuals.widgets.noninteractive] { w.rounding = egui::Rounding::same(4.0); }
            });
        }
        ctx.set_theme(theme_pref(&cfg.theme));
        add_fallback_fonts(ctx);
        let (tx, rx) = channel();
        let mut a = App { cfg, repos: vec![], sel: None, search: String::new(), only_changed: false, scanning: false,
            checked: HashSet::new(), known: HashSet::new(), diff_for: None, diff: String::new(), msg: String::new(), generated: String::new(), amend: false,
            tab: Tab::Changes, history: vec![], branches: vec![], stashes: 0, log: vec![], show_log: false, status: "Ready".into(), busy: None,
            tx, rx, dialog: None, was_focused: true, focus_search: false, regen_needed: false };
        let fetch = a.cfg.fetch_on_start; a.rescan(ctx, fetch);
        a
    }
    fn cur(&self) -> Option<&Repo> { let p = self.sel.as_ref()?; self.repos.iter().find(|r| &r.path == p) }
    fn visible(&self) -> Vec<usize> {
        let q = self.search.trim().to_lowercase();
        (0..self.repos.len()).filter(|&i| { let r = &self.repos[i]; (q.is_empty() || r.name.to_lowercase().contains(&q)) && (!self.only_changed || r.dirty() > 0 || r.ahead() > 0) }).collect()
    }

    fn rescan(&mut self, ctx: &egui::Context, fetch: bool) {
        if self.scanning { return; }
        self.scanning = true; self.status = if fetch { "Fetching from remotes…".into() } else { "Reading projects…".into() };
        let dir = self.cfg.projects.clone(); let tx = self.tx.clone(); let c = ctx.clone();
        std::thread::spawn(move || {
            if fetch {
                let paths: Vec<PathBuf> = scan::list(&dir).into_iter().filter(|p| p.join(".git").exists() && git::remote_url(p).is_some()).collect();
                for chunk in paths.chunks(8) { std::thread::scope(|s| { for p in chunk { s.spawn(move || git::git(p, &["fetch", "--quiet", "--prune"])); } }); }
            }
            let _ = tx.send(Msg::Scan(scan::scan(&dir))); c.request_repaint();
        });
    }
    fn reload_current(&self, ctx: &egui::Context) {
        let Some(p) = self.sel.clone() else { return }; if self.busy.is_some() { return; }
        let tx = self.tx.clone(); let c = ctx.clone();
        std::thread::spawn(move || { let _ = tx.send(Msg::Repo(scan::load(&p))); c.request_repaint(); });
    }
    fn select(&mut self, p: PathBuf) {
        if self.sel.as_ref() == Some(&p) { return; }
        self.sel = Some(p); self.checked.clear(); self.known.clear(); self.diff_for = None; self.diff.clear();
        self.msg.clear(); self.generated.clear(); self.amend = false; self.after_reload();
    }
    /// the selected repo just (re)loaded: keep the user's ticks, tick new files, refresh diff, history and the generated message
    fn after_reload(&mut self) {
        let Some(r) = self.cur().cloned() else { return };
        let Some(st) = &r.status else { self.history.clear(); self.branches.clear(); self.known.clear(); self.checked.clear(); return };
        let paths: HashSet<String> = st.changes.iter().map(|c| c.path.clone()).collect();
        for c in &st.changes { if !self.known.contains(&c.path) && commitmsg::risky(c).is_none() { self.checked.insert(c.path.clone()); } }
        self.checked.retain(|p| paths.contains(p)); self.known = paths;
        match self.diff_for.clone() { Some(d) if self.known.contains(&d) => self.load_diff(&d), Some(_) => { self.diff_for = None; self.diff.clear(); } None => {} }
        self.history = git::log(&r.path, 200); self.branches = git::branches(&r.path); self.stashes = git::stash_count(&r.path);
        if self.cfg.autofill && (self.msg.trim().is_empty() || self.msg == self.generated) { self.regen(); }
    }
    fn chosen(&self) -> Vec<Change> { self.cur().and_then(|r| r.status.as_ref()).map(|s| s.changes.iter().filter(|c| self.checked.contains(&c.path)).cloned().collect()).unwrap_or_default() }
    fn regen(&mut self) {
        let ch = self.chosen();
        let m = match self.cur() { Some(r) if !ch.is_empty() => match &r.status { Some(st) => ops::message(&r.path, st, &ch, self.cfg.conventional), None => String::new() }, _ => String::new() };
        self.msg = m.clone(); self.generated = m;
    }
    fn load_diff(&mut self, path: &str) {
        let d = self.cur().and_then(|r| { let st = r.status.as_ref()?; let c = st.changes.iter().find(|c| c.path == path)?; Some(git::file_diff(&r.path, c, st.no_commits)) });
        self.diff = d.unwrap_or_default();
    }

    fn job<F>(&mut self, ctx: &egui::Context, what: &str, f: F) where F: FnOnce(&Path, &mut Vec<String>) -> bool + Send + 'static {
        if self.busy.is_some() { return; }
        let Some(r) = self.cur() else { return };
        let (path, name, what) = (r.path.clone(), r.name.clone(), what.to_string());
        self.busy = Some(format!("{what} · {name}"));
        self.log.push((format!("── {what} · {name}"), 1));
        let tx = self.tx.clone(); let c = ctx.clone();
        std::thread::spawn(move || {
            let mut log = vec![]; let ok = f(&path, &mut log);
            let _ = tx.send(Msg::Done { what, ok, log }); let _ = tx.send(Msg::Repo(scan::load(&path))); c.request_repaint();
        });
    }
    fn gitjob(&mut self, ctx: &egui::Context, what: &str, args: &[&str]) {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        self.job(ctx, what, move |p, log| { let o = git::git_owned(p, &args); let t = o.text(); if !t.trim().is_empty() { log.push(t); } o.ok });
    }
    fn do_commit(&mut self, ctx: &egui::Context, push: bool) {
        let Some(r) = self.cur() else { return }; let Some(st) = &r.status else { return };
        let chosen = self.chosen(); let msg = self.msg.trim().to_string(); let amend = self.amend;
        if msg.is_empty() || (chosen.is_empty() && !amend) { return; }
        if push && r.remote.is_none() { self.dialog = Some(Dialog::Remote(String::new())); return; }
        let all = chosen.len() == st.changes.len();
        self.job(ctx, if push { "Commit & push" } else { "Commit" }, move |p, log| ops::commit(p, &chosen, all, &msg, amend, log) && (!push || ops::push(p, false, log)));
    }

    fn run(&mut self, ctx: &egui::Context, a: Act) {
        let path = self.cur().map(|r| r.path.clone());
        match a {
            Act::Rescan(f) => self.rescan(ctx, f),
            Act::Select(p) => self.select(p),
            Act::Move(d) => { let v = self.visible(); if v.is_empty() { return; } let at = v.iter().position(|&i| Some(&self.repos[i].path) == self.sel.as_ref()); let n = match at { Some(k) => (k as i32 + d).clamp(0, v.len() as i32 - 1) as usize, None => 0 }; let p = self.repos[v[n]].path.clone(); self.select(p); }
            Act::ShowDiff(p) => { self.diff_for = Some(p.clone()); self.load_diff(&p); }
            Act::Generate => self.regen(),
            Act::Commit(push) => self.do_commit(ctx, push),
            Act::Push => { if self.cur().map_or(false, |r| r.remote.is_none()) { self.dialog = Some(Dialog::Remote(String::new())); } else { self.job(ctx, "Push", |p, log| ops::push(p, false, log)); } }
            Act::DoForcePush => self.job(ctx, "Force push", |p, log| ops::push(p, true, log)),
            Act::Pull(rebase) => self.job(ctx, "Pull", move |p, log| ops::pull(p, rebase, log)),
            Act::Fetch => self.gitjob(ctx, "Fetch", &["fetch", "--prune"]),
            Act::Stash => self.gitjob(ctx, "Stash", &["stash", "push", "-u", "-m", "gitpushm stash"]),
            Act::Pop => self.gitjob(ctx, "Pop stash", &["stash", "pop"]),
            Act::Checkout(b) => self.gitjob(ctx, "Switch branch", &["switch", &b]),
            Act::DoNewBranch(b) => self.gitjob(ctx, "New branch", &["switch", "-c", b.trim()]),
            Act::DoUndo => self.gitjob(ctx, "Undo last commit", &["reset", "--soft", "HEAD~1"]),
            Act::DoInit => self.job(ctx, "Initialize", |p, log| { let mut o = git::git(p, &["init", "-b", "main"]); if !o.ok { o = git::git(p, &["init"]); } log.push(o.text()); o.ok }),
            Act::DoRemote(url) => { let url = url.trim().to_string(); if url.is_empty() { return; }
                self.job(ctx, "Set remote", move |p, log| { let o = if git::remote_url(p).is_some() { git::git(p, &["remote", "set-url", "origin", &url]) } else { git::git(p, &["remote", "add", "origin", &url]) }; log.push(if o.ok { format!("origin → {url}") } else { o.text() }); o.ok }); }
            Act::DoDiscard(f, new) => self.job(ctx, "Discard", move |p, log| {
                if new { let _ = git::git(p, &["rm", "-q", "--cached", "--ignore-unmatch", "--", &f]); let fp = p.join(&f);
                    let r = if fp.is_dir() { std::fs::remove_dir_all(&fp) } else { std::fs::remove_file(&fp) }; match r { Ok(_) => { log.push(format!("deleted {f}")); true } Err(e) => { log.push(format!("could not delete {f}: {e}")); false } } }
                else { let o = git::git(p, &["restore", "--source=HEAD", "--staged", "--worktree", "--", &f]); log.push(if o.ok { format!("restored {f}") } else { o.text() }); o.ok }
            }),
            Act::Ignore(f, tracked) => self.job(ctx, "Ignore", move |p, log| {
                let gi = p.join(".gitignore"); let mut t = std::fs::read_to_string(&gi).unwrap_or_default();
                let line = format!("/{f}"); if !t.lines().any(|l| l.trim() == line) { if !t.is_empty() && !t.ends_with('\n') { t.push('\n'); } t.push_str(&line); t.push('\n'); }
                if let Err(e) = std::fs::write(&gi, t) { log.push(format!("could not write .gitignore: {e}")); return false; }
                if tracked { let _ = git::git(p, &["rm", "-q", "--cached", "--", &f]); log.push(format!("{f} is no longer tracked")); }
                log.push(format!("added {line} to .gitignore")); true
            }),
            Act::OpenFile(f) => { if let Some(p) = path { open(&p.join(f)); } }
            Act::OpenFolder => { if let Some(p) = path { open(&p); } }
            Act::Terminal => { if let Some(p) = path { terminal(&p); } }
            Act::Web => { if let Some(u) = self.cur().and_then(|r| r.remote.as_deref()).and_then(git::web_url) { open(Path::new(&u)); } }
            Act::PickFolder => { if let Some(d) = rfd::FileDialog::new().set_title("Projects folder").set_directory(&self.cfg.projects).pick_folder() { self.cfg.projects = d; self.cfg.save(); self.sel = None; self.repos.clear(); self.rescan(ctx, false); } }
            Act::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Act::SaveCfg => { self.cfg.save(); ctx.set_theme(theme_pref(&self.cfg.theme)); if self.msg == self.generated { self.regen(); } }
            Act::Open(d) => self.dialog = Some(d),
        }
    }
}

/// egui's built-in font lacks arrows like → ↑ ⎇; borrow them from a system font if one is installed
fn add_fallback_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default(); let mut n = 0;
    for path in ["/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/usr/share/fonts/google-noto/NotoSansSymbols2-Regular.ttf", "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf", "/usr/share/fonts/google-noto/NotoSansMath-Regular.ttf"] {
        if let Ok(bytes) = std::fs::read(path) {
            let key = format!("fallback{n}"); n += 1;
            fonts.font_data.insert(key.clone(), egui::FontData::from_owned(bytes));
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] { fonts.families.entry(fam).or_default().push(key.clone()); }
        }
    }
    if n > 0 { ctx.set_fonts(fonts); }
}
fn theme_pref(t: &str) -> egui::ThemePreference { match t { "dark" => egui::ThemePreference::Dark, "light" => egui::ThemePreference::Light, _ => egui::ThemePreference::System } }
fn open(p: &Path) { let _ = std::process::Command::new("xdg-open").arg(p).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn(); }
fn terminal(p: &Path) {
    let d = p.to_string_lossy().to_string();
    let tries: [(&str, Vec<String>); 6] = [("konsole", vec!["--workdir".into(), d.clone()]), ("gnome-terminal", vec![format!("--working-directory={d}")]), ("xfce4-terminal", vec![format!("--working-directory={d}")]),
        ("kitty", vec!["--directory".into(), d.clone()]), ("alacritty", vec!["--working-directory".into(), d.clone()]), ("x-terminal-emulator", vec![])];
    for (t, a) in tries { if std::process::Command::new(t).args(&a).current_dir(p).spawn().is_ok() { return; } }
}
fn repo_state(r: &Repo) -> (Color32, String) {
    if !r.is_git { return (GREY, "Not a git repository".into()); }
    if let Some(e) = &r.error { return (RED, e.lines().next().unwrap_or("git error").to_string()); }
    if r.dirty() > 0 { return (ORANGE, format!("{} uncommitted change{}", r.dirty(), if r.dirty() == 1 { "" } else { "s" })); }
    if r.ahead() > 0 { return (BLUE, format!("{} commit{} not pushed", r.ahead(), if r.ahead() == 1 { "" } else { "s" })); }
    if r.behind() > 0 { return (BLUE, format!("{} commit{} on the remote to pull", r.behind(), if r.behind() == 1 { "" } else { "s" })); }
    if r.remote.is_none() { return (GREY, "Clean, no remote set".into()); }
    (GREEN, "Clean and up to date".into())
}
fn repo_sub(r: &Repo) -> String {
    if !r.is_git { return "not a git repo".into(); }
    let mut v = vec![r.branch()];
    if r.dirty() > 0 { v.push(format!("{} changed", r.dirty())); }
    if r.ahead() > 0 { v.push(format!("↑{}", r.ahead())); }
    if r.behind() > 0 { v.push(format!("↓{}", r.behind())); }
    if r.remote.is_none() { v.push("no remote".into()); }
    v.join("  ·  ")
}
fn kind_color(k: &Kind) -> Color32 { match k { Kind::Added | Kind::Untracked => GREEN, Kind::Deleted => RED, Kind::Conflict => RED, Kind::Renamed(_) => BLUE, _ => ORANGE } }
fn counts(c: &Change) -> String { if c.binary { "binary".into() } else if c.adds + c.dels == 0 { String::new() } else { format!("+{}  −{}", c.adds, c.dels) } }
fn diff_job(text: &str, ui: &egui::Ui) -> LayoutJob {
    let base = ui.visuals().text_color(); let dark = ui.visuals().dark_mode;
    let font = FontId::monospace(12.5); let mut job = LayoutJob::default(); job.wrap.max_width = f32::INFINITY;
    for (n, l) in text.lines().enumerate() {
        if n >= 4000 { job.append("… (diff cut off at 4000 lines)\n", 0.0, TextFormat::simple(font.clone(), GREY)); break; }
        let (col, bg) = if l.starts_with("+++") || l.starts_with("---") || l.starts_with("diff ") || l.starts_with("index ") { (GREY, Color32::TRANSPARENT) }
            else if l.starts_with('+') { (if dark { Color32::from_rgb(140, 214, 150) } else { Color32::from_rgb(20, 110, 40) }, if dark { Color32::from_rgb(30, 50, 36) } else { Color32::from_rgb(226, 244, 230) }) }
            else if l.starts_with('-') { (if dark { Color32::from_rgb(236, 140, 140) } else { Color32::from_rgb(160, 30, 30) }, if dark { Color32::from_rgb(58, 32, 34) } else { Color32::from_rgb(250, 228, 228) }) }
            else if l.starts_with("@@") { (BLUE, Color32::TRANSPARENT) } else { (base, Color32::TRANSPARENT) };
        job.append(&format!("{l}\n"), 0.0, TextFormat { font_id: font.clone(), color: col, background: bg, ..Default::default() });
    }
    job
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Scan(v) => {
                    self.repos = v; self.scanning = false;
                    let n = self.repos.iter().filter(|r| r.dirty() > 0).count(); let up = self.repos.iter().filter(|r| r.ahead() > 0).count();
                    self.status = format!("{} projects · {n} with changes · {up} with unpushed commits", self.repos.len());
                    if self.sel.as_ref().map_or(false, |p| !self.repos.iter().any(|r| &r.path == p)) { self.sel = None; }
                    self.after_reload();
                    // GITPUSHM_SELECT=name[:file] opens a project (and a file's diff) at start
                    if self.sel.is_none() { if let Ok(s) = std::env::var("GITPUSHM_SELECT") { let (n, f) = s.split_once(':').map(|(a, b)| (a.to_string(), Some(b.to_string()))).unwrap_or((s.clone(), None));
                        if let Some(p) = self.repos.iter().find(|r| r.name == n).map(|r| r.path.clone()) { self.select(p); if let Some(f) = f { self.diff_for = Some(f.clone()); self.load_diff(&f); } } } }
                }
                Msg::Repo(r) => { let is_sel = self.sel.as_ref() == Some(&r.path); if let Some(x) = self.repos.iter_mut().find(|x| x.path == r.path) { *x = r; } if is_sel { self.after_reload(); } }
                Msg::Done { what, ok, log } => {
                    self.busy = None;
                    for l in log { for line in l.lines() { if !line.trim().is_empty() { self.log.push((line.to_string(), if ok { 0 } else { 2 })); } } }
                    self.log.push((if ok { format!("✓ {what} done") } else { format!("✗ {what} failed") }, if ok { 3 } else { 2 }));
                    if self.log.len() > 3000 { self.log.drain(..self.log.len() - 3000); }
                    self.status = if ok { format!("{what} done") } else { format!("{what} failed. See Output.") };
                    if !ok { self.show_log = true; }
                    if ok && what.starts_with("Commit") { self.msg.clear(); self.generated.clear(); self.amend = false; }
                    if ok && what == "Initialize" { self.dialog = Some(Dialog::Remote(String::new())); }
                }
            }
        }
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if focused && !self.was_focused { self.reload_current(ctx); }   // pick up edits made in other programs
        self.was_focused = focused;

        let mut acts: Vec<Act> = vec![];
        self.keys(ctx, &mut acts);
        self.menu(ctx, &mut acts);
        self.statusbar(ctx);
        if self.show_log { self.output(ctx); }
        self.project_list(ctx, &mut acts);
        self.main(ctx, &mut acts);
        self.dialogs(ctx, &mut acts);
        if std::mem::take(&mut self.regen_needed) && self.msg == self.generated { self.regen(); }
        for a in acts { self.run(ctx, a); }
    }
}

impl App {
    fn keys(&mut self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        ctx.input_mut(|i| {
            if self.dialog.is_some() { if i.consume_key(Modifiers::NONE, Key::Escape) { self.dialog = None; } return; }
            if i.consume_key(Modifiers::NONE, Key::F5) { acts.push(Act::Rescan(false)); }
            if i.consume_key(Modifiers::COMMAND, Key::Enter) { acts.push(Act::Commit(true)); }
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Enter) { acts.push(Act::Commit(false)); }
            if i.consume_key(Modifiers::COMMAND, Key::G) { acts.push(Act::Generate); }
            if i.consume_key(Modifiers::COMMAND, Key::F) { self.focus_search = true; }
            if i.consume_key(Modifiers::COMMAND, Key::Q) { acts.push(Act::Quit); }
            if i.consume_key(Modifiers::COMMAND, Key::Num1) { self.tab = Tab::Changes; }
            if i.consume_key(Modifiers::COMMAND, Key::Num2) { self.tab = Tab::History; }
            if i.consume_key(Modifiers::ALT, Key::ArrowUp) { acts.push(Act::Move(-1)); }
            if i.consume_key(Modifiers::ALT, Key::ArrowDown) { acts.push(Act::Move(1)); }
        });
    }

    fn menu(&mut self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        let r = self.cur().cloned(); let git_ok = r.as_ref().map_or(false, |r| r.is_git && r.status.is_some()); let idle = self.busy.is_none();
        let has_remote = r.as_ref().map_or(false, |r| r.remote.is_some());
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Projects folder…").clicked() { acts.push(Act::PickFolder); ui.close_menu(); }
                    if ui.add(Button::new("Refresh").shortcut_text("F5")).clicked() { acts.push(Act::Rescan(false)); ui.close_menu(); }
                    if ui.button("Fetch all projects").on_hover_text("Ask every remote what is new, so ↓ counts are right").clicked() { acts.push(Act::Rescan(true)); ui.close_menu(); }
                    ui.separator();
                    ui.menu_button("Preferences", |ui| {
                        let mut ch = false;
                        ch |= ui.checkbox(&mut self.cfg.conventional, "Conventional commit style (feat:, fix:, docs: …)").changed();
                        ch |= ui.checkbox(&mut self.cfg.autofill, "Write the message automatically").changed();
                        ch |= ui.checkbox(&mut self.cfg.fetch_on_start, "Fetch all projects at start").changed();
                        ch |= ui.checkbox(&mut self.cfg.pull_rebase, "Pull with rebase").changed();
                        if ch { acts.push(Act::SaveCfg); }
                    });
                    ui.separator();
                    if ui.add(Button::new("Quit").shortcut_text("Ctrl+Q")).clicked() { acts.push(Act::Quit); }
                });
                ui.add_enabled_ui(r.is_some(), |ui| ui.menu_button("Repository", |ui| {
                    let ok = git_ok && idle; let rb = self.cfg.pull_rebase;
                    if ui.add_enabled(ok && has_remote, Button::new("Pull")).clicked() { acts.push(Act::Pull(rb)); ui.close_menu(); }
                    if ui.add_enabled(ok && has_remote, Button::new(if rb { "Pull (merge, fast-forward only)" } else { "Pull with rebase" })).clicked() { acts.push(Act::Pull(!rb)); ui.close_menu(); }
                    if ui.add_enabled(ok && has_remote, Button::new("Fetch")).clicked() { acts.push(Act::Fetch); ui.close_menu(); }
                    if ui.add_enabled(ok, Button::new("Push")).clicked() { acts.push(Act::Push); ui.close_menu(); }
                    if ui.add_enabled(ok && has_remote, Button::new("Force push…")).clicked() { acts.push(Act::Open(Dialog::ForcePush)); ui.close_menu(); }
                    ui.separator();
                    let dirty = r.as_ref().map_or(0, |r| r.dirty());
                    if ui.add_enabled(ok && dirty > 0, Button::new("Stash changes")).on_hover_text("Put your changes aside and clean the working tree").clicked() { acts.push(Act::Stash); ui.close_menu(); }
                    if ui.add_enabled(ok && self.stashes > 0, Button::new(format!("Restore stash ({})", self.stashes))).clicked() { acts.push(Act::Pop); ui.close_menu(); }
                    if ui.add_enabled(ok && !self.history.is_empty(), Button::new("Undo last commit…")).clicked() { acts.push(Act::Open(Dialog::Undo)); ui.close_menu(); }
                    if ui.add_enabled(ok, Button::new("New branch…")).clicked() { acts.push(Act::Open(Dialog::NewBranch(String::new()))); ui.close_menu(); }
                    ui.separator();
                    let is_git = r.as_ref().map_or(false, |r| r.is_git);
                    if ui.add_enabled(idle && !is_git, Button::new("Initialize repository…")).clicked() { acts.push(Act::Open(Dialog::Init)); ui.close_menu(); }
                    if ui.add_enabled(idle && is_git, Button::new(if has_remote { "Change remote…" } else { "Set remote…" })).clicked() { acts.push(Act::Open(Dialog::Remote(r.as_ref().and_then(|r| r.remote.clone()).unwrap_or_default()))); ui.close_menu(); }
                    ui.separator();
                    if ui.button("Open folder").clicked() { acts.push(Act::OpenFolder); ui.close_menu(); }
                    if ui.button("Open terminal here").clicked() { acts.push(Act::Terminal); ui.close_menu(); }
                    let web = r.as_ref().and_then(|r| r.remote.as_deref()).and_then(git::web_url).is_some();
                    if ui.add_enabled(web, Button::new("Open on the web")).clicked() { acts.push(Act::Web); ui.close_menu(); }
                }));
                ui.menu_button("View", |ui| {
                    if ui.add(egui::SelectableLabel::new(self.tab == Tab::Changes, "Changes  ")).clicked() { self.tab = Tab::Changes; ui.close_menu(); }
                    if ui.add(egui::SelectableLabel::new(self.tab == Tab::History, "History  ")).clicked() { self.tab = Tab::History; ui.close_menu(); }
                    ui.separator();
                    ui.checkbox(&mut self.only_changed, "Only projects with changes");
                    ui.checkbox(&mut self.show_log, "Output panel");
                    ui.separator();
                    ui.menu_button("Theme", |ui| { for (k, l) in [("system", "Follow system"), ("dark", "Dark"), ("light", "Light")] { if ui.radio(self.cfg.theme == k, l).clicked() { self.cfg.theme = k.into(); acts.push(Act::SaveCfg); ui.close_menu(); } } });
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("Keyboard shortcuts").clicked() { acts.push(Act::Open(Dialog::Keys)); ui.close_menu(); }
                    if ui.button("About gitpushm").clicked() { acts.push(Act::Open(Dialog::About)); ui.close_menu(); }
                });
            });
        });
    }

    fn statusbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status").exact_height(26.0).show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                if let Some(b) = &self.busy { ui.add(egui::Spinner::new().size(13.0)); ui.label(format!("{b}…")); }
                else { let failed = self.status.ends_with("See Output."); ui.label(RichText::new(&self.status).color(if failed { RED } else { ui.visuals().weak_text_color() })); }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.toggle_value(&mut self.show_log, "Output");
                    let home = std::env::var("HOME").unwrap_or_default(); let d = self.cfg.projects.to_string_lossy().replacen(&home, "~", 1);
                    ui.label(RichText::new(d).weak().small());
                });
            });
        });
    }

    fn output(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("output").resizable(true).default_height(160.0).height_range(80.0..=500.0).show(ctx, |ui| {
            ui.horizontal(|ui| { ui.label(RichText::new("Output").strong()); ui.with_layout(Layout::right_to_left(Align::Center), |ui| { if ui.small_button("Clear").clicked() { self.log.clear(); } }); });
            egui::ScrollArea::vertical().auto_shrink([false; 2]).stick_to_bottom(true).show(ui, |ui| {
                if self.log.is_empty() { ui.label(RichText::new("Git output shows here.").weak()); }
                for (l, k) in &self.log {
                    let t = RichText::new(l).monospace().size(12.0);
                    ui.label(match k { 1 => t.strong(), 2 => t.color(RED), 3 => t.color(GREEN), _ => t });
                }
            });
        });
    }

    fn project_list(&mut self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        egui::SidePanel::left("projects").resizable(true).default_width(290.0).width_range(220.0..=480.0).show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Projects").strong()); ui.label(RichText::new(self.repos.len().to_string()).weak());
                if self.scanning { ui.add(egui::Spinner::new().size(12.0)); }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| { if ui.small_button("⟳").on_hover_text("Refresh (F5)").clicked() { acts.push(Act::Rescan(false)); } });
            });
            let te = ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Filter  (Ctrl+F)").desired_width(f32::INFINITY));
            if std::mem::take(&mut self.focus_search) { te.request_focus(); }
            ui.add_space(2.0); ui.separator();
            let vis = self.visible();
            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                if vis.is_empty() && !self.scanning { ui.add_space(12.0); ui.label(RichText::new(if self.repos.is_empty() { "No folders here. Pick your projects folder in File > Projects folder…" } else { "Nothing matches the filter." }).weak()); }
                for i in vis {
                    let r = &self.repos[i];
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::click());
                    let v = ui.visuals().clone(); let sel = self.sel.as_ref() == Some(&r.path);
                    if sel { ui.painter().rect_filled(rect, 4.0, v.selection.bg_fill); } else if resp.hovered() { ui.painter().rect_filled(rect, 4.0, v.widgets.hovered.weak_bg_fill); }
                    let (dot, tip) = repo_state(r);
                    ui.painter().circle_filled(rect.left_center() + Vec2::new(12.0, 0.0), 4.5, dot);
                    let p = ui.painter_at(rect.shrink2(Vec2::new(4.0, 0.0)));
                    p.text(rect.left_top() + Vec2::new(26.0, 5.0), Align2::LEFT_TOP, &r.name, FontId::proportional(14.0), if sel { v.selection.stroke.color } else { v.strong_text_color() });
                    p.text(rect.left_top() + Vec2::new(26.0, 23.0), Align2::LEFT_TOP, repo_sub(r), FontId::proportional(11.5), if sel { v.selection.stroke.color.gamma_multiply(0.75) } else { v.weak_text_color() });
                    if resp.clicked() { acts.push(Act::Select(r.path.clone())); }
                    resp.on_hover_text(tip);
                }
            });
        });
    }

    fn main(&mut self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(r) = self.cur().cloned() else {
                ui.centered_and_justified(|ui| ui.label(RichText::new(if self.scanning { "Reading projects…" } else { "Select a project on the left" }).weak().size(15.0)));
                return;
            };
            let idle = self.busy.is_none();
            // header: name, branch, remote; pull and push on the right
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(&r.name).size(19.0).strong());
                    ui.horizontal(|ui| {
                        if let Some(st) = &r.status {
                            egui::ComboBox::from_id_salt("branch").selected_text(format!("⎇ {}", st.branch)).show_ui(ui, |ui| {
                                for b in &self.branches { if ui.selectable_label(b == &st.branch, b).clicked() && b != &st.branch { acts.push(Act::Checkout(b.clone())); } }
                                ui.separator(); if ui.button("New branch…").clicked() { acts.push(Act::Open(Dialog::NewBranch(String::new()))); }
                            }).response.on_hover_text("Switch branch");
                        }
                        match &r.remote {
                            Some(u) => { let web = git::web_url(u); let l = ui.add(egui::Label::new(RichText::new(u).weak()).sense(Sense::click())); if web.is_some() { if l.on_hover_text("Open on the web").clicked() { acts.push(Act::Web); } } }
                            None if r.is_git => { ui.label(RichText::new("No remote").color(ORANGE)); if ui.small_button("Set remote…").clicked() { acts.push(Act::Open(Dialog::Remote(String::new()))); } }
                            None => {}
                        }
                    });
                });
                if let Some(st) = &r.status {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let push = if st.ahead > 0 { format!("Push  ↑{}", st.ahead) } else { "Push".into() };
                        let tip = if r.remote.is_none() { "Set a remote first".to_string() } else if st.upstream.is_none() { "This branch isn't on the remote yet; pushing creates it".into() } else { format!("{} commit(s) to send", st.ahead) };
                        if ui.add_enabled(idle, Button::new(push)).on_hover_text(tip).clicked() { acts.push(Act::Push); }
                        let pull = if st.behind > 0 { format!("Pull  ↓{}", st.behind) } else { "Pull".into() };
                        if ui.add_enabled(idle && r.remote.is_some(), Button::new(pull)).on_hover_text("Get new commits from the remote").clicked() { acts.push(Act::Pull(self.cfg.pull_rebase)); }
                    });
                }
            });
            ui.add_space(2.0); ui.separator();
            if !r.is_git {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| { ui.label(RichText::new("This folder is not a git repository yet.").size(15.0)); ui.add_space(8.0); if ui.add_enabled(idle, Button::new("Initialize repository…")).clicked() { acts.push(Act::Open(Dialog::Init)); } });
                return;
            }
            let Some(st) = r.status.clone() else { ui.colored_label(RED, r.error.clone().unwrap_or_default()); return };
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Changes, format!("Changes ({})", st.changes.len()));
                ui.selectable_value(&mut self.tab, Tab::History, "History");
            });
            match self.tab {
                Tab::History => self.history_view(ui, st.ahead as usize),
                Tab::Changes if st.changes.is_empty() => {
                    ui.add_space(30.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Nothing to commit. The working tree is clean.").size(15.0));
                        ui.add_space(6.0);
                        if r.remote.is_some() && (st.ahead > 0 || (st.upstream.is_none() && !st.no_commits)) {
                            let t = if st.ahead > 0 { format!("{} commit{} waiting to be pushed.", st.ahead, if st.ahead == 1 { "" } else { "s" }) } else { format!("Branch {} is not on the remote yet.", st.branch) };
                            ui.label(RichText::new(t).weak()); ui.add_space(6.0);
                            if ui.add_enabled(idle, Button::new(RichText::new("Push").color(Color32::WHITE).strong()).fill(ACCENT)).clicked() { acts.push(Act::Push); }
                        } else if st.behind > 0 { ui.label(RichText::new(format!("{} new commit(s) on the remote. Use Pull.", st.behind)).weak()); }
                        else if let Some(c) = self.history.first() { ui.label(RichText::new(format!("Last commit: {} ({})", c.subject, c.when)).weak()); }
                    });
                }
                Tab::Changes => self.changes_view(ui, &r, &st, acts),
            }
        });
    }

    fn changes_view(&mut self, ui: &mut egui::Ui, r: &Repo, st: &git::Status, acts: &mut Vec<Act>) {
        let idle = self.busy.is_none();
        egui::TopBottomPanel::bottom("commitbox").show_separator_line(true).show_inside(ui, |ui| {
            ui.add_space(6.0);
            let subj = self.msg.lines().next().map(|l| l.chars().count()).unwrap_or(0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Commit message").strong());
                ui.label(RichText::new(format!("{subj}/72")).small().color(if subj > 72 { ORANGE } else { ui.visuals().weak_text_color() })).on_hover_text("Length of the first line; keep it under 72");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| { if ui.small_button("Generate").on_hover_text("Write a message from the ticked files (Ctrl+G)").clicked() { acts.push(Act::Generate); } });
            });
            egui::ScrollArea::vertical().id_salt("msgscroll").max_height(118.0).min_scrolled_height(118.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut self.msg).desired_rows(6).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace).hint_text("Summary on the first line, details below"));
            });
            let chosen = self.chosen();
            let risky: Vec<String> = chosen.iter().filter(|c| commitmsg::risky(c).is_some()).map(|c| c.path.clone()).collect();
            if !risky.is_empty() { ui.colored_label(ORANGE, format!("⚠ Ticked file{} that look risky: {}", if risky.len() == 1 { "" } else { "s" }, risky.join(", "))); }
            if self.amend && st.upstream.is_some() && st.ahead == 0 { ui.colored_label(ORANGE, "The last commit is already pushed. Amending it means you must force push."); }
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.amend, "Amend last commit").on_hover_text("Fold these changes (and the message) into the previous commit");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let can = idle && !self.msg.trim().is_empty() && (!chosen.is_empty() || self.amend);
                    if ui.add_enabled(can, Button::new(RichText::new("Commit & Push").color(Color32::WHITE).strong()).fill(ACCENT).min_size(Vec2::new(0.0, 24.0))).on_hover_text("Ctrl+Enter").clicked() { acts.push(Act::Commit(true)); }
                    if ui.add_enabled(can, Button::new("Commit").min_size(Vec2::new(0.0, 24.0))).on_hover_text("Commit without pushing (Ctrl+Shift+Enter)").clicked() { acts.push(Act::Commit(false)); }
                    ui.label(RichText::new(format!("{} of {} files", chosen.len(), st.changes.len())).weak());
                });
            });
            ui.add_space(6.0);
        });
        egui::TopBottomPanel::top("files").resizable(true).default_height(190.0).height_range(70.0..=600.0).show_inside(ui, |ui| {
            let n = st.changes.len(); let k = self.checked.len();
            ui.horizontal(|ui| {
                let mut all = k == n;
                let resp = ui.checkbox(&mut all, if k == n { "All files".to_string() } else { format!("{k} of {n} ticked") });
                if resp.changed() { if all { for c in &st.changes { self.checked.insert(c.path.clone()); } } else { self.checked.clear(); } self.regen_needed = true; }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| { let (a, d) = st.changes.iter().fold((0, 0), |s, c| (s.0 + c.adds, s.1 + c.dels)); ui.label(RichText::new(format!("+{a}  −{d}")).weak()); });
            });
            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                for c in &st.changes {
                    ui.horizontal(|ui| {
                        let mut on = self.checked.contains(&c.path);
                        if ui.checkbox(&mut on, "").changed() { if on { self.checked.insert(c.path.clone()); } else { self.checked.remove(&c.path); } self.regen_needed = true; }
                        ui.label(RichText::new(c.kind.letter()).monospace().strong().color(kind_color(&c.kind))).on_hover_text(c.kind.word());
                        let name = match &c.kind { Kind::Renamed(o) => format!("{o} → {}", c.path), _ => c.path.clone() };
                        let resp = ui.selectable_label(self.diff_for.as_deref() == Some(&c.path), name);
                        if let Some(w) = commitmsg::risky(c) { ui.label(RichText::new("⚠").color(ORANGE)).on_hover_text(w); }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| { ui.label(RichText::new(counts(c)).weak().monospace().size(11.5)); });
                        if resp.clicked() { acts.push(Act::ShowDiff(c.path.clone())); }
                        let new = matches!(c.kind, Kind::Untracked | Kind::Added);
                        resp.context_menu(|ui| {
                            if ui.button("Open file").clicked() { acts.push(Act::OpenFile(c.path.clone())); ui.close_menu(); }
                            if ui.button("Copy path").clicked() { ui.ctx().copy_text(c.path.clone()); ui.close_menu(); }
                            ui.separator();
                            if ui.button(if c.kind == Kind::Untracked { "Add to .gitignore" } else { "Stop tracking and ignore" }).clicked() { acts.push(Act::Ignore(c.path.clone(), c.kind != Kind::Untracked)); ui.close_menu(); }
                            if ui.add_enabled(idle && !matches!(c.kind, Kind::Renamed(_)), Button::new("Discard changes…")).clicked() { acts.push(Act::Open(Dialog::Discard(c.path.clone(), new))); ui.close_menu(); }
                        });
                    });
                }
            });
        });
        egui::CentralPanel::default().show_inside(ui, |ui| {
            match &self.diff_for {
                None => { ui.centered_and_justified(|ui| ui.label(RichText::new("Click a file to see what changed").weak())); }
                Some(_) => { let job = diff_job(&self.diff, ui); egui::ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| { ui.add(egui::Label::new(job).extend()); }); }
            }
        });
        let _ = r;
    }

    fn history_view(&mut self, ui: &mut egui::Ui, ahead: usize) {
        if self.history.is_empty() { ui.add_space(20.0); ui.label(RichText::new("No commits yet.").weak()); return; }
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            egui::Grid::new("history").striped(true).num_columns(4).spacing(Vec2::new(14.0, 6.0)).show(ui, |ui| {
                for (i, c) in self.history.iter().enumerate() {
                    let h = ui.add(egui::Label::new(RichText::new(&c.hash).monospace().color(if i < ahead { BLUE } else { GREY })).sense(Sense::click()));
                    let h = if i < ahead { h.on_hover_text("Not pushed yet. Right-click to copy the hash") } else { h.on_hover_text("Right-click to copy the hash") };
                    h.context_menu(|ui| { if ui.button("Copy hash").clicked() { ui.ctx().copy_text(c.hash.clone()); ui.close_menu(); } if ui.button("Copy message").clicked() { ui.ctx().copy_text(c.subject.clone()); ui.close_menu(); } });
                    ui.label(&c.subject); ui.label(RichText::new(&c.when).weak()); ui.label(RichText::new(&c.author).weak());
                    ui.end_row();
                }
            });
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        let r = self.cur().cloned(); let name = r.as_ref().map(|r| r.name.clone()).unwrap_or_default();
        let branch = r.as_ref().map(|r| r.branch()).unwrap_or_default();
        let Some(d) = &mut self.dialog else { return };
        let title = match d { Dialog::ForcePush => "Force push", Dialog::Init => "Initialize repository", Dialog::Remote(_) => "Remote", Dialog::Discard(..) => "Discard changes", Dialog::Undo => "Undo last commit", Dialog::NewBranch(_) => "New branch", Dialog::About => "About gitpushm", Dialog::Keys => "Keyboard shortcuts" };
        let mut close = false;
        // two buttons on the right: Cancel and the action
        let buttons = |ui: &mut egui::Ui, label: &str, danger: bool, enabled: bool| -> (bool, bool) {
            let mut res = (false, false);
            ui.add_space(8.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let b = Button::new(RichText::new(label).color(Color32::WHITE).strong()).fill(if danger { RED } else { ACCENT });
                res.0 = ui.add_enabled(enabled, b).clicked() || (enabled && ui.input(|i| i.key_pressed(Key::Enter)));
                res.1 = ui.button("Cancel").clicked();
            });
            res
        };
        let undo_subject = self.history.first().map(|c| c.subject.clone()).unwrap_or_default();
        let pushed = r.as_ref().and_then(|r| r.status.as_ref()).map_or(false, |s| s.upstream.is_some() && s.ahead == 0);
        egui::Window::new(title).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).default_width(420.0).show(ctx, |ui| {
            ui.set_max_width(440.0);
            match d {
                Dialog::ForcePush => {
                    ui.label(format!("Overwrite branch {branch} on the remote with your version?"));
                    ui.label(RichText::new("Uses --force-with-lease: it refuses if someone else pushed since your last fetch.").weak());
                    let (go, no) = buttons(ui, "Force push", true, true); if go { acts.push(Act::DoForcePush); } close = go || no;
                }
                Dialog::Init => {
                    ui.label(format!("Create a git repository in {name}?")); ui.label(RichText::new("The first branch is called main. You can add the remote next.").weak());
                    let (go, no) = buttons(ui, "Initialize", false, true); if go { acts.push(Act::DoInit); } close = go || no;
                }
                Dialog::Remote(url) => {
                    ui.label(format!("Remote address (origin) for {name}:"));
                    let te = ui.add(egui::TextEdit::singleline(url).desired_width(f32::INFINITY).hint_text(format!("git@github.com:you/{name}.git")));
                    if !te.has_focus() && url.is_empty() { te.request_focus(); }
                    ui.label(RichText::new("Create the empty repository on GitHub first, then paste its SSH or HTTPS address.").weak());
                    let ok = !url.trim().is_empty() && !url.trim().contains(' ');
                    let (go, no) = buttons(ui, "Save", false, ok); if go { acts.push(Act::DoRemote(url.clone())); } close = go || no;
                }
                Dialog::Discard(p, new) => {
                    ui.label(format!("Discard your changes to {p}?"));
                    ui.label(RichText::new(if *new { "This file is new: it is deleted permanently." } else { "The file goes back to the last commit. This can't be undone." }).color(ORANGE));
                    let (go, no) = buttons(ui, if *new { "Delete file" } else { "Discard" }, true, true); if go { acts.push(Act::DoDiscard(p.clone(), *new)); } close = go || no;
                }
                Dialog::Undo => {
                    ui.label(format!("Undo the last commit \"{undo_subject}\"?")); ui.label(RichText::new("Its changes stay in your files, ready to commit again.").weak());
                    if pushed { ui.label(RichText::new("It is already pushed: after committing again you will need to force push.").color(ORANGE)); }
                    let (go, no) = buttons(ui, "Undo commit", false, true); if go { acts.push(Act::DoUndo); } close = go || no;
                }
                Dialog::NewBranch(b) => {
                    ui.label("Branch name:");
                    let te = ui.add(egui::TextEdit::singleline(b).desired_width(f32::INFINITY).hint_text("feature/something")); if !te.has_focus() && b.is_empty() { te.request_focus(); }
                    ui.label(RichText::new("Creates the branch from here and switches to it. Uncommitted changes come along.").weak());
                    let ok = !b.trim().is_empty() && !b.contains(' ') && !b.contains(".."); let (go, no) = buttons(ui, "Create", false, ok); if go { acts.push(Act::DoNewBranch(b.clone())); } close = go || no;
                }
                Dialog::About => {
                    ui.label(RichText::new(format!("gitpushm {}", env!("CARGO_PKG_VERSION"))).strong());
                    ui.label("See every project in your projects folder, write a good commit message from what changed, and push.");
                    ui.label(RichText::new("Built with Rust and egui. Settings: ~/.config/gitpushm.conf and ~/.config/gitpushm/settings").weak());
                    ui.add_space(6.0); ui.with_layout(Layout::right_to_left(Align::Center), |ui| { if ui.button("Close").clicked() { close = true; } });
                }
                Dialog::Keys => {
                    egui::Grid::new("keys").num_columns(2).spacing(Vec2::new(24.0, 4.0)).show(ui, |ui| {
                        for (k, t) in [("Ctrl+Enter", "Commit & push"), ("Ctrl+Shift+Enter", "Commit only"), ("Ctrl+G", "Generate message"), ("F5", "Refresh projects"), ("Ctrl+F", "Filter projects"), ("Alt+↑ / Alt+↓", "Previous / next project"), ("Ctrl+1 / Ctrl+2", "Changes / History"), ("Ctrl+Q", "Quit")] {
                            ui.label(RichText::new(k).monospace()); ui.label(t); ui.end_row();
                        }
                    });
                    ui.add_space(6.0); ui.with_layout(Layout::right_to_left(Align::Center), |ui| { if ui.button("Close").clicked() { close = true; } });
                }
            }
        });
        if close { self.dialog = None; }
    }
}
