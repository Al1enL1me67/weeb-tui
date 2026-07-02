use weeb_tui::config::Config;
use weeb_tui::ui::App;
use weeb_tui::update::{InstallMethod, UpdateChecker, CURRENT_VERSION};
use anyhow::Result;
use clap::Parser;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "weeb-tui")]
#[command(about = "Netflix-inspired TUI for anime and manga streaming")]
#[command(version)]
struct Cli {
    /// Search query to start with
    #[arg(short, long)]
    query: Option<String>,

    /// Path to configuration file
    #[arg(short, long)]
    config: Option<String>,

    /// Enable debug mode
    #[arg(short, long)]
    debug: bool,

    /// Check for and install updates
    #[arg(long)]
    update: bool,

    /// Check for updates without installing
    #[arg(long)]
    check_update: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.debug {
        tracing_subscriber::fmt::init();
    }

    if cli.check_update {
        return handle_check_update().await;
    }

    if cli.update {
        return handle_self_update().await;
    }

    let config = Config::load()?;
    let db = Arc::new(weeb_tui::db::Database::new().await?);
    let mut app = App::new(config, db).await?;

    if let Some(query) = cli.query {
        app.set_initial_search(query);
    }

    app.run().await?;

    Ok(())
}

async fn handle_check_update() -> Result<()> {
    let checker = UpdateChecker::new();

    match checker.check().await {
        Ok(Some(result)) if result.has_update => {
            println!(
                "weeb-tui v{} is available (current: v{})",
                result.latest_version, result.current_version
            );

            let method = UpdateChecker::detect_install_method();
            match method {
                InstallMethod::Homebrew => {
                    println!("Run: brew upgrade weeb-tui");
                }
                InstallMethod::Scoop => {
                    println!("Run: scoop update weeb-tui");
                }
                InstallMethod::Binary => {
                    println!("Run: weeb-tui --update");
                }
            }
            println!("\nRelease notes: {}", result.release_url);
        }
        Ok(Some(_)) => {
            println!("weeb-tui is up to date (v{})", CURRENT_VERSION);
        }
        Ok(None) => {
            eprintln!("Could not check for updates. Please try again later.");
        }
        Err(e) => {
            eprintln!("Update check failed: {}", e);
        }
    }

    Ok(())
}

async fn handle_self_update() -> Result<()> {
    let method = UpdateChecker::detect_install_method();

    match method {
        InstallMethod::Homebrew => {
            eprintln!("Installed via Homebrew.");
            eprintln!("Please run: brew update && brew upgrade weeb-tui");
            std::process::exit(1);
        }
        InstallMethod::Scoop => {
            eprintln!("Installed via Scoop.");
            eprintln!("Please run: scoop update weeb-tui");
            std::process::exit(1);
        }
        InstallMethod::Binary => {}
    }

    println!("Checking for updates...");

    match UpdateChecker::self_update() {
        Ok(version) => {
            println!("\nSuccessfully updated to v{}!", version);
            println!("Please restart weeb-tui to use the new version.");
        }
        Err(e) => {
            eprintln!("\nUpdate failed: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
