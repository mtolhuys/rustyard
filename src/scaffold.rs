use anyhow::{bail, Context, Result};
use std::{fs, path::Path};

pub fn create(path: &Path, name: Option<String>, port: Option<u16>) -> Result<()> {
    if path.exists() {
        bail!(
            "Destination already exists: {}. Nothing was overwritten.",
            path.display()
        );
    }
    let inferred = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("Choose a project directory name")?;
    let name = name.unwrap_or_else(|| inferred.replace(['_', '.'], "-").to_lowercase());
    // Validate before creating anything. The same DNS label is a legal Cargo package name.
    let config = crate::config::Config {
        name: name.clone(),
        port: port.unwrap_or(3100),
        command: vec!["cargo".into(), "run".into()],
        watch: vec!["src".into()],
        domains: Vec::new(),
        group: None,
    };
    config.validate()?;
    if matches!(
        name.as_str(),
        "test"
            | "build"
            | "deps"
            | "examples"
            | "incremental"
            | "self"
            | "super"
            | "crate"
            | "std"
            | "core"
            | "alloc"
    ) || name.starts_with(|c: char| c.is_ascii_digit())
    {
        bail!(
            "Choose a Cargo package name beginning with a letter and not reserved by Cargo/Rust."
        );
    }
    // Reserve the final directory exclusively; never adopt an existing directory.
    fs::create_dir(path).context("Cannot create project; the parent directory must exist")?;
    let result = (|| -> Result<()> {
        fs::create_dir(path.join("src"))?;
        let manifest =
            include_str!("../examples/hello/Cargo.toml").replace("rustyard-hello", &name);
        fs::write(path.join("Cargo.toml"), manifest)?;
        fs::write(
            path.join("Cargo.lock"),
            include_str!("../examples/hello/Cargo.lock").replace("rustyard-hello", &name),
        )?;
        fs::write(
            path.join("src/main.rs"),
            include_str!("../examples/hello/src/main.rs"),
        )?;
        fs::write(path.join(".gitignore"), "/target/\n/.lerd.yaml\n/.env\n")?;
        fs::write(
            path.join("AGENTS.md"),
            include_str!("../templates/project-AGENTS.md"),
        )?;
        crate::config::init(path, Some(name), port, vec![])?;
        fs::write(path.join("README.md"), "# Local Rust website\n\nRun `rustyard up`, then `rustyard open`. Edit `src/main.rs` to rebuild automatically.\n\n`rustyard logs --follow` shows build/app output. `rustyard stop` pauses the site.\nUse `rustyard dev` for foreground HTTP without Lerd.\n\nThe app reads HOST and PORT; /health returns ok. No production deployment is configured.\n")?;
        Ok(())
    })();
    if result.is_err() {
        // Only our newly reserved directory can reach this branch.
        let _ = fs::remove_dir_all(path);
    }
    result
}
