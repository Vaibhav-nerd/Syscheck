//! syscheck — terminal system performance checker for Linux.

mod collector;
mod report;
mod sources;
mod tui;
mod util;

use std::time::Duration;

use clap::{Arg, ArgAction, Command};
use collector::{parse_only, Data, SECTIONS};
use sources::speedtest;
use sources::Collectors;

const SAMPLE_SECONDS: f64 = 0.5;
const DEFAULT_WATCH: f64 = 2.0;

fn cli() -> Command {
    Command::new("syscheck")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Terminal system performance checker for Linux \
                (CPU, RAM, storage, network, processes, temperatures)")
        .arg(
            Arg::new("watch")
                .long("watch")
                .num_args(0..=1)
                .default_missing_value("2")
                .value_name("SECONDS")
                .help("live TUI dashboard (default 2s refresh)"),
        )
        .arg(
            Arg::new("only")
                .long("only")
                .value_name("SECTIONS")
                .help(format!("comma-separated subset of: {}", SECTIONS.join(", "))),
        )
        .arg(
            Arg::new("speedtest")
                .long("speedtest")
                .action(ArgAction::SetTrue)
                .help("also run an internet speed test (Cloudflare)"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .action(ArgAction::SetTrue)
                .help("output machine-readable JSON (one-shot mode)"),
        )
        .after_help(format!(
            "Sections: {}\nIn watch mode: q quit, +/- refresh interval, \
             c/m sort processes, s speed test",
            SECTIONS.join(", ")
        ))
        .help_template(
            "{before-help}{name} {version}\n{about-with-newline}\n{usage-heading} {usage}\n\n\
             {all-args}{after-help}",
        )
}

fn main() {
    let m = cli().get_matches();
    let only = match m.get_one::<String>("only").map(|s| s.as_str()) {
        Some(v) => match parse_only(Some(v)) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(2);
            }
        },
        None => None,
    };
    let speedtest_flag = m.get_flag("speedtest");
    let json = m.get_flag("json");

    if let Some(watch) = m.get_one::<String>("watch") {
        let interval = watch.parse::<f64>().unwrap_or(DEFAULT_WATCH).max(0.5);
        if json {
            eprintln!("error: --json is only supported in one-shot mode");
            std::process::exit(2);
        }
        if !atty_stdout() {
            eprintln!("error: watch mode requires a terminal");
            std::process::exit(1);
        }
        let collectors = Collectors::new();
        tui::run(collectors, interval, only);        return;
    }

    // One-shot mode
    let mut collectors = Collectors::new();
    collectors.prime();
    std::thread::sleep(Duration::from_secs_f64(SAMPLE_SECONDS));
    let mut data: Data = collectors.sample(only.as_deref());

    if speedtest_flag {
        eprintln!("Running speed test (ping, download, upload)...");
        data.speedtest = Some(speedtest::run());
    }

    if json {
        println!("{}", report::render_json(&data));
    } else {
        let colors = atty_stdout();
        println!("{}", report::render(&data, only.as_deref(), colors));
    }
}

fn atty_stdout() -> bool {
    unsafe { libc::isatty(libc::STDOUT_FILENO) == 1 }
}
