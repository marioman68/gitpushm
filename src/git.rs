//! Running git and reading what it says. Every call is `git -C <repo> ...` with no shell in between.
use std::path::Path;
use std::process::{Command, Stdio};

pub struct Out { pub ok: bool, pub stdout: String, pub stderr: String }
impl Out { pub fn text(&self) -> String { let mut s = self.stdout.trim_end().to_string(); let e = self.stderr.trim_end(); if !e.is_empty() { if !s.is_empty() { s.push('\n'); } s.push_str(e); } s } }

pub fn git(repo: &Path, args: &[&str]) -> Out {
    // never stop to ask for a password or an editor: the window cannot answer
    match Command::new("git").arg("-C").arg(repo).args(args)
        .env("GIT_TERMINAL_PROMPT", "0").env("GIT_EDITOR", "true").env("GIT_ASKPASS", "true").env("LC_ALL", "C")
        .stdin(Stdio::null()).output()
    {
        Ok(o) => Out { ok: o.status.success(), stdout: String::from_utf8_lossy(&o.stdout).into_owned(), stderr: String::from_utf8_lossy(&o.stderr).into_owned() },
        Err(e) => Out { ok: false, stdout: String::new(), stderr: format!("could not run git: {e}") },
    }
}
pub fn git_owned(repo: &Path, args: &[String]) -> Out { let a: Vec<&str> = args.iter().map(|s| s.as_str()).collect(); git(repo, &a) }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind { Added, Modified, Deleted, Renamed(String), TypeChange, Conflict, Untracked }
impl Kind {
    pub fn letter(&self) -> &'static str { match self { Kind::Added => "A", Kind::Modified => "M", Kind::Deleted => "D", Kind::Renamed(_) => "R", Kind::TypeChange => "T", Kind::Conflict => "!", Kind::Untracked => "?" } }
    pub fn word(&self) -> &'static str { match self { Kind::Added | Kind::Untracked => "new", Kind::Modified | Kind::TypeChange => "changed", Kind::Deleted => "deleted", Kind::Renamed(_) => "renamed", Kind::Conflict => "conflict" } }
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // staged/unstaged kept for display later
pub struct Change { pub path: String, pub kind: Kind, pub staged: bool, pub unstaged: bool, pub adds: u32, pub dels: u32, pub binary: bool, pub size: u64 }

#[derive(Clone, Debug, Default)]
pub struct Status { pub branch: String, pub upstream: Option<String>, pub ahead: u32, pub behind: u32, pub changes: Vec<Change>, pub detached: bool, pub no_commits: bool }

/// `git status --porcelain=v2 --branch -z`: branch, upstream, ahead/behind and every change
pub fn status(repo: &Path) -> Result<Status, String> {
    let o = git(repo, &["status", "--porcelain=v2", "--branch", "-z", "--untracked-files=all"]);
    if !o.ok { return Err(o.text()); }
    let mut s = Status::default();
    let mut it = o.stdout.split('\0').peekable();
    while let Some(rec) = it.next() {
        if rec.is_empty() { continue; }
        if let Some(h) = rec.strip_prefix("# ") {
            if let Some(b) = h.strip_prefix("branch.head ") { if b == "(detached)" { s.detached = true; s.branch = "detached".into(); } else { s.branch = b.into(); } }
            else if let Some(u) = h.strip_prefix("branch.upstream ") { s.upstream = Some(u.into()); }
            else if let Some(ab) = h.strip_prefix("branch.ab ") { for p in ab.split(' ') { if let Some(a) = p.strip_prefix('+') { s.ahead = a.parse().unwrap_or(0); } else if let Some(b) = p.strip_prefix('-') { s.behind = b.parse().unwrap_or(0); } } }
            else if let Some(oid) = h.strip_prefix("branch.oid ") { s.no_commits = oid == "(initial)"; }
            continue;
        }
        let f: Vec<&str> = rec.splitn(9, ' ').collect();
        match rec.as_bytes()[0] {
            b'1' if f.len() >= 9 => { let xy = f[1]; s.changes.push(change(f[8], xy, None)); }
            b'2' => { let f2: Vec<&str> = rec.splitn(10, ' ').collect(); if f2.len() >= 10 { let orig = it.next().unwrap_or("").to_string(); s.changes.push(change(f2[9], f2[1], Some(orig))); } }
            b'u' => { let fu: Vec<&str> = rec.splitn(11, ' ').collect(); if let Some(p) = fu.get(10) { s.changes.push(Change { path: p.to_string(), kind: Kind::Conflict, staged: false, unstaged: true, adds: 0, dels: 0, binary: false, size: 0 }); } }
            b'?' => { s.changes.push(Change { path: rec[2..].to_string(), kind: Kind::Untracked, staged: false, unstaged: true, adds: 0, dels: 0, binary: false, size: 0 }); }
            _ => {}
        }
    }
    Ok(s)
}
fn change(path: &str, xy: &str, orig: Option<String>) -> Change {
    let b = xy.as_bytes(); let (x, y) = (b[0], b[1]);
    let kind = if let Some(o) = orig { Kind::Renamed(o) } else if x == b'A' { Kind::Added } else if x == b'D' || y == b'D' { Kind::Deleted } else if x == b'T' || y == b'T' { Kind::TypeChange } else { Kind::Modified };
    Change { path: path.to_string(), kind, staged: x != b'.', unstaged: y != b'.', adds: 0, dels: 0, binary: false, size: 0 }
}

/// lines added / removed per file against the last commit (or against nothing in a brand new repo), plus untracked files
pub fn fill_counts(repo: &Path, st: &mut Status) {
    let base = if st.no_commits { "4b825dc642cb6eb9a060e54bf8d69288fbee4904" } else { "HEAD" }; // git's empty tree
    let o = git(repo, &["diff", "--numstat", "-z", "-M", base]);
    if o.ok { apply_numstat(&o.stdout, st); }
    for c in st.changes.iter_mut() {
        let p = repo.join(&c.path);
        c.size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        if c.kind == Kind::Untracked && c.size <= 2_000_000 {
            match std::fs::read(&p) { Ok(b) => { if b.contains(&0) { c.binary = true; } else { c.adds = String::from_utf8_lossy(&b).lines().count() as u32; } } Err(_) => {} }
        } else if c.kind == Kind::Untracked { c.binary = true; }
    }
}
fn apply_numstat(out: &str, st: &mut Status) {
    // records: "adds\tdels\tpath\0" or, for renames, "adds\tdels\t\0old\0new\0"
    let mut it = out.split('\0');
    while let Some(rec) = it.next() {
        if rec.is_empty() { continue; }
        let mut f = rec.splitn(3, '\t'); let (a, d, p) = (f.next().unwrap_or(""), f.next().unwrap_or(""), f.next().unwrap_or(""));
        let path = if p.is_empty() { let _old = it.next(); it.next().unwrap_or("").to_string() } else { p.to_string() };
        if let Some(c) = st.changes.iter_mut().find(|c| c.path == path) {
            if a == "-" { c.binary = true; } else { c.adds = a.parse().unwrap_or(0); c.dels = d.parse().unwrap_or(0); }
        }
    }
}
/// edited files whose changes are only whitespace (used to call a commit "style"). With -w git leaves such files out entirely.
pub fn whitespace_only(repo: &Path, no_commits: bool, changes: &[Change]) -> Vec<String> {
    if no_commits { return vec![]; }
    let o = git(repo, &["diff", "--numstat", "-w", "-z", "HEAD"]);
    if !o.ok { return vec![]; }
    let real: Vec<String> = o.stdout.split('\0').filter_map(|r| { let f: Vec<&str> = r.splitn(3, '\t').collect(); if f.len() == 3 && !(f[0] == "0" && f[1] == "0") && !f[2].is_empty() { Some(f[2].to_string()) } else { None } }).collect();
    changes.iter().filter(|c| c.kind == Kind::Modified && !c.binary && c.adds + c.dels > 0 && !real.contains(&c.path)).map(|c| c.path.clone()).collect()
}
/// the diff text for one file (or the start of a new file)
pub fn file_diff(repo: &Path, c: &Change, no_commits: bool) -> String {
    if c.kind == Kind::Untracked {
        if c.binary { return format!("(new binary file, {} bytes)", c.size); }
        return std::fs::read_to_string(repo.join(&c.path)).map(|t| t.lines().take(400).map(|l| format!("+{l}")).collect::<Vec<_>>().join("\n")).unwrap_or_default();
    }
    let base = if no_commits { "4b825dc642cb6eb9a060e54bf8d69288fbee4904" } else { "HEAD" };
    let mut args = vec!["diff", "-M", base, "--"]; if let Kind::Renamed(o) = &c.kind { args.push(o); } args.push(&c.path);
    let o = git(repo, &args); let t = o.text(); if t.trim().is_empty() { "(no text difference)".into() } else { t }
}
/// the old and new text of a manifest line like `version = "1.2.3"` (for spotting version bumps)
pub fn added_removed_lines(repo: &Path, path: &str, no_commits: bool) -> (Vec<String>, Vec<String>) {
    if no_commits { return (vec![], vec![]); }
    let o = git(repo, &["diff", "-U0", "HEAD", "--", path]);
    let (mut a, mut r) = (vec![], vec![]);
    for l in o.stdout.lines() { if l.starts_with("+++") || l.starts_with("---") { continue; } if let Some(x) = l.strip_prefix('+') { a.push(x.to_string()); } else if let Some(x) = l.strip_prefix('-') { r.push(x.to_string()); } }
    (a, r)
}

pub fn remote_url(repo: &Path) -> Option<String> { let o = git(repo, &["remote", "get-url", "origin"]); if o.ok { let s = o.stdout.trim().to_string(); if s.is_empty() { None } else { Some(s) } } else { None } }
pub fn branches(repo: &Path) -> Vec<String> { git(repo, &["for-each-ref", "--format=%(refname:short)", "refs/heads"]).stdout.lines().map(|s| s.to_string()).filter(|s| !s.is_empty()).collect() }
#[derive(Clone, Debug)]
pub struct Commit { pub hash: String, pub author: String, pub when: String, pub subject: String }
pub fn log(repo: &Path, n: usize) -> Vec<Commit> {
    let o = git(repo, &["log", &format!("-n{n}"), "--pretty=format:%h%x1f%an%x1f%ar%x1f%s"]);
    o.stdout.lines().filter_map(|l| { let f: Vec<&str> = l.split('\x1f').collect(); if f.len() == 4 { Some(Commit { hash: f[0].into(), author: f[1].into(), when: f[2].into(), subject: f[3].into() }) } else { None } }).collect()
}
/// commits on this branch that the remote doesn't have yet: (short hash, full message), newest first
pub fn outgoing(repo: &Path, has_upstream: bool) -> Vec<(String, String)> {
    let range: Vec<&str> = if has_upstream { vec!["log", "@{u}..HEAD"] } else { vec!["log", "HEAD", "--not", "--remotes"] };
    let mut args = range; args.extend(["-n50", "--format=%h%x1f%B%x1e"]);
    let o = git(repo, &args); if !o.ok { return vec![]; }
    o.stdout.split('\x1e').filter_map(|r| { let r = r.trim_start_matches('\n'); let (h, m) = r.split_once('\x1f')?; if h.is_empty() { None } else { Some((h.to_string(), m.trim_end().to_string())) } }).collect()
}
pub fn stash_count(repo: &Path) -> usize { git(repo, &["stash", "list"]).stdout.lines().count() }
/// a browser address for the remote (GitHub, GitLab and friends), if it can be worked out
pub fn web_url(remote: &str) -> Option<String> {
    let r = remote.trim().trim_end_matches(".git");
    if let Some(rest) = r.strip_prefix("git@") { let (host, path) = rest.split_once(':')?; return Some(format!("https://{host}/{path}")); }
    if let Some(rest) = r.strip_prefix("ssh://git@") { let (host, path) = rest.split_once('/')?; return Some(format!("https://{}/{path}", host.split(':').next()?)); }
    if r.starts_with("https://") || r.starts_with("http://") { return Some(r.to_string()); }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn web_urls() {
        assert_eq!(web_url("git@github.com:marioman68/gitpushm.git").as_deref(), Some("https://github.com/marioman68/gitpushm"));
        assert_eq!(web_url("https://github.com/a/b.git").as_deref(), Some("https://github.com/a/b"));
        assert_eq!(web_url("ssh://git@host.example:2222/a/b.git").as_deref(), Some("https://host.example/a/b"));
    }
    #[test] fn numstat_parse() {
        let mut st = Status { changes: vec![Change { path: "src/a.rs".into(), kind: Kind::Modified, staged: false, unstaged: true, adds: 0, dels: 0, binary: false, size: 0 },
            Change { path: "new.rs".into(), kind: Kind::Renamed("old.rs".into()), staged: true, unstaged: false, adds: 0, dels: 0, binary: false, size: 0 },
            Change { path: "logo.png".into(), kind: Kind::Modified, staged: false, unstaged: true, adds: 0, dels: 0, binary: false, size: 0 }], ..Default::default() };
        apply_numstat("10\t2\tsrc/a.rs\0" .to_string().as_str(), &mut st);
        apply_numstat("3\t1\t\0old.rs\0new.rs\0-\t-\tlogo.png\0", &mut st);
        assert_eq!((st.changes[0].adds, st.changes[0].dels), (10, 2));
        assert_eq!((st.changes[1].adds, st.changes[1].dels), (3, 1));
        assert!(st.changes[2].binary);
    }
}
