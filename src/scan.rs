//! The projects folder: every sub-folder, whether it is a git repo, and its state. Checked in parallel.
use crate::git::{self, Status};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Repo { pub name: String, pub path: PathBuf, pub is_git: bool, pub status: Option<Status>, pub remote: Option<String>, pub error: Option<String> }
impl Repo {
    pub fn dirty(&self) -> usize { self.status.as_ref().map(|s| s.changes.len()).unwrap_or(0) }
    pub fn ahead(&self) -> u32 { self.status.as_ref().map(|s| s.ahead).unwrap_or(0) }
    pub fn behind(&self) -> u32 { self.status.as_ref().map(|s| s.behind).unwrap_or(0) }
    pub fn branch(&self) -> String { self.status.as_ref().map(|s| s.branch.clone()).unwrap_or_default() }
}

pub fn list(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir() && !p.file_name().map_or(true, |n| n.to_string_lossy().starts_with('.'))).collect()).unwrap_or_default();
    v.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default());
    v
}
pub fn load(path: &Path) -> Repo {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let is_git = path.join(".git").exists();
    if !is_git { return Repo { name, path: path.to_path_buf(), is_git, status: None, remote: None, error: None }; }
    match git::status(path) {
        Ok(mut st) => { git::fill_counts(path, &mut st); Repo { name, path: path.to_path_buf(), is_git, remote: git::remote_url(path), status: Some(st), error: None } }
        Err(e) => Repo { name, path: path.to_path_buf(), is_git, status: None, remote: None, error: Some(e) },
    }
}
/// every project, eight at a time
pub fn scan(dir: &Path) -> Vec<Repo> {
    let paths = list(dir);
    let mut out: Vec<Option<Repo>> = vec![None; paths.len()];
    for chunk in paths.iter().enumerate().collect::<Vec<_>>().chunks(8) {
        let got: Vec<(usize, Repo)> = std::thread::scope(|s| { let hs: Vec<_> = chunk.iter().map(|(i, p)| s.spawn(move || (*i, load(p)))).collect(); hs.into_iter().filter_map(|h| h.join().ok()).collect() });
        for (i, r) in got { out[i] = Some(r); }
    }
    out.into_iter().flatten().collect()
}
