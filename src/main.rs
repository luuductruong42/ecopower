use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::*,
    style::{Color, Style},
    symbols,
    text::Line,
    widgets::{
        Axis, Block, Borders, Cell, Chart, Clear, Dataset, Gauge, GraphType,
        Paragraph, Row, Table, TableState, Tabs,
    },
};
use std::time::{Duration, Instant};
use sysinfo::{Disks, Networks, Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, Signal, System};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tab {
    Overview,
    Processes,
    Disks,
    History,
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
enum SortBy {
    Cpu,
    Memory,
}

impl Default for Tab {
    fn default() -> Self { Tab::Overview }
}

impl Default for SortBy {
    fn default() -> Self { SortBy::Cpu }
}

#[derive(Default)]
struct App {
    sys: System,
    networks: Networks,
    tab: Tab,
    tick: u64,
    cpu_history: Vec<u64>,
    mem_history: Vec<u64>,
    download_history: Vec<f64>,
    upload_history: Vec<f64>,
    processes: Vec<ProcessInfo>,
    table_state: TableState,
    filter: String,
    input_mode: bool,
    status: String,
    sort_by: SortBy,
    show_help: bool,
}

#[derive(Clone)]
struct ProcessInfo {
    pid: u32,
    name: String,
    user: String,
    cpu: f32,
    memory_kb: u64,
    state: String,
    nice: i32,
}

impl App {
    fn new() -> Self {
        let mut sys = System::new_with_specifics(RefreshKind::everything());
        let networks = Networks::new_with_refreshed_list();
        sys.refresh_all();

        Self {
            sys,
            networks,
            table_state: TableState::default().with_selected(Some(0)),
            status: "Ecopower Final Boss - Ready".to_string(),
            ..Default::default()
        }
    }

    fn update(&mut self) {
        self.tick += 1;
        self.sys.refresh_all();
        self.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::everything());
        self.networks.refresh(true);

        // CPU & Memory History
        let cpu = self.sys.global_cpu_usage() as u64;
        self.cpu_history.push(cpu);
        if self.cpu_history.len() > 150 { self.cpu_history.remove(0); }

        let mem_pct = if self.sys.total_memory() > 0 {
            (self.sys.used_memory() as f64 / self.sys.total_memory() as f64 * 100.0) as u64
        } else { 0 };
        self.mem_history.push(mem_pct);
        if self.mem_history.len() > 150 { self.mem_history.remove(0); }

        // Network History (KB/s)
        let mut total_down: u64 = 0;
        let mut total_up: u64 = 0;
        for (_, data) in self.networks.iter() {
            total_down += data.received();
            total_up += data.transmitted();
        }
        let down_kbps = (total_down as f64 / 1024.0) * 2.0;
        let up_kbps = (total_up as f64 / 1024.0) * 2.0;

        self.download_history.push(down_kbps);
        self.upload_history.push(up_kbps);
        if self.download_history.len() > 150 { self.download_history.remove(0); }
        if self.upload_history.len() > 150 { self.upload_history.remove(0); }

        // Processes
        self.processes.clear();
        for (pid, proc) in self.sys.processes() {
            self.processes.push(ProcessInfo {
                pid: pid.as_u32(),
                name: proc.name().to_string_lossy().into_owned(),
                user: proc.user_id().map_or("?".to_string(), |u| u.to_string()),
                cpu: proc.cpu_usage(),
                memory_kb: proc.memory() / 1024,
                state: proc.status().to_string(),
                nice: get_nice_from_proc(pid.as_u32()),
            });
        }
        self.sort_processes();

        self.status = if self.filter.is_empty() {
            format!("Tick {} | {} processes", self.tick, self.processes.len())
        } else {
            format!("Filter: {} | {} processes", self.filter, self.processes.len())
        };
    }

    fn sort_processes(&mut self) {
        match self.sort_by {
            SortBy::Cpu => self.processes.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal)),
            SortBy::Memory => self.processes.sort_by(|a, b| b.memory_kb.cmp(&a.memory_kb)),
        }
    }

    fn kill_selected(&mut self) {
        if let Some(i) = self.table_state.selected() {
            let filtered = self.filtered_processes();
            if let Some(p) = filtered.get(i) {
                if let Some(proc) = self.sys.process(Pid::from_u32(p.pid)) {
                    if proc.kill_with(Signal::Kill).unwrap_or(false) {
                        self.status = format!("✓ Killed PID {} - {}", p.pid, p.name);
                    } else {
                        self.status = format!("✗ Cannot kill PID {}", p.pid);
                    }
                }
            }
        }
    }

    fn renice_selected(&mut self, nice: i32) {
        if let Some(i) = self.table_state.selected() {
            let filtered = self.filtered_processes();
            if let Some(p) = filtered.get(i) {
                let _ = std::process::Command::new("renice")
                    .arg(nice.to_string())
                    .arg("-p")
                    .arg(p.pid.to_string())
                    .output();
                self.status = format!("Reniced PID {} → {}", p.pid, nice);
            }
        }
    }

    fn filtered_processes(&self) -> Vec<&ProcessInfo> {
        if self.filter.is_empty() {
            self.processes.iter().collect()
        } else {
            self.processes.iter()
                .filter(|p| p.name.to_lowercase().contains(&self.filter.to_lowercase()))
                .collect()
        }
    }
}

