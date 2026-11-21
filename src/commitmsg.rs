//! Commit message from what actually changed: kinds of files, lines added and removed, renames, version bumps and
//! whitespace-only edits. Gives a short subject (conventional style if wanted) and a body that lists the files.
use crate::git::{Change, Kind};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cat { Docs, Test, Ci, Build, Deps, Config, Source, Asset }

pub fn category(path: &str) -> Cat {
    let p = path.to_ascii_lowercase(); let name = p.rsplit('/').next().unwrap_or(&p);
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    if p.starts_with(".github/") || p.starts_with(".gitlab") || p.starts_with(".circleci/") || name == ".travis.yml" || name == "jenkinsfile" || p.starts_with(".woodpecker") { return Cat::Ci; }
    if matches!(name, "cargo.lock" | "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" | "poetry.lock" | "go.sum" | "gemfile.lock" | "composer.lock" | "uv.lock" | "flake.lock") { return Cat::Deps; }
    if name == "build.rs" && !p.contains("src/") { return Cat::Build; }
    if matches!(name, "cargo.toml" | "package.json" | "pyproject.toml" | "setup.py" | "setup.cfg" | "go.mod" | "gemfile" | "composer.json" | "makefile" | "cmakelists.txt" | "dockerfile" | "docker-compose.yml" | "meson.build" | "build.gradle" | "pom.xml")
        || name.starts_with("requirements") && ext == "txt" { return Cat::Build; }
    if p.starts_with("tests/") || p.starts_with("test/") || p.contains("/tests/") || p.contains("/__tests__/") || name.starts_with("test_") || name.contains("_test.") || name.contains(".test.") || name.contains(".spec.") { return Cat::Test; }
    if matches!(ext, "md" | "rst" | "txt" | "adoc") || p.starts_with("docs/") || name.starts_with("license") || name.starts_with("changelog") || name.starts_with("readme") { return Cat::Docs; }
    if matches!(ext, "json" | "yaml" | "yml" | "toml" | "ini" | "cfg" | "conf" | "env") || name.starts_with('.') { return Cat::Config; }
    if matches!(ext, "png" | "jpg" | "jpeg" | "gif" | "svg" | "ico" | "webp" | "mp3" | "wav" | "ttf" | "otf" | "woff" | "woff2" | "pdf" | "zip") { return Cat::Asset; }
    Cat::Source
}

pub struct Input<'a> { pub changes: &'a [Change], pub whitespace_only: &'a [String], pub version_bump: Option<String>, pub conventional: bool }

