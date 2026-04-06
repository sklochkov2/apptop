mod app;
mod proc;
mod ui;

use std::io::{self, Write};
use std::time::{Duration, Instant};

use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{format_mib, App, SortColumn};

#[derive(Parser)]
#[command(name = "apptop", about = "Top-like memory usage viewer aggregated by application")]
struct Cli {
    /// Delay between updates in seconds
    #[arg(short = 'd', long = "delay", default_value_t = 2.0)]
    delay: f64,

    /// Run in batch mode (non-interactive); optionally specify iteration count
    #[arg(short = 'b', long = "batch")]
    batch: bool,

    /// Number of iterations (batch mode implied; 0 = unlimited)
    #[arg(short = 'n', long = "iterations", default_value_t = 0)]
    iterations: u64,

    /// Sort column: pss, swap, total, procs, name
    #[arg(short = 's', long = "sort", default_value = "total")]
    sort: String,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let sort_col = SortColumn::from_str_loose(&cli.sort).unwrap_or_else(|| {
        eprintln!(
            "warning: unknown sort column '{}', defaulting to 'total'",
            cli.sort
        );
        SortColumn::Total
    });

    let delay = Duration::from_secs_f64(cli.delay);
    let batch = cli.batch || cli.iterations > 0;

    if batch {
        run_batch(sort_col, delay, cli.iterations)?;
    } else {
        run_tui(sort_col, delay)?;
    }

    Ok(())
}

fn run_batch(sort_col: SortColumn, delay: Duration, iterations: u64) -> anyhow::Result<()> {
    let mut app = App::new(sort_col);
    let unlimited = iterations == 0;
    let mut iter = 0u64;

    loop {
        app.refresh();
        iter += 1;

        println!(
            "apptop — {} apps, {} procs | PSS: {} Swap: {} Total: {}",
            app.entries.len(),
            app.total_procs,
            format_mib(app.total_pss),
            format_mib(app.total_swap),
            format_mib(app.total_mem),
        );
        println!(
            "{:>6} {:>12} {:>12} {:>12}  {}",
            SortColumn::Procs.label(),
            SortColumn::Pss.label(),
            SortColumn::Swap.label(),
            SortColumn::Total.label(),
            SortColumn::Name.label(),
        );

        for e in &app.entries {
            println!(
                "{:>6} {:>12} {:>12} {:>12}  {}",
                e.num_procs,
                format_mib(e.pss_kib),
                format_mib(e.swap_kib),
                format_mib(e.total_kib),
                e.exe,
            );
        }
        println!();

        if !unlimited && iter >= iterations {
            break;
        }

        std::thread::sleep(delay);
    }

    Ok(())
}

fn run_tui(sort_col: SortColumn, delay: Duration) -> anyhow::Result<()> {
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(sort_col);
    app.refresh();

    let result = tui_loop(&mut terminal, &mut app, delay);

    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    io::stdout().flush()?;

    result
}

fn tui_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    delay: Duration,
) -> anyhow::Result<()> {
    let mut last_refresh = Instant::now();

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        let timeout = delay.saturating_sub(last_refresh.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Char('s') => app.cycle_sort(),
                    KeyCode::Char('r') | KeyCode::Char('R') => app.toggle_sort_order(),
                    KeyCode::Char('1') => app.set_sort(SortColumn::Procs),
                    KeyCode::Char('2') => app.set_sort(SortColumn::Pss),
                    KeyCode::Char('3') => app.set_sort(SortColumn::Swap),
                    KeyCode::Char('4') => app.set_sort(SortColumn::Total),
                    KeyCode::Char('5') => app.set_sort(SortColumn::Name),
                    KeyCode::Up | KeyCode::Char('k') => app.scroll_up(),
                    KeyCode::Down | KeyCode::Char('j') => app.scroll_down(),
                    KeyCode::PageUp => app.scroll_page_up(20),
                    KeyCode::PageDown => app.scroll_page_down(20),
                    KeyCode::Home => app.scroll_home(),
                    KeyCode::End => app.scroll_end(),
                    _ => {}
                }
            }
        }

        if last_refresh.elapsed() >= delay {
            app.refresh();
            last_refresh = Instant::now();
        }
    }

    Ok(())
}
