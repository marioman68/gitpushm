//! The things the app does to a repo: build a commit message, commit chosen files, push, pull. Shared by the window and the command line.
use crate::commitmsg::{self, Input};
use crate::git::{self, Change, Kind, Status};
use std::path::Path;

pub fn message(repo: &Path, st: &Status, chosen: &[Change], conventional: bool) -> String {
    let ws = git::whitespace_only(repo, st.no_commits, chosen);
    let mut bump = None;
    for c in chosen.iter().filter(|c| matches!(commitmsg::category(&c.path), commitmsg::Cat::Build) && c.kind == Kind::Modified) { let (a, _) = git::added_removed_lines(repo, &c.path, st.no_commits); if let Some(v) = commitmsg::version_from_lines(&a) { bump = Some(v); break; } }
    commitmsg::generate(&Input { changes: chosen, whitespace_only: &ws, version_bump: bump, conventional })
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
