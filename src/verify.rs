use anyhow::{bail, Context, Result};
use std::{
    process::Command,
    time::{Duration, Instant},
};

pub fn validate_url(url: &str) -> Result<()> {
    if !(url.starts_with("https://") || url.starts_with("http://"))
        || url.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        bail!("Provide an explicit http:// or https:// URL without whitespace.");
    }
    Ok(())
}

pub async fn wait(url: &str, timeout: u64, contains: Option<&str>) -> Result<()> {
    validate_url(url)?;
    let deadline = Instant::now() + Duration::from_secs(timeout);
    eprintln!("Checking {url} (HTTP 2xx required; HTTPS certificate verification enabled)…");
    loop {
        let remaining = deadline
            .saturating_duration_since(Instant::now())
            .as_secs_f64()
            .clamp(0.1, 2.0);
        let output = Command::new("curl")
            .args([
                "--disable",
                "--silent",
                "--show-error",
                "--globoff",
                "--noproxy",
                "*",
                "--proto",
                "=http,https",
                "--connect-timeout",
                "1",
                "--max-time",
                &remaining.to_string(),
                "--write-out",
                "\n%{http_code}",
                "--url",
                url,
            ])
            .output()
            .context("curl is required for HTTP and trusted HTTPS checks")?;
        let response = String::from_utf8_lossy(&output.stdout);
        let (body, code) = response.rsplit_once('\n').unwrap_or(("", "000"));
        let status = code.parse::<u16>().unwrap_or(0);
        let matched = contains.is_none_or(|expected| body.contains(expected));
        if output.status.success() && (200..300).contains(&status) && matched {
            println!("Ready: {url} (HTTP {status})");
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("URL did not become ready within {timeout}s: {url}; HTTP {code}; body matched: {matched}. {}\nInspect `rustyard logs --follow` and `lerd doctor`. The site may still be registered/running.", String::from_utf8_lossy(&output.stderr).trim());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