pub fn generate(i: &Input) -> String {
    let ch: Vec<&Change> = i.changes.iter().collect();
    if ch.is_empty() { return "Sync repository".into(); }
    let file = |c: &Change| c.path.rsplit('/').next().unwrap_or(&c.path).to_string();
    let cats: Vec<Cat> = ch.iter().map(|c| category(&c.path)).collect();
    let all = |k: Cat| cats.iter().all(|c| *c == k);
    let new: Vec<&&Change> = ch.iter().filter(|c| matches!(c.kind, Kind::Added | Kind::Untracked)).collect();
    let gone: Vec<&&Change> = ch.iter().filter(|c| c.kind == Kind::Deleted).collect();
    let moved: Vec<&&Change> = ch.iter().filter(|c| matches!(c.kind, Kind::Renamed(_))).collect();
    let edited: Vec<&&Change> = ch.iter().filter(|c| matches!(c.kind, Kind::Modified | Kind::TypeChange | Kind::Conflict)).collect();
    let ws_all = !edited.is_empty() && new.is_empty() && gone.is_empty() && moved.is_empty() && edited.iter().all(|c| i.whitespace_only.iter().any(|w| w == &c.path));
    let (adds, dels): (u32, u32) = ch.iter().fold((0, 0), |a, c| (a.0 + c.adds, a.1 + c.dels));

    // what kind of change this is
    let release = i.version_bump.is_some() && ch.iter().all(|c| matches!(category(&c.path), Cat::Build | Cat::Deps | Cat::Docs));
    let ty: String = if release { "chore(release)".into() }
        else if ws_all { "style".into() }
        else if all(Cat::Docs) { "docs".into() } else if all(Cat::Test) { "test".into() } else if all(Cat::Ci) { "ci".into() }
        else if cats.iter().all(|c| *c == Cat::Deps) { "build(deps)".into() }
        else if cats.iter().all(|c| matches!(c, Cat::Build | Cat::Deps)) { "build".into() }
        else if cats.iter().all(|c| matches!(c, Cat::Config)) { "chore".into() }
        else if !moved.is_empty() && new.is_empty() && gone.is_empty() && moved.len() * 2 >= ch.len() { "refactor".into() }
        else if !gone.is_empty() && new.is_empty() && edited.is_empty() { "chore".into() }
        else if new.iter().any(|c| category(&c.path) == Cat::Source) && new.len() * 2 >= ch.len().min(4) { "feat".into() }
        else { "update".into() };
    // scope: the folder everything shares, skipping generic ones
    let scope = if ty.contains('(') { None } else { common_scope(&ch.iter().map(|c| c.path.as_str()).collect::<Vec<_>>()) };

    // the subject: say the most important thing first
    // name the files that matter most: source first, lock files and dotfiles last, bigger changes before smaller
    let names = |v: &[&&Change], n: usize| -> String {
        let mut v: Vec<&&Change> = v.to_vec();
        v.sort_by_key(|c| (rank(category(&c.path), &c.path), std::cmp::Reverse(c.adds + c.dels)));
        let mut s: Vec<String> = v.iter().take(n).map(|c| file(c)).collect(); s.dedup();
        if v.len() > n { format!("{} and {} more", s.join(", "), v.len() - n) } else { join_and(&s) }
    };
    let what = if let (Some(v), true) = (&i.version_bump, ty.starts_with("chore(release)")) { format!("bump version to {v}") }
        else if ws_all { format!("tidy whitespace in {}", names(&edited, 2)) }
        else if ty == "build(deps)" { format!("update {}", names(&edited.iter().chain(new.iter()).copied().collect::<Vec<_>>(), 2)) }
        else if ty == "refactor" { if moved.len() == 1 { if let Kind::Renamed(o) = &moved[0].kind { format!("rename {} to {}", o.rsplit('/').next().unwrap_or(o), file(moved[0])) } else { String::new() } } else { format!("move {} files", moved.len()) } }
        else {
            let mut parts = vec![];
            if !new.is_empty() { parts.push(format!("add {}", names(&new, 2))); }
            if !edited.is_empty() { parts.push(format!("update {}", names(&edited, if new.is_empty() { 3 } else { 1 }))); }
            if !gone.is_empty() { parts.push(format!("remove {}", names(&gone, 1))); }
            if !moved.is_empty() && ty != "refactor" { parts.push(format!("rename {} file{}", moved.len(), if moved.len() == 1 { "" } else { "s" })); }
            parts.join("; ")
        };
    let prefix = if i.conventional && ty != "update" { match &scope { Some(s) => format!("{ty}({s}): "), None => format!("{ty}: ") } } else { String::new() };
    // too long: drop the least important clauses (they're all in the body) before cutting words
    let mut what = what; let room = 72usize.saturating_sub(prefix.chars().count());
    while what.chars().count() > room { match what.rsplit_once("; ") { Some((a, _)) => what = a.to_string(), None => break } }
    let what = what.replace("; ", ", ");
    let subject = if prefix.is_empty() { capitalise(&what) } else { format!("{prefix}{what}") };
    let subject = shorten(&subject, 72);

    // the body: every file, grouped, with line counts
    let mut body: Vec<String> = vec![];
    let line = |c: &Change| -> String {
        let n = if c.binary { "binary".to_string() } else if c.adds + c.dels == 0 { String::new() } else if c.dels == 0 { format!("+{}", c.adds) } else if c.adds == 0 { format!("-{}", c.dels) } else { format!("+{} -{}", c.adds, c.dels) };
        let p = match &c.kind { Kind::Renamed(o) => format!("{o} -> {}", c.path), _ => c.path.clone() };
        if n.is_empty() { format!("- {p}") } else { format!("- {p} ({n})") }
    };
    let mut shown = 0;
    for (title, list) in [("New", &new), ("Changed", &edited), ("Renamed", &moved), ("Removed", &gone)] {
        if list.is_empty() { continue; }
        body.push(format!("{title}:"));
        let mut list: Vec<&&Change> = list.to_vec(); list.sort_by_key(|c| (rank(category(&c.path), &c.path), c.path.clone()));
        for c in list.iter() { if shown < 25 { body.push(line(c)); } shown += 1; }
    }
    if shown > 25 { body.push(format!("... and {} more", shown - 25)); }
    body.push(String::new());
    body.push(format!("{} file{} changed, +{} -{}", ch.len(), if ch.len() == 1 { "" } else { "s" }, adds, dels));
    if ch.len() == 1 && body.len() <= 4 { return subject; }   // a one-file change needs no body
    format!("{subject}\n\n{}", body.join("\n"))
}

