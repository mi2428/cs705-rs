//! Independent category-based settings TUI. All edits stay local until Apply.
#![forbid(unsafe_code)]

mod activity;
mod app;
mod catalog;
mod encoding;
mod model;
mod radio;
mod storage;
mod theme;

use clap::Parser;
use ratatui::crossterm::{
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
};
use std::path::PathBuf;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Parser)]
#[command(
    version,
    about = "IC-705 settings editing session; Ctrl+S reviews and saves all staged changes"
)]
struct Args {
    /// Encoding of stored names/text, not the UTF-8 terminal (cp932 is accepted).
    #[arg(long, default_value = "shift_jis")]
    encoding: encoding::TextEncoding,
    /// Canonical MapRev 5 or 6 ICF; no device is opened in file mode.
    #[arg(long, conflicts_with = "port", required_unless_present = "port")]
    file: Option<PathBuf>,
    /// Explicit serial endpoint. OS open can transiently assert DTR/RTS.
    #[arg(long, conflicts_with = "file", requires_all = ["allow_device_io", "acknowledge_line_risk"])]
    port: Option<String>,
    #[arg(long, requires = "port")]
    allow_device_io: bool,
    #[arg(long, requires = "port")]
    acknowledge_line_risk: bool,
    /// Private backup and edited-file destination; originals are never overwritten.
    #[arg(long)]
    backup_dir: PathBuf,
    #[arg(long, default_value_t = 115200, value_parser = clap::value_parser!(u32).range(300..=4_000_000))]
    baud: u32,
    #[arg(long, default_value_t = 1000, value_parser = clap::value_parser!(u64).range(1..=60000))]
    timeout_ms: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let source = match (args.file, args.port) {
        (Some(path), None) => app::Source::File(path),
        (None, Some(port)) if args.allow_device_io && args.acknowledge_line_risk => {
            app::Source::Radio(radio::Radio {
                port,
                baud: args.baud,
                timeout_ms: args.timeout_ms,
            })
        }
        _ => return Err("select one file or an explicitly authorized radio endpoint".into()),
    };
    let mut application = app::App::new(source, args.backup_dir);
    application.set_encoding(args.encoding);
    let mut terminal = ratatui::try_init()?;
    let result: Result<()> = (|| {
        execute!(std::io::stdout(), EnableBracketedPaste, EnableMouseCapture)?;
        application.run(&mut terminal)
    })();
    application.shutdown();
    let input_restore = execute!(
        std::io::stdout(),
        DisableMouseCapture,
        DisableBracketedPaste
    );
    ratatui::restore();
    if application.needs_verification() {
        eprintln!(
            "Radio write pending or unconfirmed. Keep the backup and verify after restart before further writes."
        );
    }
    result?;
    input_restore?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_consent_is_required_before_constructing_a_radio() {
        assert!(
            Args::try_parse_from(["tui", "--port", "never-open", "--backup-dir", "x"]).is_err()
        );
        assert!(
            Args::try_parse_from([
                "tui",
                "--port",
                "never-open",
                "--backup-dir",
                "x",
                "--allow-device-io"
            ])
            .is_err()
        );
        assert!(
            Args::try_parse_from([
                "tui",
                "--port",
                "never-open",
                "--backup-dir",
                "x",
                "--allow-device-io",
                "--acknowledge-line-risk"
            ])
            .is_ok()
        );
        assert!(Args::try_parse_from(["tui", "--file", "unused", "--backup-dir", "x"]).is_ok());
        assert!(
            Args::try_parse_from([
                "tui",
                "--file",
                "unused",
                "--port",
                "never-open",
                "--backup-dir",
                "x",
                "--allow-device-io",
                "--acknowledge-line-risk"
            ])
            .is_err()
        );
    }
}
