use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rustyard"))
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = 'demo-app'\nversion = '0.1.0'\nedition = '2021'\n",
    )
    .unwrap();
    dir
}

fn initialize(root: &Path) {
    let result = cli(root, &["init", "--port", "3107"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn initialization_is_offline_and_dry_run_does_not_create_adapter() {
    let dir = project();
    initialize(dir.path());
    let result = cli(dir.path(), &["up", "--dry-run"]);
    assert!(result.status.success());
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(output.contains("port: 3107"));
    assert!(output.contains("lerd link demo-app"));
    assert!(!dir.path().join(".lerd.yaml").exists());
}

#[test]
fn refuses_existing_configs_and_invalid_domains_without_writing() {
    let dir = project();
    let result = cli(
        dir.path(),
        &["init", "--name", "bad;touch-pwned", "--port", "3107"],
    );
    assert!(!result.status.success());
    assert!(!dir.path().join(".rustyard.toml").exists());
    fs::write(dir.path().join(".lerd.yaml"), "original\n").unwrap();
    assert!(!cli(dir.path(), &["init", "--port", "3107"])
        .status
        .success());
    assert_eq!(
        fs::read_to_string(dir.path().join(".lerd.yaml")).unwrap(),
        "original\n"
    );
}

#[test]
fn new_scaffolds_a_configured_app_without_executing_it_and_preserves_existing_files() {
    let dir = tempfile::tempdir().unwrap();
    let result = cli(dir.path(), &["new", "hello", "--port", "3111"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let app = dir.path().join("hello");
    let config = fs::read_to_string(app.join(".rustyard.toml")).unwrap();
    assert!(config.contains("name = \"hello\""));
    assert!(config.contains("port = 3111"));
    assert!(fs::read_to_string(app.join("src/main.rs"))
        .unwrap()
        .contains("/health"));
    assert!(fs::read_to_string(app.join("Cargo.lock"))
        .unwrap()
        .contains("name = \"hello\""));
    assert!(!app.join(".lerd.yaml").exists());
    fs::write(app.join("src/main.rs"), "owner changes").unwrap();
    assert!(!cli(dir.path(), &["new", "hello", "--port", "3111"])
        .status
        .success());
    assert_eq!(
        fs::read_to_string(app.join("src/main.rs")).unwrap(),
        "owner changes"
    );
    assert!(!cli(
        dir.path(),
        &["new", "invalid", "--name", "bad;name", "--port", "3111"]
    )
    .status
    .success());
    assert!(!dir.path().join("invalid").exists());
}

#[cfg(unix)]
#[test]
fn invalid_scaffold_configuration_leaves_no_directory() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("owner.txt"), "keep").unwrap();
    assert!(!cli(dir.path(), &["new", "app", "--port", "80"])
        .status
        .success());
    assert!(!dir.path().join("app").exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("owner.txt")).unwrap(),
        "keep"
    );
}

#[cfg(unix)]
#[test]
fn readiness_requires_2xx_and_expected_body_and_never_disables_tls() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let curl = dir.path().join("curl");
    fs::write(&curl, "#!/bin/sh\nprintf '%s\\n' \"$@\" > curl-args\nprintf '%s' \"$RESPONSE\"\nexit \"${CURL_EXIT:-0}\"\n").unwrap();
    fs::set_permissions(&curl, fs::Permissions::from_mode(0o700)).unwrap();
    for (response, success) in [
        ("ready\n200", true),
        ("wrong\n200", false),
        ("ready\n302", false),
        ("ready\n404", false),
        ("ready\n503", false),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_rustyard"))
            .current_dir(dir.path())
            .args([
                "verify",
                "--url",
                "https://hello.test/health",
                "--contains",
                "ready",
                "--timeout",
                "0",
            ])
            .env("PATH", dir.path())
            .env("RESPONSE", response)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            success,
            "{response}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let args = fs::read_to_string(dir.path().join("curl-args")).unwrap();
        assert!(!args.lines().any(|arg| matches!(arg, "-k" | "--insecure")));
        assert!(args.contains("--disable\n"));
        assert!(args.contains("--noproxy\n*\n"));
    }
    assert!(!cli(
        dir.path(),
        &["verify", "--url", "file:///etc/passwd", "--timeout", "0"]
    )
    .status
    .success());
}

#[cfg(unix)]
#[test]
fn doctor_propagates_infrastructure_failure_instead_of_reporting_tools_as_healthy() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    for tool in ["cargo", "rustc", "curl"] {
        let path = dir.path().join(tool);
        fs::write(&path, "#!/bin/sh\necho tool-version\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mock = dir.path().join("lerd");
    fs::write(&mock, "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'lerd version 1.33.1'; exit 0; fi\nexit 7\n").unwrap();
    fs::set_permissions(&mock, fs::Permissions::from_mode(0o700)).unwrap();
    for (args, success) in [
        (vec!["doctor", "--tools-only"], true),
        (vec!["doctor"], false),
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_rustyard"))
            .current_dir(dir.path())
            .args(args)
            .env("PATH", dir.path())
            .env("RUSTYARD_LERD", &mock)
            .output()
            .unwrap();
        assert_eq!(result.status.success(), success);
    }
    fs::write(&mock, "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'lerd version 1.33.1'; exit 0; fi\necho '6 failure(s), 3 warning(s) found.'\nexit 0\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rustyard"))
        .current_dir(dir.path())
        .arg("doctor")
        .env("PATH", dir.path())
        .env("RUSTYARD_LERD", &mock)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Lerd infrastructure checks failed"));
}

#[cfg(unix)]
fn backend(root: &Path, failure: bool) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = root.join("mock-lerd");
    fs::write(&path, format!("#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'lerd version 1.33.1'; exit 0; fi\nprintf '%s\\n' \"$*\" >> calls.log\n{}\n", if failure { "if [ \"$1\" = link ]; then exit 7; fi" } else { "exit 0" })).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}