fn rank(c: Cat, path: &str) -> u8 {
    let dot = path.rsplit('/').next().map_or(false, |n| n.starts_with('.'));
    (match c { Cat::Source => 0, Cat::Test => 1, Cat::Build => 2, Cat::Docs => 3, Cat::Ci => 4, Cat::Config => 5, Cat::Asset => 6, Cat::Deps => 7 }) + if dot { 8 } else { 0 }
}
fn common_scope(paths: &[&str]) -> Option<String> {
    let dirs: Vec<Vec<&str>> = paths.iter().map(|p| { let mut v: Vec<&str> = p.split('/').collect(); v.pop(); v }).collect();
    let first = dirs.first()?.clone(); let mut n = 0;
    while n < first.len() && dirs.iter().all(|d| d.get(n) == first.get(n)) { n += 1; }
    first[..n].iter().rev().find(|s| !matches!(s.to_ascii_lowercase().as_str(), "src" | "lib" | "app" | "pkg" | "internal" | "source" | "main" | "java" | "kotlin" | "com")).map(|s| s.to_string())
}
fn join_and(v: &[String]) -> String { match v.len() { 0 => String::new(), 1 => v[0].clone(), 2 => format!("{} and {}", v[0], v[1]), _ => format!("{} and {}", v[..v.len() - 1].join(", "), v[v.len() - 1]) } }
fn capitalise(s: &str) -> String { let mut c = s.chars(); match c.next() { Some(f) => f.to_uppercase().collect::<String>() + c.as_str(), None => String::new() } }
fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max { return s.to_string(); }
    let cut: String = s.chars().take(max - 1).collect(); let cut = cut.rsplit_once(' ').map(|(a, _)| a).unwrap_or(&cut);
    format!("{}…", cut.trim_end_matches([',', ' ']))
}

/// "1.2.3" when a manifest's version line changed
pub fn version_from_lines(added: &[String]) -> Option<String> {
    for l in added {
        let t = l.trim();
        for key in ["version =", "\"version\":", "version:", "__version__ ="] {
            if let Some(rest) = t.strip_prefix(key) { let v = rest.trim().trim_matches(|c| c == '"' || c == '\'' || c == ',' || c == ' '); if v.chars().next().map_or(false, |c| c.is_ascii_digit()) { return Some(v.to_string()); } }
        }
    }
    None
}

