//! The things the app does to a repo: build a commit message, commit chosen files, push, pull. Shared by the window and the command line.
use crate::commitmsg::{self, Input};
use crate::git::{self, Change, Kind, Status};
use std::path::Path;

pub fn message(repo: &Path, st: &Status, chosen: &[Change], conventional: bool) -> String {
    let ws = git::whitespace_only(repo, st.no_commits, chosen);
    let mut bump = None;
    for c in chosen.iter().filter(|c| matches!(commitmsg::category(&c.path), commitmsg::Cat::Build) && c.kind == Kind::Modified) { let (a, _) = git::added_removed_lines(repo, &c.path, st.no_commits); if let Some(v) = commitmsg::version_from_lines(&a) { bump = Some(v); break; } }
    let syms = symbols(repo, st, chosen);
    commitmsg::generate(&Input { changes: chosen, whitespace_only: &ws, version_bump: bump, conventional, symbols: &syms })
}
/// definitions added / changed / removed in each source file, read from the diff (and the hunk headers git puts the enclosing function in)
pub fn symbols(repo: &Path, st: &Status, chosen: &[Change]) -> Vec<commitmsg::FileSymbols> {
    let base = if st.no_commits { "4b825dc642cb6eb9a060e54bf8d69288fbee4904" } else { "HEAD" };
    let mut out = vec![];
    for c in chosen.iter().take(60) {
        if c.binary || !matches!(commitmsg::category(&c.path), commitmsg::Cat::Source | commitmsg::Cat::Test) || c.size > 2_000_000 { continue; }
        let text = if c.kind == Kind::Untracked {
            std::fs::read_to_string(repo.join(&c.path)).map(|s| s.lines().map(|l| format!("+{l}\n")).collect::<String>()).unwrap_or_default()
        } else {
            let mut args = vec!["diff", "-U0", "-M", base, "--"]; if let Kind::Renamed(o) = &c.kind { args.push(o); } args.push(&c.path);
            git::git(repo, &args).stdout
        };
        // the file as it is now, to find which function each edit sits in
        let now: Vec<String> = if c.kind == Kind::Deleted { vec![] } else { std::fs::read_to_string(repo.join(&c.path)).map(|s| s.lines().map(String::from).collect()).unwrap_or_default() };
        let enclosing = |line: usize| -> Option<String> { (0..line.min(now.len())).rev().take(400).find_map(|k| commitmsg::definition(&now[k])) };
        let (mut added, mut removed, mut context): (Vec<String>, Vec<String>, Vec<String>) = (vec![], vec![], vec![]);
        let push = |v: &mut Vec<String>, s: String| { if !v.contains(&s) { v.push(s); } };
        // per hunk: definitions in the +/- lines are added/removed; a hunk with none edits whatever function it sits in
        let mut hunk_start = 0usize; let mut hunk_has_def = false; let mut in_hunk = false;
        let mut close = |start: usize, has_def: bool, ctx: &mut Vec<String>| { if !has_def { if let Some(d) = enclosing(start) { if !ctx.contains(&d) { ctx.push(d); } } } };
        for l in text.lines() {
            if l.starts_with("+++") || l.starts_with("---") { continue; }
            if let Some(h) = l.strip_prefix("@@ ") {
                if in_hunk { close(hunk_start, hunk_has_def, &mut context); }
                // "@@ -a,b +c,d @@": new-file line c (1-based)
                hunk_start = h.split_whitespace().find(|x| x.starts_with('+')).and_then(|x| x[1..].split(',').next()?.parse::<usize>().ok()).unwrap_or(0);
                hunk_has_def = false; in_hunk = true; continue;
            }
            if let Some(a) = l.strip_prefix('+') { if let Some(d) = commitmsg::definition(a) { push(&mut added, d); hunk_has_def = true; } }
            else if let Some(r) = l.strip_prefix('-') { if let Some(d) = commitmsg::definition(r) { push(&mut removed, d); hunk_has_def = true; } }
        }
        if in_hunk && c.kind != Kind::Untracked { close(hunk_start, hunk_has_def, &mut context); }
        let changed: Vec<String> = added.iter().filter(|a| removed.contains(a)).cloned()
            .chain(context.iter().filter(|x| !added.contains(x) && !removed.contains(x)).cloned()).collect();
        let f = commitmsg::FileSymbols { path: c.path.clone(), added: added.iter().filter(|a| !removed.contains(a)).cloned().collect(),
            removed: removed.iter().filter(|r| !added.contains(r)).cloned().collect(), changed };
        if !(f.added.is_empty() && f.changed.is_empty() && f.removed.is_empty()) { out.push(f); }
    }
    out
}
/// stage exactly the chosen files (adds, edits, deletions and renames), then commit
pub fn commit(repo: &Path, chosen: &[Change], all: bool, msg: &str, amend: bool, log: &mut Vec<String>) -> bool {
    let o = if all { git::git(repo, &["add", "-A"]) } else {
        // start from a clean index so only the chosen files go in (a repo with no commits has nothing to reset to)
        if !git::git(repo, &["reset", "-q"]).ok { let _ = git::git(repo, &["rm", "-r", "-q", "--cached", "--ignore-unmatch", "."]); }
        if chosen.is_empty() { return commit_only(repo, msg, amend, log); }   // e.g. amending only the message
        let mut args = vec!["add".to_string(), "-A".into(), "--".into()];
        for c in chosen { args.push(c.path.clone()); if let Kind::Renamed(o) = &c.kind { args.push(o.clone()); } }
        git::git_owned(repo, &args)
    };
    if !o.ok { log.push(format!("git add failed: {}", o.text())); return false; }
    commit_only(repo, msg, amend, log)
}
fn commit_only(repo: &Path, msg: &str, amend: bool, log: &mut Vec<String>) -> bool {
    let mut args = vec!["commit", "-m", msg]; if amend { args.push("--amend"); }
    let o = git::git(repo, &args); log.push(o.text());
    if !o.ok && o.text().contains("Please tell me who you are") { log.push("Git doesn't know your name yet. Run: git config --global user.name \"Your Name\" and git config --global user.email you@example.com".into()); }
    o.ok
}
/// change the message of the newest commit (only offered while it isn't pushed)
pub fn reword(repo: &Path, msg: &str, log: &mut Vec<String>) -> bool {
    let o = git::git(repo, &["commit", "--amend", "--allow-empty", "--only", "-m", msg]); log.push(o.text()); o.ok
}
pub fn push(repo: &Path, force: bool, log: &mut Vec<String>) -> bool {
    let args: Vec<&str> = if force { vec!["push", "--force-with-lease", "-u", "origin", "HEAD"] } else { vec!["push", "-u", "origin", "HEAD"] };
    let o = git::git(repo, &args); let t = o.text(); if !t.is_empty() { log.push(t.clone()); }
    if !o.ok {
        let l = t.to_lowercase();
        log.push(if l.contains("no such remote") || l.contains("does not appear to be a git repository") { "There is no 'origin' remote yet: use Repository > Set remote.".into() }
            else if l.contains("rejected") || l.contains("fetch first") || l.contains("non-fast-forward") { "The remote has commits you don't have. Pull first (or force push from the Repository menu if you mean to overwrite).".into() }
            else if l.contains("permission denied") || l.contains("authentication") || l.contains("could not read username") { "The remote refused the login. Check your SSH key or token.".into() }
            else { "Push failed.".into() });
    }
    o.ok
}
pub fn pull(repo: &Path, rebase: bool, log: &mut Vec<String>) -> bool {
    let o = git::git(repo, &if rebase { vec!["pull", "--rebase"] } else { vec!["pull", "--ff-only"] }); log.push(o.text());
    if !o.ok && !rebase && o.text().to_lowercase().contains("not possible to fast-forward") { log.push("Your branch and the remote have both moved. Use Repository > Pull with rebase.".into()); }
    o.ok
}