#[cfg(unix)]
#[test]
fn backend_validation_precedes_link_and_errors_propagate() {
    let dir = project();
    initialize(dir.path());
    let mock = backend(dir.path(), true);
    let output = Command::new(env!("CARGO_BIN_EXE_rustyard"))
        .arg("-C")
        .arg(dir.path())
        .args(["up", "--yes"])
        .env("RUSTYARD_LERD", mock)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("partially applied"));
    assert_eq!(
        fs::read_to_string(dir.path().join("calls.log")).unwrap(),
        "check\nlink demo-app --yes\n"
    );
    // Drift is rejected without invoking the backend's check/link or losing edits.
    fs::write(dir.path().join(".lerd.yaml"), "hand-edited\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rustyard"))
        .arg("-C")
        .arg(dir.path())
        .args(["up", "--yes"])
        .env("RUSTYARD_LERD", dir.path().join("mock-lerd"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(dir.path().join(".lerd.yaml")).unwrap(),
        "hand-edited\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("calls.log")).unwrap(),
        "check\nlink demo-app --yes\n"
    );
}

#[cfg(unix)]
#[test]
fn grouped_up_retains_both_domains_after_relink_and_recovers_from_alias_conflict() {
    use std::os::unix::fs::PermissionsExt;
    let dir = project();
    initialize(dir.path());
    let config = dir.path().join(".rustyard.toml");
    let mut contents = fs::read_to_string(&config).unwrap();
    contents.push_str(
        "domains = [\"shop.acme\", \"shop\"]\n[group]\nmain = \"acme\"\nlabel = \"shop\"\n",
    );
    contents = contents.replace("name = \"demo-app\"", "name = \"shop\"");
    fs::write(config, contents).unwrap();
    // Grouped no-argument re-link needs the project directory to have its site name.
    let app = dir.path().join("shop");
    fs::create_dir(&app).unwrap();
    for file in ["Cargo.toml", ".rustyard.toml"] {
        fs::rename(dir.path().join(file), app.join(file)).unwrap();
    }
    let mock = app.join("mock-lerd");
    fs::write(
        &mock,
        r#"#!/bin/sh
if [ "$1" = --version ]; then echo 'lerd version 1.37.1'; exit 0; fi
printf '%s\n' "$*" >> calls.log
case "$1 $2" in
  'check ') exit 0 ;;
  'domain list')
    if [ ! -f domains-state ]; then
      echo "no site registered for this directory — run 'lerd link' first" >&2
      exit 1
    fi
    cat domains-state
    exit 0 ;;
  'domain add')
    if [ -f alias-conflict ]; then echo 'domain already used by another site' >&2; exit 7; fi
    printf '  shop.acme.test (primary)\n  shop.test\n' > domains-state
    exit 0 ;;
  'group list')
    if [ -f group-state ]; then cat group-state; else echo 'No site groups.'; fi
    exit 0 ;;
  'group add')
    printf 'acme (acme.test)\n  shop -> shop.acme.test\n' > group-state
    printf '  shop.acme.test (primary)\n' > domains-state
    exit 0 ;;
  'link shop')
    printf '  shop.test (primary)\n  shop.acme.test\n' > domains-state
    exit 0 ;;
  'link --yes') exit 0 ;;