/// files that probably should not be committed: secrets and files too big for GitHub
pub fn risky(c: &Change) -> Option<String> {
    let p = c.path.to_ascii_lowercase(); let name = p.rsplit('/').next().unwrap_or(&p);
    if c.kind == Kind::Deleted { return None; }
    let secret = name == ".env" || name.starts_with(".env.") && !name.ends_with(".example") && !name.ends_with(".sample") || matches!(name, "id_rsa" | "id_ed25519" | "id_ecdsa" | "credentials.json" | "service-account.json" | ".npmrc" | ".pypirc" | ".netrc" | "secrets.json" | "settings.local.json")
        || name.ends_with(".pem") || name.ends_with(".key") || name.ends_with(".p12") || name.ends_with(".pfx") || name.ends_with(".keystore") || name.contains("backup-codes");
    if secret { return Some("looks like a secret or key".into()); }
    if c.size > 100 * 1024 * 1024 { return Some(format!("{} MB: GitHub refuses files over 100 MB", c.size / 1024 / 1024)); }
    if c.size > 50 * 1024 * 1024 { return Some(format!("{} MB: GitHub warns over 50 MB", c.size / 1024 / 1024)); }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c(p: &str, k: Kind, a: u32, d: u32) -> Change { Change { path: p.into(), kind: k, staged: false, unstaged: true, adds: a, dels: d, binary: false, size: 10 } }
    fn g(ch: &[Change]) -> String { generate(&Input { changes: ch, whitespace_only: &[], version_bump: None, conventional: true }) }
    fn subj(s: &str) -> String { s.lines().next().unwrap().to_string() }
    #[test] fn docs_only() { assert_eq!(subj(&g(&[c("README.md", Kind::Modified, 4, 1)])), "docs: update README.md"); }
    #[test] fn new_source_is_feat_with_scope() { assert_eq!(subj(&g(&[c("core/src/build.rs", Kind::Untracked, 200, 0), c("core/src/pick.rs", Kind::Untracked, 80, 0), c("core/src/lib.rs", Kind::Modified, 2, 0)])), "feat(core): add build.rs and pick.rs, update lib.rs"); }
    #[test] fn deps() { assert_eq!(subj(&g(&[c("Cargo.lock", Kind::Modified, 40, 12)])), "build(deps): update Cargo.lock"); }
    #[test] fn rename_refactor() { assert_eq!(subj(&g(&[c("src/new_name.rs", Kind::Renamed("src/old_name.rs".into()), 0, 0)])), "refactor: rename old_name.rs to new_name.rs"); }
    #[test] fn whitespace() { let ch = [c("src/a.rs", Kind::Modified, 5, 5)]; let s = generate(&Input { changes: &ch, whitespace_only: &["src/a.rs".into()], version_bump: None, conventional: true }); assert_eq!(subj(&s), "style: tidy whitespace in a.rs"); }
    #[test] fn version() { let ch = [c("Cargo.toml", Kind::Modified, 1, 1), c("Cargo.lock", Kind::Modified, 1, 1)]; let s = generate(&Input { changes: &ch, whitespace_only: &[], version_bump: Some("3.0.0".into()), conventional: true }); assert_eq!(subj(&s), "chore(release): bump version to 3.0.0"); }
    #[test] fn plain_mode() { let ch = [c("src/app.rs", Kind::Modified, 10, 2)]; let s = generate(&Input { changes: &ch, whitespace_only: &[], version_bump: None, conventional: false }); assert_eq!(s, "Update app.rs"); }
    #[test] fn body_and_length() {
        let ch: Vec<Change> = (0..40).map(|k| c(&format!("src/module_with_long_name_{k}.rs"), Kind::Modified, k, 1)).collect();
        let s = g(&ch); let first = subj(&s);
        assert!(first.chars().count() <= 72, "{first}"); assert!(s.contains("... and 15 more")); assert!(s.contains("40 files changed"));
    }
    #[test] fn source_named_first() { let ch = [c("Cargo.lock", Kind::Untracked, 4000, 0), c(".gitignore", Kind::Untracked, 1, 0), c("src/app.rs", Kind::Untracked, 600, 0), c("src/git.rs", Kind::Untracked, 150, 0), c("README.md", Kind::Untracked, 20, 0)]; assert_eq!(subj(&g(&ch)), "feat: add app.rs, git.rs and 3 more"); }
    #[test] fn long_drops_clauses() { let ch = [c("src/app.rs", Kind::Untracked, 600, 0), c("src/commitmsg.rs", Kind::Untracked, 160, 0), c("src/config.rs", Kind::Untracked, 20, 0), c("README.md", Kind::Modified, 40, 30), c("legacy/gitpushm-gui.py", Kind::Renamed("gitpushm-gui.py".into()), 0, 0), c("legacy/gitpushm", Kind::Renamed("gitpushm".into()), 0, 0)]; let s = subj(&g(&ch)); assert_eq!(s, "feat: add app.rs, commitmsg.rs and 1 more, update README.md"); }
    #[test] fn removal() { assert_eq!(subj(&g(&[c("old.py", Kind::Deleted, 0, 30)])), "chore: remove old.py"); }
    #[test] fn version_lines() { assert_eq!(version_from_lines(&["version = \"1.4.0\"".into()]).as_deref(), Some("1.4.0")); assert_eq!(version_from_lines(&["  \"version\": \"2.0.1\",".into()]).as_deref(), Some("2.0.1")); }
    #[test] fn secrets() { assert!(risky(&c(".env", Kind::Untracked, 1, 0)).is_some()); assert!(risky(&c(".env.example", Kind::Untracked, 1, 0)).is_none()); assert!(risky(&c("keys/server.pem", Kind::Untracked, 1, 0)).is_some()); }
}
