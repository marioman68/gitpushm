//! Settings. The projects folder stays in ~/.config/gitpushm.conf (same file the older versions used); the rest sits next to it.
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config { pub projects: PathBuf, pub conventional: bool, pub autofill: bool, pub fetch_on_start: bool, pub pull_rebase: bool, pub theme: String, pub describe: bool }
fn home() -> PathBuf { std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")) }
fn dir() -> PathBuf { std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config")) }
fn expand(s: &str) -> PathBuf { if let Some(r) = s.strip_prefix("~/") { home().join(r) } else if s == "~" { home() } else { PathBuf::from(s) } }
impl Config {
    pub fn load() -> Config {
        let projects = std::fs::read_to_string(dir().join("gitpushm.conf")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).map(|s| expand(&s)).unwrap_or_else(|| home().join("Projects"));
        let mut c = Config { projects, conventional: true, autofill: true, fetch_on_start: false, pull_rebase: false, theme: "system".into(), describe: true };
        if let Ok(t) = std::fs::read_to_string(dir().join("gitpushm").join("settings")) {
            for l in t.lines() { if let Some((k, v)) = l.split_once('=') { let on = v.trim() == "true"; match k.trim() { "conventional" => c.conventional = on, "autofill" => c.autofill = on, "fetch_on_start" => c.fetch_on_start = on, "pull_rebase" => c.pull_rebase = on, "theme" => c.theme = v.trim().to_string(), "describe" => c.describe = on, _ => {} } } }
        }
        c
    }
    pub fn save(&self) {
        let d = dir(); let _ = std::fs::create_dir_all(d.join("gitpushm"));
        let _ = std::fs::write(d.join("gitpushm.conf"), self.projects.to_string_lossy().as_bytes());
        let _ = std::fs::write(d.join("gitpushm").join("settings"), format!("conventional={}\nautofill={}\nfetch_on_start={}\npull_rebase={}\ntheme={}\ndescribe={}\n", self.conventional, self.autofill, self.fetch_on_start, self.pull_rebase, self.theme, self.describe));
    }
}
