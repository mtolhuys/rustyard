mod config;
mod dev;
mod lerd;
mod scaffold;
mod verify;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::{path::PathBuf, process::Command};

#[derive(Parser)]
#[command(version, about = "Rust-first local websites, powered by Lerd")]
struct Cli {
    #[arg(short = 'C', long, global = true, default_value = ".")]
    directory: PathBuf,
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Create a ready-to-run Axum website. Does not execute or link it.
    New {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        port: Option<u16>,
    },
    /// Configure an existing Rust web app without changing the machine or starting anything.
    Init {
        #[arg(long)]
        name: Option<String>,
        /// Explicit port; omitted selects a currently free port.
        #[arg(long)]
        port: Option<u16>,
        /// App arguments after --, e.g. -- cargo run --bin server.
        #[arg(last = true)]
        command: Vec<String>,
    },
    /// Write the local Lerd adapter, validate it, and link the site with HTTPS.
    Up {
        /// Approve running this project's command on the host, without Lerd's prompt.
        #[arg(long)]
        yes: bool,
        /// Print the adapter and steps without writing or running anything.
        #[arg(long)]
        dry_run: bool,
        /// Verify this exact HTTPS URL after linking, including DNS and certificate trust.
        #[arg(long)]
        url: Option<String>,
        #[arg(long, default_value_t = 120)]
        timeout: u64,
    },
    /// Run the app in the foreground and restart on source changes.
    Dev,
    /// Resume a previously linked site.
    Start,
    /// Pause the site and stop its app processes.
    Stop,
    /// Restart a previously linked site.
    Restart,
    /// Open the site using Lerd's actual domain and configured TLD.
    Open,
    /// Open the existing Lerd dashboard (sites, services, dev-server logs).
    Dashboard,
    /// Remove the site's Lerd registration; retain project config.
    Unlink,
    /// List sites registered with Lerd.
    Sites,
    /// Read the app's Dev Server logs (Linux journal; dashboard on macOS).
    Logs {
        #[arg(short, long)]
        follow: bool,
        #[arg(short = 'n', long, default_value_t = 100)]
        lines: u32,
    },
    /// Wait for an exact URL to return HTTP 2xx. HTTPS verifies DNS and TLS trust.
    Verify {
        #[arg(long)]
        url: String,
        #[arg(long, default_value_t = 120)]
        timeout: u64,
        /// Also require this text in the response (useful for end-to-end acceptance).
        #[arg(long)]
        contains: Option<String>,
    },
    /// Check installed prerequisites without changing them.
    Doctor {
        /// Only check installed tools; skip live Lerd infrastructure checks.
        #[arg(long)]
        tools_only: bool,
    },
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = cli.directory.canonicalize()?;
    match cli.command {
        Action::New { path, name, port } => {
            let path = root.join(path);
            scaffold::create(&path, name, port)?;
            println!(
                "Created {}. Next: rustyard -C '{}' up",
                path.display(),
                path.display()
            );
        }
        Action::Init {
            name,
            port,
            command,
        } => {
            let config = config::init(&root, name, port, command)?;
            println!("Configured {} on port {}. No server was started.\nYour app must read HOST and PORT. Next: rustyard up", config.name, config.port);
        }
        Action::Doctor { tools_only } => {
            let mut failures = 0;
            for tool in ["cargo", "rustc", "curl"] {
                match Command::new(tool).arg("--version").output() {
                    Ok(output) if output.status.success() => {
                        println!("OK: {}", String::from_utf8_lossy(&output.stdout).trim())
                    }
                    _ => {
                        eprintln!("MISSING: {tool}");
                        failures += 1;
                    }
                }
            }
            match lerd::check_version() {
                Ok(()) => println!("OK: Lerd supports host-proxy sites"),
                Err(error) => {
                    eprintln!("FAIL: {error:#}");
                    failures += 1;
                }
            }
            println!("Platform: {}", std::env::consts::OS);
            if !cfg!(unix) {
                failures += 1;
            }
            if failures > 0 {
                bail!("{failures} prerequisite check(s) failed");
            }
            if !tools_only {
                // A version check alone never establishes that local sites work.
                lerd::doctor(&root)?;
                let _listener = std::net::TcpListener::bind(("127.0.0.1", 0))
                    .map_err(|error| anyhow::anyhow!("Cannot bind a local app socket: {error}. Run on the host with local network access."))?;
            }
        }
        Action::Dashboard => {
            lerd::run(&root, &["dashboard"])?;
        }
        Action::Sites => lerd::run(&root, &["sites"])?,
        Action::Verify {
            url,
            timeout,
            contains,
        } => {
            verify::wait(&url, timeout, contains.as_deref()).await?;
        }
        action => {
            let config = config::Config::load(&root)?;
            match action {
                Action::Up {
                    yes,
                    dry_run,
                    url,
                    timeout,
                } => {
                    if let Some(url) = &url {
                        verify::validate_url(url)?;
                    }
                    let rendered = lerd::render(&config, &std::env::current_exe()?)?;
                    if dry_run {
                        print!("{rendered}\nWould validate .lerd.yaml, run `lerd check`, then `lerd link {}{}`.\nApp: {:?}\n", config.name, if yes { " --yes" } else { "" }, config.command);
                        if let Some(group) = &config.group {
                            println!("Would ensure group under {} at {} and retain declared aliases through Lerd.", group.main, group.label);
                        }
                        return Ok(());
                    }
                    lerd::check_version()?;
                    lerd::write_adapter(&root, &rendered, &config)?;
                    lerd::run(&root, &["check"])?;
                    if config.group.is_some() {
                        lerd::up_grouped(&root, &config, yes)?;
                    } else {
                        let mut args = vec!["link", config.name.as_str()];
                        if yes {
                            args.push("--yes");
                        }
                        lerd::run(&root, &args)?;
                    }
                    println!("Link command completed. Lerd's output lists the domain. Compilation may still be in progress; check the Dev Server logs in `rustyard dashboard`.");
                    if let Some(url) = url {
                        verify::wait(&url, timeout, None).await?;
                    }
                }
                Action::Dev => dev::run(&root, &config).await?,
                Action::Start => lerd::run(&root, &["unpause"])?,
                Action::Stop => lerd::run(&root, &["pause"])?,
                Action::Restart => lerd::run(&root, &["restart"])?,
                Action::Open => lerd::run(&root, &["open"])?,
                Action::Unlink => lerd::run(&root, &["unlink"])?,
                Action::Logs { follow, lines } => lerd::logs(&root, &config.name, follow, lines)?,
                _ => unreachable!(),
            }
        }
    }
    Ok(())
}