fn get_nice_from_proc(pid: u32) -> i32 {
    if let Ok(content) = std::fs::read_to_string(format!("/proc/{}/stat", pid)) {
        let parts: Vec<&str> = content.split_whitespace().collect();
        if parts.len() > 18 {
            return parts[18].parse().unwrap_or(0);
        }
    }
    0
}

fn main() -> Result<()> {
    std::panic::set_hook(Box::new(|_| { let _ = ratatui::restore(); }));

    let mut terminal = ratatui::init();
    let mut app = App::new();
    let tick_rate = Duration::from_millis(500);
    let mut last_tick = Instant::now();
    let mut renice_mode = false;
    let mut renice_input = String::new();

    loop {
        if last_tick.elapsed() >= tick_rate {
            app.update();
            last_tick = Instant::now();
        }

        terminal.draw(|f| draw_ui(f, &mut app, renice_mode, &renice_input))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if renice_mode {
                        match key.code {
                            KeyCode::Enter => {
                                if let Ok(val) = renice_input.parse::<i32>() {
                                    if (-20..=19).contains(&val) {
                                        app.renice_selected(val);
                                    }
                                }
                                renice_mode = false;
                                renice_input.clear();
                            }
                            KeyCode::Esc => {
                                renice_mode = false;
                                renice_input.clear();
                            }
                            KeyCode::Backspace => { let _ = renice_input.pop(); }
                            KeyCode::Char(c) if c.is_ascii_digit() || c == '-' => renice_input.push(c),
                            _ => {}
                        }
                    } else if app.input_mode {
                        match key.code {
                            KeyCode::Enter => app.input_mode = false,
                            KeyCode::Esc => {
                                app.filter.clear();
                                app.input_mode = false;
                            }
                            KeyCode::Backspace => { let _ = app.filter.pop(); }
                            KeyCode::Char(c) if c.is_ascii() => app.filter.push(c),
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => break,
                            KeyCode::Char('h') => app.show_help = !app.show_help,
                            KeyCode::Tab => {
                                app.tab = match app.tab {
                                    Tab::Overview => Tab::Processes,
                                    Tab::Processes => Tab::Disks,
                                    Tab::Disks => Tab::History,
                                    Tab::History => Tab::Overview,
                                }
                            }
                            KeyCode::Char('1') => app.tab = Tab::Overview,
                            KeyCode::Char('2') => app.tab = Tab::Processes,
                            KeyCode::Char('3') => app.tab = Tab::Disks,
                            KeyCode::Char('4') => app.tab = Tab::History,
                            KeyCode::Char('f') => app.input_mode = true,
                            KeyCode::Char('k') => app.kill_selected(),
                            KeyCode::Char('n') => renice_mode = true,
                            KeyCode::Char('c') => app.sort_by = SortBy::Cpu,
                            KeyCode::Char('m') => app.sort_by = SortBy::Memory,
                            KeyCode::Down | KeyCode::Char('j') => {
                                let len = app.filtered_processes().len();
                                let i = app.table_state.selected().unwrap_or(0);
                                app.table_state.select(Some((i + 1).min(len.saturating_sub(1))));
                            }
                            KeyCode::Up => {
                                let i = app.table_state.selected().unwrap_or(0);
                                app.table_state.select(Some(i.saturating_sub(1)));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    ratatui::restore();
    Ok(())
}

// ====================== DRAW UI ======================
fn draw_ui(f: &mut ratatui::Frame, app: &mut App, renice_mode: bool, renice_input: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(14), Constraint::Length(6)])
        .split(f.area());

    let titles = ["Overview", "Processes", "Disks", "History"]
        .iter()
        .map(|&t| Line::from(t))
        .collect::<Vec<_>>();

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(" Ecopower Final Boss "))
        .select(match app.tab {
            Tab::Overview => 0,
            Tab::Processes => 1,
            Tab::Disks => 2,
            Tab::History => 3,
        })
        .highlight_style(Style::default().fg(Color::Cyan).bold());

    f.render_widget(tabs, chunks[0]);

    match app.tab {
        Tab::Overview => draw_overview_detailed(f, app, chunks[1]),
        Tab::Processes => draw_processes(f, app, chunks[1]),
        Tab::Disks => draw_disks(f, chunks[1]),
        Tab::History => draw_history(f, app, chunks[1]),
    }

    let status_line = if renice_mode {
        format!("Renice mode → Nhập nice (-20..19): {}", renice_input)
    } else if app.input_mode {
        format!("Filter mode: {}", app.filter)
    } else {
        app.status.clone()
    };

    let help_line = "f: Filter | k: Kill | n: Renice | c/m: Sort | ↑↓/j/k: Di chuyển | h: Help | q: Thoát";

    let status = Paragraph::new(vec![
        Line::from(status_line),
        Line::from(help_line).style(Style::default().fg(Color::Yellow)),
    ])
    .block(Block::default().borders(Borders::ALL).title("Status"));

    f.render_widget(status, chunks[2]);

    if app.show_help {
        let help = Paragraph::new(
            "f → Filter\nk → Kill\nn → Renice\nc/m → Sort\n↑↓ j/k → Di chuyển\n1-4/Tab → Chuyển tab\nq → Thoát",
        )
        .block(Block::default().title("Help").borders(Borders::ALL).style(Style::default().fg(Color::Cyan)));

        let area = centered_rect(70, 45, f.area());
        f.render_widget(Clear, area);
        f.render_widget(help, area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

// ====================== OVERVIEW ======================
fn draw_overview_detailed(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(10), Constraint::Length(14)])
        .split(area);

    let gauge_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);

    let cpu_gauge = Gauge::default()
        .block(Block::default().title("CPU Usage").borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Red))
        .label(format!("{}%", app.cpu_history.last().unwrap_or(&0)))
        .ratio(*app.cpu_history.last().unwrap_or(&0) as f64 / 100.0);

    let mem_gauge = Gauge::default()
        .block(Block::default().title("Memory Usage").borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Green))
        .label(format!("{}%", app.mem_history.last().unwrap_or(&0)))
        .ratio(*app.mem_history.last().unwrap_or(&0) as f64 / 100.0);

    f.render_widget(cpu_gauge, gauge_chunks[0]);
    f.render_widget(mem_gauge, gauge_chunks[1]);

    let load = System::load_average();
    let uptime = System::uptime();
    let hostname = System::host_name().unwrap_or_else(|| "Unknown".to_string());
    let kernel = System::kernel_version().unwrap_or_else(|| "Unknown".to_string());
    let os = System::os_version().unwrap_or_else(|| "Unknown".to_string());

    let info = Paragraph::new(vec![
        Line::from(format!("Hostname     : {}", hostname)),
        Line::from(format!("OS Version   : {}", os)),
        Line::from(format!("Kernel       : {}", kernel)),
        Line::from(format!("Uptime       : {} phút ({:.1} giờ)", uptime / 60, uptime as f64 / 3600.0)),
        Line::from(format!("Load Average : {:.2} {:.2} {:.2}", load.one, load.five, load.fifteen)),
        Line::from(format!("Total Processes : {}", app.processes.len())),
    ])
    .block(Block::default().title("System Information").borders(Borders::ALL))
    .style(Style::default().fg(Color::White));

    f.render_widget(info, chunks[1]);
}

