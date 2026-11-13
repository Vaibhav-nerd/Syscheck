//! Full-screen ratatui dashboard for live monitoring.

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::collector::Data;
use crate::sources::speedtest::{self, SpeedResult};
use crate::util::{human_bytes, human_uptime};

#[derive(Clone, Copy, PartialEq)]
enum ProcSort {
    Cpu,
    Mem,
}

enum SpeedState {
    Idle,
    Running(Receiver<SpeedResult>),
    Done(SpeedResult),
}

struct App {
    data: Data,
    interval: f64,
    sort: ProcSort,
    speed: SpeedState,
    only: Option<Vec<String>>,
}

pub fn run(mut collectors: crate::sources::Collectors, interval: f64, only: Option<Vec<String>>) {
    let mut terminal = ratatui::init();
    let mut app = App {
        data: Data::default(),
        interval,
        sort: ProcSort::Cpu,
        speed: SpeedState::Idle,
        only: only.clone(),
    };
    collectors.prime();
    // Give the first delta window a moment so the opening frame has real rates.
    std::thread::sleep(Duration::from_millis(400));
    let mut last_sample = Instant::now();
    app.data = collectors.sample(only.as_deref());

    loop {
        let _ = terminal.draw(|f| ui(f, &app));
        // Poll for keys in short slices so sampling stays responsive.
        if event::poll(Duration::from_millis(100)).unwrap_or(false) {
            if let Event::Key(key) = event::read().unwrap_or(Event::FocusGained) {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('c')
                            if key.modifiers.contains(KeyModifiers::CONTROL) =>
                        {
                            break
                        }
                        KeyCode::Char('c') => app.sort = ProcSort::Cpu,
                        KeyCode::Char('m') => app.sort = ProcSort::Mem,
                        KeyCode::Char('+') | KeyCode::Char('=') => {
                            app.interval = (app.interval * 1.25).min(30.0);
                        }
                        KeyCode::Char('-') => {
                            app.interval = (app.interval / 1.25).max(0.5);
                        }
                        KeyCode::Char('s') => start_speedtest(&mut app),
                        _ => {}
                    }
                }
            }
        }
        if let SpeedState::Running(rx) = &app.speed {
            if let Ok(result) = rx.try_recv() {
                app.speed = SpeedState::Done(result);
            }
        }
        if last_sample.elapsed().as_secs_f64() >= app.interval {
            app.data = collectors.sample(app.only.as_deref());
            last_sample = Instant::now();
        }
    }
    ratatui::restore();
}

fn start_speedtest(app: &mut App) {
    if matches!(app.speed, SpeedState::Running(_)) {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(speedtest::run());
    });
    app.speed = SpeedState::Running(rx);
}

// ---------------------------------------------------------------------------
// UI
// ---------------------------------------------------------------------------

fn color_for_pct(v: f64) -> Color {
    if v >= 85.0 {
        Color::Red
    } else if v >= 60.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}

fn block(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ))
}

fn bar_span(pct: f64, width: usize) -> Span<'static> {
    let pct = pct.clamp(0.0, 100.0);
    let filled = (pct / 100.0 * width as f64).round() as usize;
    Span::styled(
        format!("{}{}", "█".repeat(filled), "░".repeat(width.saturating_sub(filled))),
        Style::default().fg(color_for_pct(pct)),
    )
}

fn pct_span(pct: f64) -> Span<'static> {
    Span::styled(
        format!("{pct:5.1}%"),
        Style::default().fg(color_for_pct(pct)),
    )
}

fn temp_color(c: f64) -> Color {
    if c >= 80.0 {
        Color::Red
    } else if c >= 60.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}

fn dim(s: impl std::fmt::Display) -> Span<'static> {
    Span::styled(s.to_string(), Style::default().fg(Color::DarkGray))
}

