use std::io::{self, Stdout};
use std::panic;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use color_eyre::eyre::Result;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tracing_appender::non_blocking::WorkerGuard;

mod app;
mod event;
mod model;
mod sampler;
mod ui;

use app::App;
use event::spawn_input_thread;
use sampler::{SamplerCommand, spawn_sampler_thread};

type Tui = Terminal<CrosstermBackend<Stdout>>;

fn init_tracing() -> WorkerGuard {
    let appender = tracing_appender::rolling::never(".", "hanun.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .init();
    guard
}

/// Enables raw mode and switches to the alternate screen, returning the `Terminal`. Each
/// step that succeeds is rolled back immediately if a later one fails, so this function
/// never returns an error while leaving raw mode or the alternate screen enabled with
/// nothing left to restore them.
fn init_terminal() -> Result<Tui> {
    enable_raw_mode()?;

    if let Err(err) = execute!(io::stdout(), EnterAlternateScreen) {
        tracing::error!(%err, "entering alternate screen failed, reverting raw mode");
        let _ = disable_raw_mode();
        return Err(err.into());
    }

    match Terminal::new(CrosstermBackend::new(io::stdout())) {
        Ok(terminal) => Ok(terminal),
        Err(err) => {
            tracing::error!(%err, "terminal construction failed, reverting terminal setup");
            let _ = restore_terminal();
            Err(err.into())
        }
    }
}

/// Restores raw mode and the primary screen. Safe to call more than once, and safe to
/// call when only part of `init_terminal()` succeeded: both steps are always attempted
/// independently, so one failing does not stop the other from being tried.
fn restore_terminal() -> Result<()> {
    let raw_mode_result = disable_raw_mode();
    let alt_screen_result = execute!(io::stdout(), LeaveAlternateScreen);
    raw_mode_result?;
    alt_screen_result?;
    Ok(())
}

fn install_panic_hook() {
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = restore_terminal();
        original_hook(panic_info);
    }));
}

fn run(terminal: &mut Tui, ascii_mode: bool) -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let (tx, rx) = mpsc::channel();
    let input_handle = spawn_input_thread(tx.clone(), Arc::clone(&running));

    let (sampler_cmd_tx, sampler_cmd_rx) = mpsc::channel();
    let sampler_handle = spawn_sampler_thread(tx, sampler_cmd_rx);

    let mut app = App::new(sampler_cmd_tx.clone(), ascii_mode);
    while !app.should_quit {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        match rx.recv() {
            Ok(event) => app.update(event),
            Err(_) => break,
        }
    }

    running.store(false, Ordering::Relaxed);
    let _ = sampler_cmd_tx.send(SamplerCommand::Shutdown);
    let _ = input_handle.join();
    let _ = sampler_handle.join();

    Ok(())
}

/// Parsed command-line options; `None` means the process already handled `--help`/`--version`
/// and should exit without starting the TUI.
struct CliArgs {
    ascii_mode: bool,
}

/// Hand-rolled arg parsing (no `clap`): the surface is tiny (`--ascii`, `--help`, `--version`)
/// and doesn't warrant a new dependency.
fn parse_args() -> Option<CliArgs> {
    let mut ascii_mode = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return None;
            }
            "-V" | "--version" => {
                println!("hanun {}", env!("CARGO_PKG_VERSION"));
                return None;
            }
            "--ascii" => ascii_mode = true,
            _ => {}
        }
    }
    Some(CliArgs { ascii_mode })
}

fn print_help() {
    println!("hanun {}", env!("CARGO_PKG_VERSION"));
    println!("Windows terminal resource monitor (CPU / memory / disk / network / process)");
    println!();
    println!("USAGE:");
    println!("    hanun [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    --ascii        Use ASCII-only borders/gauges instead of Unicode box-drawing");
    println!("    -h, --help     Print this help and exit");
    println!("    -V, --version  Print version and exit");
    println!();
    println!("KEYS (inside hanun):");
    println!("    q / Esc        Quit");
    println!("    Tab, 1-6       Focus panel");
    println!("    Up/Down, j k   Scroll focused list");
    println!("    s              Cycle sort");
    println!("    /              Filter processes");
    println!("    p              Pause / resume");
    println!("    +/-            Adjust refresh interval");
    println!("    g              Cycle chart (Mem/Disk/CPU)");
    println!("    ?              Help overlay");
}

fn main() -> Result<()> {
    let Some(cli) = parse_args() else {
        return Ok(());
    };

    color_eyre::install()?;
    let _tracing_guard = init_tracing();
    install_panic_hook();

    let mut terminal = init_terminal()?;
    let result = run(&mut terminal, cli.ascii_mode);
    restore_terminal()?;

    result
}