// ====================== HISTORY - NETWORK ======================
fn draw_history(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(12), Constraint::Length(12), Constraint::Length(12)])
        .split(area);

    let current_cpu = *app.cpu_history.last().unwrap_or(&0);
    let current_mem = *app.mem_history.last().unwrap_or(&0);
    let current_down = *app.download_history.last().unwrap_or(&0.0);
    let current_up = *app.upload_history.last().unwrap_or(&0.0);

    let cpu_data: Vec<(f64, f64)> = app.cpu_history.iter().enumerate().map(|(i, &v)| (i as f64, v as f64)).collect();
    let mem_data: Vec<(f64, f64)> = app.mem_history.iter().enumerate().map(|(i, &v)| (i as f64, v as f64)).collect();
    let down_data: Vec<(f64, f64)> = app.download_history.iter().enumerate().map(|(i, &v)| (i as f64, v)).collect();
    let up_data: Vec<(f64, f64)> = app.upload_history.iter().enumerate().map(|(i, &v)| (i as f64, v)).collect();

    let max_net = down_data.iter().chain(up_data.iter()).map(|(_, v)| *v).fold(0.0, f64::max).max(100.0);

    let cpu_chart = Chart::new(vec![Dataset::default()
        .name("CPU Usage")
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Red).bold())
        .data(&cpu_data)])
        .block(Block::default().title(format!("CPU Usage History  •  Hiện tại: {}%", current_cpu)).borders(Borders::ALL))
        .x_axis(Axis::default().title("Thời gian (~75 giây)").bounds([0.0, cpu_data.len() as f64]))
        .y_axis(Axis::default().title("CPU %").bounds([0.0, 100.0]).labels(vec![
            Line::from("0%"), Line::from("50%"), Line::from("100%")
        ]));

    let mem_chart = Chart::new(vec![Dataset::default()
        .name("Memory Usage")
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Green).bold())
        .data(&mem_data)])
        .block(Block::default().title(format!("Memory Usage History  •  Hiện tại: {}%", current_mem)).borders(Borders::ALL))
        .x_axis(Axis::default().title("Thời gian (~75 giây)").bounds([0.0, mem_data.len() as f64]))
        .y_axis(Axis::default().title("Memory %").bounds([0.0, 100.0]).labels(vec![
            Line::from("0%"), Line::from("50%"), Line::from("100%")
        ]));

    let net_chart = Chart::new(vec![
        Dataset::default()
            .name(format!("↓ Download ({:.1} KB/s)", current_down))
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Blue))
            .data(&down_data),
        Dataset::default()
            .name(format!("↑ Upload ({:.1} KB/s)", current_up))
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Magenta))
            .data(&up_data),
    ])
    .block(Block::default().title("Network Usage History (KB/s)").borders(Borders::ALL))
    .x_axis(Axis::default().title("Thời gian (~75 giây)").bounds([0.0, down_data.len() as f64]))
    .y_axis(Axis::default().title("Tốc độ (KB/s)").bounds([0.0, max_net]).labels(vec![
        Line::from("0"),
        Line::from(format!("{:.0}", max_net / 2.0)),
        Line::from(format!("{:.0}", max_net)),
    ]));

    f.render_widget(cpu_chart, chunks[0]);
    f.render_widget(mem_chart, chunks[1]);
    f.render_widget(net_chart, chunks[2]);
}