fn ui(f: &mut Frame, app: &App) {
    let area = f.area();
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(area);

    render_header(f, header, app);
    render_body(f, body, app);
    render_footer(f, footer, app);
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let mut lines = Vec::new();
    if let Some(h) = &app.data.header {
        lines.push(Line::from(vec![
            Span::styled(h.host.clone(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            dim(&h.kernel),
        ]));
        lines.push(Line::from(vec![
            dim("up "),
            Span::raw(human_uptime(h.uptime)),
            dim("   load "),
            Span::raw(format!("{:.2} {:.2} {:.2}", h.load[0], h.load[1], h.load[2])),
        ]));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn render_body(f: &mut Frame, area: Rect, app: &App) {
    let [left, right] = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .areas(area);
    let [cpu, mem, temp] = Layout::vertical([
        Constraint::Percentage(45),
        Constraint::Percentage(30),
        Constraint::Min(3),
    ])
    .areas(left);
    let [disk, net, procs] = Layout::vertical([
        Constraint::Percentage(38),
        Constraint::Percentage(24),
        Constraint::Min(5),
    ])
    .areas(right);

    render_cpu(f, cpu, app);
    render_mem(f, mem, app);
    render_temps(f, temp, app);
    render_disk(f, disk, app);
    render_net(f, net, app);
    render_procs(f, procs, app);
}

fn render_cpu(f: &mut Frame, area: Rect, app: &App) {
    let mut lines = Vec::new();
    if let Some(c) = &app.data.cpu {
        let freq = c
            .freq_mhz
            .map(|mhz| format!("  {:.2} GHz", mhz / 1000.0))
            .unwrap_or_default();
        lines.push(Line::from(vec![dim(&format!("{} cores{}", c.ncores, freq))]));
        lines.push(Line::from(vec![
            Span::raw("total "),
            bar_span(c.overall, 24),
            Span::raw(" "),
            pct_span(c.overall),
        ]));
        for (i, core) in c.cores.iter().enumerate() {
            lines.push(Line::from(vec![
                Span::raw(format!("c{i:<4} ")),
                bar_span(*core, 24),
                Span::raw(" "),
                pct_span(*core),
            ]));
        }
    } else {
        lines.push(Line::from(dim("waiting for sample...")));
    }
    f.render_widget(
        Paragraph::new(lines).block(block("CPU")),
        area,
    );
}

fn render_mem(f: &mut Frame, area: Rect, app: &App) {
    let mut lines = Vec::new();
    if let Some(m) = &app.data.mem {
        lines.push(Line::from(vec![
            Span::raw("RAM    "),
            bar_span(m.pct, 24),
            Span::raw(" "),
            pct_span(m.pct),
            Span::raw("  "),
            Span::raw(format!(
                "{} / {}",
                human_bytes(m.used as f64),
                human_bytes(m.total as f64)
            )),
        ]));
        if m.swap_total > 0 {
            lines.push(Line::from(vec![
                Span::raw("SWAP   "),
                bar_span(m.swap_pct, 24),
                Span::raw(" "),
                pct_span(m.swap_pct),
                Span::raw("  "),
                Span::raw(format!(
                    "{} / {}",
                    human_bytes(m.swap_used as f64),
                    human_bytes(m.swap_total as f64)
                )),
            ]));
        }
    } else {
        lines.push(Line::from(dim("waiting for sample...")));
    }
    f.render_widget(Paragraph::new(lines).block(block("MEMORY")), area);
}

fn render_temps(f: &mut Frame, area: Rect, app: &App) {
    let visible = area.height.saturating_sub(2) as usize;
    let mut lines = Vec::new();
    for (label, t) in app.data.temps.iter().take(visible) {
        lines.push(Line::from(vec![
            Span::raw(format!("{:<26} ", label.chars().take(26).collect::<String>())),
            Span::styled(format!("{t:5.1}°C"), Style::default().fg(temp_color(*t))),
        ]));
    }
    if lines.is_empty() {
        lines.push(Line::from(dim("no temperature sensors found")));
    }
    f.render_widget(Paragraph::new(lines).block(block("TEMPERATURES")), area);
}

fn render_disk(f: &mut Frame, area: Rect, app: &App) {
    let mut lines = Vec::new();
    for fs in app.data.disks.iter().take(area.height.saturating_sub(2) as usize / 2) {
        lines.push(Line::from(vec![
            Span::raw(format!("{:<14} ", fs.mount)),
            bar_span(fs.pct, 14),
            Span::raw(" "),
            pct_span(fs.pct),
        ]));
        lines.push(Line::from(dim(format!(
            "  {} / {}",
            human_bytes(fs.used as f64),
            human_bytes(fs.total as f64)
        ))));
    }
    if let Some(ios) = &app.data.diskio {
        for io in ios.iter().take(2) {
            lines.push(Line::from(vec![
                Span::raw(format!("{:<8} ", io.name)),
                Span::raw(format!("R {:>9} W {:>9}", human_bps_short(io.read_bps), human_bps_short(io.write_bps))),
                Span::raw(" "),
                bar_span(io.util_pct, 10),
                Span::raw(" "),
                pct_span(io.util_pct),
            ]));
        }
    }
    f.render_widget(Paragraph::new(lines).block(block("STORAGE")), area);
}

fn render_net(f: &mut Frame, area: Rect, app: &App) {
    let mut lines = Vec::new();
    for i in app.data.net.iter().flatten().take(area.height.saturating_sub(2) as usize) {
        let (symbol, color) = match i.state.as_str() {
            "up" => ("●", Color::Green),
            "down" => ("○", Color::DarkGray),
            _ => ("?", Color::DarkGray),
        };
        lines.push(Line::from(vec![
            Span::styled(symbol, Style::default().fg(color)),
            Span::raw(format!(" {:<10} ", i.iface)),
            Span::raw(format!("↓ {:>9} ", human_bps_short(i.rx_bps))),
            Span::raw(format!("↑ {:>9} ", human_bps_short(i.tx_bps))),
            dim(&i.state),
        ]));
    }
    f.render_widget(Paragraph::new(lines).block(block("NETWORK")), area);
}

fn render_procs(f: &mut Frame, area: Rect, app: &App) {
    let mut lines = Vec::new();
    if let Some(pr) = &app.data.procs {
        let rows = match app.sort {
            ProcSort::Cpu => &pr.by_cpu,
            ProcSort::Mem => &pr.by_mem,
        };
        lines.push(Line::from(dim(format!(
            "  {:<20} {:<9} {:>7} {:>10}",
            "NAME", "USER", if app.sort == ProcSort::Cpu { "CPU" } else { "MEM" },
            "RSS"
        ))));
        for r in rows.iter().take(area.height.saturating_sub(3) as usize) {
            let pct = if app.sort == ProcSort::Cpu { r.cpu_pct } else { r.mem_pct };
            lines.push(Line::from(vec![
                Span::raw(format!("{:<20} ", r.name.chars().take(20).collect::<String>())),
                Span::raw(format!("{:<9} ", r.user)),
                pct_span(pct),
                Span::raw(format!(" {:>10}", human_bytes(r.rss as f64))),
            ]));
        }
    }
    f.render_widget(Paragraph::new(lines).block(block("TOP PROCESSES")), area);
}

fn render_footer(f: &mut Frame, area: Rect, app: &App) {
    let mut spans = vec![
        dim(" q"),
        Span::raw(" quit  "),
        dim("+/-"),
        Span::raw(format!(" refresh {:.1}s  ", app.interval)),
        dim("c/m"),
        Span::raw(" sort  "),
        dim("s"),
        Span::raw(" speed test  "),
    ];
    match &app.speed {
        SpeedState::Idle => {}
        SpeedState::Running(_) => spans.push(Span::styled(
            "speed test running...",
            Style::default().fg(Color::Yellow),
        )),
        SpeedState::Done(s) => match &s.error {
            Some(e) => spans.push(Span::styled(
                format!("speed test failed: {e}"),
                Style::default().fg(Color::Red),
            )),
            None => spans.push(Span::styled(
                format!(
                    "speed: ↓ {:.1} Mb/s  ↑ {:.1} Mb/s  ping {:.0} ms",
                    s.down_mbps, s.up_mbps, s.ping_ms
                ),
                Style::default().fg(Color::Green),
            )),
        },
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Compact rate format for narrow TUI columns.
fn human_bps_short(v: f64) -> String {
    let units = ["B", "K", "M", "G"];
    let mut x = v;
    for (i, u) in units.iter().enumerate() {
        if x.abs() < 1024.0 || i == units.len() - 1 {
            return format!("{x:.0}{u}");
        }
        x /= 1024.0;
    }
    unreachable!()
}
