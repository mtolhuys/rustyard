#!/usr/bin/env python3
"""Real host acceptance: Axum HTTP/HTTPS, rebuild, compile-error recovery, shutdown.

HTTPS mode explicitly registers one temporary site in Lerd, then unlinks it.
No resolver, CA, system service or other project is changed by this script.
"""
import argparse
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time
import tomllib
import urllib.error
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runner", default=str(Path(__file__).resolve().parents[1] / ".local/bin/rustyard"))
    parser.add_argument("--https", action="store_true")
    parser.add_argument("--tld", default="test")
    args = parser.parse_args()
    runner = str(Path(args.runner).resolve())
    project_root = Path(__file__).resolve().parents[1]
    # Fail before creating a site if this execution environment prohibits sockets.
    try:
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]
    except PermissionError as error:
        raise SystemExit(f"BLOCKED: this session cannot create local network sockets ({error}). Run this test in your host terminal.") from error
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def run(*command):
        subprocess.run([runner, *map(str, command)], check=True)

    def wait_body(url, expected, seconds=180):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            try:
                with opener.open(url, timeout=2) as response:
                    if 200 <= response.status < 300 and expected in response.read().decode():
                        return
            except (OSError, urllib.error.URLError):
                pass
            time.sleep(0.3)
        raise RuntimeError(f"Did not get {expected!r} from {url}; inspect the app log")

    with tempfile.TemporaryDirectory(prefix="rustyard-smoke-") as folder:
        app = Path(folder) / "app"
        name = f"rustyard-smoke-{os.getpid()}"
        run("new", app, "--name", name, "--port", port)
        config_path = app / ".rustyard.toml"
        # Share the build cache across smoke runs, including Lerd's worker.
        config = tomllib.loads(config_path.read_text())
        config["command"] = ["cargo", "run", "--target-dir", str(project_root / "target/smoke")]
        config_path.write_text("\n".join(f"{key} = {json.dumps(value)}" for key, value in config.items()) + "\n")
        source = app / "src/main.rs"
        original = source.read_text()
        process = None
        linked = False
        log_path = project_root / "target/smoke.log"
        log_path.parent.mkdir(parents=True, exist_ok=True)
        base = f"https://{name}.{args.tld}" if args.https else f"http://127.0.0.1:{port}"
        with log_path.open("w") as log:
            try:
                if args.https:
                    linked = True  # Also clean up a partially successful link.
                    run("-C", app, "up", "--yes")
                else:
                    process = subprocess.Popen([runner, "-C", str(app), "dev"], stdout=log, stderr=log)
                wait_body(base + "/health", "ok")
                wait_body(base, "Hello from Rustyard")
                source.write_text(original.replace("Hello from Rustyard", "Reload acceptance passed"))
                wait_body(base, "Reload acceptance passed")
                if process:
                    # A failed compile must be reported, and the watcher must survive.
                    source.write_text(original + "\nthis is intentionally invalid Rust;\n")
                    deadline = time.monotonic() + 30
                    while "app exited" not in log_path.read_text():
                        if time.monotonic() >= deadline:
                            raise RuntimeError("Compile failure was not reported")
                        time.sleep(0.2)
                    source.write_text(original)
                    wait_body(base + "/health", "ok")
                if args.https:
                    run("-C", app, "stop")
                    run("-C", app, "start")
                    wait_body(base + "/health", "ok")
            finally:
                if process:
                    process.send_signal(signal.SIGTERM)
                    try:
                        process.wait(timeout=8)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
                        raise RuntimeError("Watcher did not shut down cleanly")
                if linked:
                    run("-C", app, "unlink")
        # No app may retain the port after shutdown or unlink.
        with socket.socket() as probe:
            probe.settimeout(2)
            if probe.connect_ex(("127.0.0.1", port)) == 0:
                raise RuntimeError("App is still listening after shutdown")
        print(f"PASS: {'HTTPS/DNS/TLS, rebuild, pause/resume, unlink' if args.https else 'HTTP, rebuild, compile-error recovery, shutdown'}")


if __name__ == "__main__":
    main()