fn draw_processes(f: &mut ratatui::Frame, app: &mut App, area: Rect) {
    let filtered = app.filtered_processes();
    let rows: Vec<Row> = filtered.iter().map(|p| {
        let cpu_color = if p.cpu > 70.0 { Color::Red } else if p.cpu > 30.0 { Color::Yellow } else { Color::Green };
        Row::new(vec![
            Cell::from(p.pid.to_string()),
            Cell::from(p.name.clone()),
            Cell::from(p.user.clone()),
            Cell::from(format!("{:>3}%", p.cpu as u32)).style(Style::default().fg(cpu_color)),
            Cell::from(format!("{:>6} MB", p.memory_kb / 1024)),
            Cell::from(p.state.clone()),
            Cell::from(p.nice.to_string()),
        ])
    }).collect();

    let table = Table::new(rows, [
        Constraint::Length(8), Constraint::Min(25), Constraint::Length(10),
        Constraint::Length(10), Constraint::Length(12), Constraint::Length(8), Constraint::Length(6),
    ])
    .header(Row::new(vec!["PID", "NAME", "USER", "CPU%", "MEMORY", "STATE", "NICE"])
        .style(Style::default().fg(Color::Cyan).bold()))
    .block(Block::default().title(format!("Processes ({} shown)", filtered.len())).borders(Borders::ALL))
    .row_highlight_style(Style::default().bg(Color::Magenta).fg(Color::Black).bold())
    .highlight_symbol("▶ ");

    f.render_stateful_widget(table, area, &mut app.table_state);
}

fn draw_disks(f: &mut ratatui::Frame, area: Rect) {
    let disks = Disks::new_with_refreshed_list();
    let rows: Vec<Row> = disks.list().iter().map(|disk| {
        let used = if disk.total_space() > 0 {
            100.0 - (disk.available_space() as f64 / disk.total_space() as f64 * 100.0)
        } else { 0.0 };
        Row::new(vec![
            Cell::from(disk.name().to_string_lossy().to_string()),
            Cell::from(disk.mount_point().to_string_lossy().to_string()),
            Cell::from(format!("{:.1} GB", disk.total_space() as f64 / 1_073_741_824.0)),
            Cell::from(format!("{:.1}%", used)),
        ])
    }).collect();

    let table = Table::new(rows, [
        Constraint::Min(15), Constraint::Min(25), Constraint::Length(12), Constraint::Length(8),
    ])
    .header(Row::new(vec!["Disk", "Mount", "Size", "Used %"]).style(Style::default().fg(Color::Cyan).bold()))
    .block(Block::default().title("Disks").borders(Borders::ALL));

    f.render_widget(table, area);
}
