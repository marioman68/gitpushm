#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]
mod app;
mod commitmsg;
mod config;
mod git;
mod ops;
mod scan;

const HELP: &str = "gitpushm 3.0 - Git Project Push Manager

  gitpushm                     open the window
  gitpushm status              every project: branch, changes, ahead/behind
  gitpushm msg <project>       print the commit message it would write
  gitpushm push <project> [-m MESSAGE] [--dry-run]
                               commit everything and push (message generated if none)
  <project> is a folder name in your projects folder, or a path.";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cfg = config::Config::load();
    let find = |n: &str| { let p = std::path::PathBuf::from(n); if p.join(".git").exists() { p } else { cfg.projects.join(n) } };
    match args.first().map(|s| s.as_str()) {
        None => {
            // window icon (built into the binary) and app id, so the taskbar matches the launcher entry
            let mut viewport = eframe::egui::ViewportBuilder::default().with_inner_size([1180.0, 760.0]).with_min_inner_size([860.0, 520.0]).with_title("gitpushm").with_app_id("gitpushm");
            if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png")) { viewport = viewport.with_icon(std::sync::Arc::new(icon)); }
            let opts = eframe::NativeOptions { viewport, ..Default::default() };
            let _ = eframe::run_native("gitpushm", opts, Box::new(move |cc| Ok(Box::new(app::App::new(cc, cfg)))));
        }
        Some("status") => {
            for r in scan::scan(&cfg.projects) {
                let st = if !r.is_git { "not a git repo".to_string() } else { format!("{:<14} {:>3} changed{}{}", r.branch(), r.dirty(), if r.ahead() > 0 { format!("  ↑{}", r.ahead()) } else { String::new() }, if r.behind() > 0 { format!("  ↓{}", r.behind()) } else { String::new() }) };
                println!("{:<28} {}", r.name, st);
            }
        }
        Some("msg") if args.len() >= 2 => { let p = find(&args[1]); let r = scan::load(&p); match r.status { Some(st) => println!("{}", ops::message(&p, &st, &st.changes, cfg.conventional)), None => eprintln!("{} is not a git repository", p.display()) } }
        Some("push") if args.len() >= 2 => {
            let p = find(&args[1]); let r = scan::load(&p); let Some(st) = r.status else { eprintln!("{} is not a git repository", p.display()); std::process::exit(1) };
            let generated = ops::message(&p, &st, &st.changes, cfg.conventional);
            let m = match args.iter().position(|a| a == "-m").and_then(|i| args.get(i + 1)) { Some(t) if cfg.describe => commitmsg::with_description(t, &generated), Some(t) => t.clone(), None => generated };
            let risky: Vec<String> = st.changes.iter().filter_map(|c| commitmsg::risky(c).map(|w| format!("  {} ({w})", c.path))).collect();
            if !risky.is_empty() { eprintln!("Not committing, these look risky:\n{}\nCommit them yourself if you mean it.", risky.join("\n")); std::process::exit(2); }
            if args.iter().any(|a| a == "--dry-run") { println!("{m}"); return; }
            let mut log = vec![];
            let ok = (st.changes.is_empty() || ops::commit(&p, &st.changes, true, &m, false, &mut log)) && ops::push(&p, false, &mut log);
            for l in log { if !l.trim().is_empty() { println!("{l}"); } }
            if !ok { std::process::exit(1); }
        }
        _ => println!("{HELP}"),
    }
}