esac
echo 'unexpected mock command' >&2
exit 9
"#,
    )
    .unwrap();
    fs::set_permissions(&mock, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(app.join("alias-conflict"), "").unwrap();
    let first = Command::new(env!("CARGO_BIN_EXE_rustyard"))
        .args(["-C", app.to_str().unwrap(), "up", "--yes"])
        .env("RUSTYARD_LERD", &mock)
        .output()
        .unwrap();
    assert!(!first.status.success());
    assert!(String::from_utf8_lossy(&first.stderr).contains("domain add"));
    fs::remove_file(app.join("alias-conflict")).unwrap();
    for _ in 0..2 {
        let result = Command::new(env!("CARGO_BIN_EXE_rustyard"))
            .args(["-C", app.to_str().unwrap(), "up", "--yes"])
            .env("RUSTYARD_LERD", &mock)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let calls = fs::read_to_string(app.join("calls.log")).unwrap();
    assert_eq!(calls.matches("group add acme shop\n").count(), 1);
    assert_eq!(calls.matches("domain add shop\n").count(), 2); // conflict, then recovery
    assert!(calls.contains("link shop --yes\n"));
    assert_eq!(calls.matches("link --yes\n").count(), 2); // no explicit name on repeat
    assert_eq!(
        fs::read_to_string(app.join("domains-state")).unwrap(),
        "  shop.acme.test (primary)\n  shop.test\n"
    );
}

#[cfg(unix)]
#[test]
fn dev_restarts_on_edits_passes_bind_env_and_stops_descendants_on_sigterm() {
    let dir = project();
    let script = "trap 'kill \"$kid\" 2>/dev/null; wait \"$kid\"; exit 0' TERM INT\nsleep 120 &\nkid=$!\nprintf '%s %s %s %s\\n' \"$HOST\" \"$PORT\" \"$$\" \"$kid\" >> runs.log\nwait \"$kid\"\n";
    fs::write(dir.path().join("server.sh"), script).unwrap();
    let output = cli(
        dir.path(),
        &["init", "--port", "3107", "--", "sh", "server.sh"],
    );
    assert!(output.status.success());
    let child = Command::new(env!("CARGO_BIN_EXE_rustyard"))
        .arg("-C")
        .arg(dir.path())
        .arg("dev")
        .env("HOST", "127.0.0.9")
        .env("PORT", "4119")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    struct Cleanup(std::process::Child);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            unsafe {
                libc::kill(self.0.id() as i32, libc::SIGTERM);
            }
            for _ in 0..40 {
                if self.0.try_wait().unwrap().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut child = Cleanup(child);
    let log = dir.path().join("runs.log");
    let wait_for = |count| {
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            let text = fs::read_to_string(&log).unwrap_or_default();
            if text.lines().count() >= count {
                return text;
            }
            assert!(
                Instant::now() < until,
                "runner did not start/restart in time"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    let first = wait_for(1);
    assert!(first.starts_with("127.0.0.9 4119 "));
    fs::write(dir.path().join("src/main.rs"), "// changed").unwrap();
    let restarted = wait_for(2);
    let pids: Vec<i32> = restarted
        .lines()
        .flat_map(|line| {
            line.split_whitespace()
                .skip(2)
                .map(|pid| pid.parse().unwrap())
        })
        .collect();
    unsafe {
        libc::kill(child.0.id() as i32, libc::SIGTERM);
    }
    let until = Instant::now() + Duration::from_secs(5);
    while child.0.try_wait().unwrap().is_none() {
        assert!(Instant::now() < until, "runner did not stop");
        std::thread::sleep(Duration::from_millis(50));
    }
    for pid in pids {
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            -1,
            "descendant {pid} was left running"
        );
    }
}
