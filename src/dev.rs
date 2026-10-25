use crate::config::Config;
use anyhow::{bail, Context, Result};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{
    collections::BTreeMap,
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    process::{Child, Command},
    time::Duration,
};

type Snapshot = BTreeMap<PathBuf, u64>;

fn visit(path: &Path, snapshot: &mut Snapshot) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("Cannot watch {}", path.display()))
        }
    };
    // Never follow links out of a project, or into directory loops.
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_dir() {
        if matches!(
            path.file_name().and_then(|s| s.to_str()),
            Some("target" | ".git" | "node_modules")
        ) {
            return Ok(());
        }
        for entry in fs::read_dir(path)? {
            visit(&entry?.path(), snapshot)?;
        }
    } else if metadata.is_file() {
        let mut hasher = DefaultHasher::new();
        match fs::read(path) {
            Ok(bytes) => bytes.hash(&mut hasher),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        }
        snapshot.insert(path.to_owned(), hasher.finish());
    }
    Ok(())
}

pub fn snapshot(root: &Path, paths: &[String]) -> Result<Snapshot> {
    let mut result = Snapshot::new();
    for path in paths {
        let path = root.join(path);
        if let Ok(resolved) = path.canonicalize() {
            if !resolved.starts_with(root) {
                bail!(
                    "Watch path resolves outside the project: {}",
                    path.display()
                );
            }
        }
        visit(&path, &mut result)?;
    }
    Ok(result)
}

struct Process(Option<Child>);

impl Process {
    fn spawn(root: &Path, config: &Config) -> Result<Self> {
        let mut command = Command::new(&config.command[0]);
        command
            .args(&config.command[1..])
            .current_dir(root)
            .env(
                "PORT",
                std::env::var("PORT").unwrap_or_else(|_| config.port.to_string()),
            )
            .env(
                "HOST",
                std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            );
        #[cfg(unix)]
        command.process_group(0);
        Ok(Self(Some(command.spawn().context(
            "Could not start app command. Check command in .rustyard.toml.",
        )?)))
    }

    fn stop(&mut self) {
        let Some(mut child) = self.0.take() else {
            return;
        };
        #[cfg(unix)]
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGTERM);
        }
        // Stop cargo and all descendants before the replacement can bind the port.
        for _ in 0..20 {
            #[cfg(unix)]
            let group_exists = unsafe { libc::kill(-(child.id() as i32), 0) == 0 };
            #[cfg(not(unix))]
            let group_exists = false;
            let _ = child.try_wait();
            if !group_exists {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        #[cfg(unix)]
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.stop();
    }
}

pub async fn run(root: &Path, config: &Config) -> Result<()> {
    if !cfg!(unix) {
        bail!("The dev runner currently supports Linux and macOS.");
    }
    let mut previous = snapshot(root, &config.watch)?;
    eprintln!(
        "Rustyard: running {:?}. Watching source files; Ctrl-C stops the app.",
        config.command
    );
    let mut process = Process::spawn(root, config)?;
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
    let mut exit_reported = false;
    loop {
        tokio::select! {
            result = &mut shutdown => { result?; return Ok(()); }
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                let next = snapshot(root, &config.watch)?;
                if next != previous {
                    // Debounce bursts, then capture generated/deleted files as part of this change.
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    previous = snapshot(root, &config.watch)?;
                    eprintln!("Rustyard: source changed, restarting…");
                    process.stop();
                    process = Process::spawn(root, config)?;
                    exit_reported = false;
                } else if !exit_reported {
                    if let Some(status) = process.0.as_mut().expect("active process").try_wait()? {
                        // Reap/stop remaining descendants now; do not retain an old process-group ID.
                        process.stop();
                        eprintln!("Rustyard: app exited ({status}). Waiting for a source change; build/runtime output is above.");
                        exit_reported = true;
                    }
                }
            }
        }
    }
}

async fn shutdown_signal() -> Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut terminate = signal(SignalKind::terminate())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result?,
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_same_length_edits_additions_and_deletions_but_skips_build_outputs() -> Result<()> {
        let dir = tempfile::tempdir()?;
        fs::create_dir_all(dir.path().join("src/target"))?;
        fs::write(dir.path().join("src/main.rs"), "first")?;
        let paths = vec!["src".into(), "templates".into()];
        let before = snapshot(dir.path(), &paths)?;
        fs::write(dir.path().join("src/target/output"), "ignored")?;
        assert_eq!(before, snapshot(dir.path(), &paths)?);
        fs::write(dir.path().join("src/main.rs"), "other")?;
        let edited = snapshot(dir.path(), &paths)?;
        assert_ne!(before, edited);
        fs::create_dir(dir.path().join("templates"))?;
        fs::write(dir.path().join("templates/index.html"), "new")?;
        assert_ne!(edited, snapshot(dir.path(), &paths)?);
        fs::remove_file(dir.path().join("templates/index.html"))?;
        assert_eq!(edited, snapshot(dir.path(), &paths)?);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn skips_symlinks_including_cycles() -> Result<()> {
        let dir = tempfile::tempdir()?;
        fs::create_dir(dir.path().join("src"))?;
        std::os::unix::fs::symlink(dir.path(), dir.path().join("src/loop"))?;
        assert!(snapshot(dir.path(), &["src".into()])?.is_empty());
        Ok(())
    }
}
