use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, io::Write, net::TcpListener, path::Path};

pub const CONFIG: &str = ".rustyard.toml";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub name: String,
    pub port: u16,
    pub command: Vec<String>,
    pub watch: Vec<String>,
    /// TLD-free domains in primary-first order. Empty keeps Lerd's default name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub domains: Vec<String>,
    /// A separate site under another Lerd site's domain; the first domain above
    /// must be `<label>.<main>` and any remaining domains are retained aliases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<Group>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub main: String,
    pub label: String,
}

fn valid_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

fn valid_domain(domain: &str) -> bool {
    !domain.is_empty()
        && domain.len() <= 253
        && domain.split('.').all(valid_label)
        && !domain.ends_with(".test")
        && !domain.ends_with(".localhost")
}

impl Config {
    pub fn load(root: &Path) -> Result<Self> {
        let raw = fs::read_to_string(root.join(CONFIG)).context(
            "No .rustyard.toml found. Run `rustyard init` in your web application's directory.",
        )?;
        let config: Self = toml::from_str(&raw).context("Invalid .rustyard.toml")?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        let name = &self.name;
        if !valid_label(name) {
            bail!("Site name must be a DNS label: lowercase letters, digits, internal hyphens, at most 63 characters.");
        }
        if self.port < 1024 {
            bail!("Choose an unprivileged application port between 1024 and 65535.");
        }
        if self.command.is_empty() || self.command[0].is_empty() {
            bail!("command must be a nonempty argument array, such as [\"cargo\", \"run\"].");
        }
        if self.watch.is_empty() {
            bail!("watch must contain at least one project-relative path.");
        }
        for path in &self.watch {
            if path.is_empty()
                || Path::new(path).components().any(|c| {
                    !matches!(
                        c,
                        std::path::Component::Normal(_) | std::path::Component::CurDir
                    )
                })
            {
                bail!("Watch paths must stay within the project: {path}");
            }
        }
        let mut seen = HashSet::new();
        for domain in &self.domains {
            if !valid_domain(domain) || !seen.insert(domain) {
                bail!("domains must be unique TLD-free DNS names, such as shop.acme and shop");
            }
        }
        if let Some(group) = &self.group {
            if !valid_label(&group.main) || !valid_label(&group.label) || group.main == self.name {
                bail!("group.main and group.label must be DNS labels and the main must be another site");
            }
            let nested = format!("{}.{}", group.label, group.main);
            if self.domains.first() != Some(&nested) {
                bail!("grouped sites must list {nested} as the first domain");
            }
        }
        Ok(())
    }
}

pub fn init(
    root: &Path,
    name: Option<String>,
    port: Option<u16>,
    command: Vec<String>,
) -> Result<Config> {
    if root.join(CONFIG).exists() || root.join(".lerd.yaml").exists() {
        bail!("A Rustyard or Lerd config already exists. Nothing was overwritten. Use a separate checkout or review your existing configuration first.");
    }
    let manifest: toml::Value = toml::from_str(
        &fs::read_to_string(root.join("Cargo.toml"))
            .context("Run init in a Rust project containing Cargo.toml.")?,
    )?;
    let inferred = manifest
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|s| s.as_str())
        .or_else(|| root.file_name().and_then(|s| s.to_str()))
        .unwrap_or("app");
    let name = name.unwrap_or_else(|| inferred.replace(['_', '.'], "-").to_lowercase());
    let port = match port {
        Some(port) => port,
        None => TcpListener::bind(("0.0.0.0", 0))
            .context("Cannot select a free local port. Pass --port <number> to configure an explicit port without probing.")?
            .local_addr()?.port(),
    };
    let config = Config {
        name,
        port,
        command: if command.is_empty() {
            vec!["cargo".into(), "run".into()]
        } else {
            command
        },
        watch: [
            "src",
            "templates",
            "static",
            "assets",
            "migrations",
            "Cargo.toml",
            "Cargo.lock",
            "build.rs",
            ".env",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        domains: Vec::new(),
        group: None,
    };
    config.validate()?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(CONFIG))?;
    file.write_all(toml::to_string_pretty(&config)?.as_bytes())?;
    Ok(config)
}
