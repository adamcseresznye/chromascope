"""Profile the opt-in Rust workload with sampled process-tree CPU/RSS/I/O.

Requires existing psutil. Never installs packages or changes source data.
Output directory must be new. Samples are observations, not a certified RSS bound.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import time

import psutil


def profile(command, directory, scans):
    env = dict(os.environ, CHROMASCOPE_BENCH_SCANS=str(scans))
    records = []
    seen = {}
    started = time.perf_counter()
    with (directory / "stdout.log").open("wb") as stdout, (
        directory / "stderr.log"
    ).open("wb") as stderr:
        child = subprocess.Popen(command, stdout=stdout, stderr=stderr, env=env)
        process = psutil.Process(child.pid)
        while child.poll() is None:
            rss = 0
            try:
                processes = [process, *process.children(recursive=True)]
            except psutil.NoSuchProcess:
                processes = []
            for proc in processes:
                try:
                    cpu = proc.cpu_times()
                    io = proc.io_counters()
                    memory = proc.memory_info()
                    rss += memory.rss
                    seen[proc.pid] = {
                        "cpu_seconds": cpu.user + cpu.system,
                        "read_bytes": io.read_bytes,
                        "write_bytes": io.write_bytes,
                    }
                except (psutil.NoSuchProcess, psutil.AccessDenied):
                    pass
            records.append(
                {
                    "elapsed_seconds": time.perf_counter() - started,
                    "unix_seconds": time.time(),
                    "rss_bytes": rss,
                    "cpu_seconds": sum(p["cpu_seconds"] for p in seen.values()),
                    "read_bytes": sum(p["read_bytes"] for p in seen.values()),
                    "write_bytes": sum(p["write_bytes"] for p in seen.values()),
                }
            )
            time.sleep(0.01)
        code = child.wait()
    lines = (directory / "stdout.log").read_text(errors="replace").splitlines()
    metrics = [
        json.loads(line.split("BENCH_JSON ", 1)[1])
        for line in lines
        if "BENCH_JSON " in line
    ]
    for metric in metrics:
        if "started_unix_seconds" not in metric:
            continue
        lo = metric["started_unix_seconds"]
        hi = lo + metric["seconds"]
        window = [r for r in records if lo <= r["unix_seconds"] <= hi]
        metric["profile_samples"] = len(window)
        if window:
            metric["sampled_peak_tree_rss_bytes"] = max(r["rss_bytes"] for r in window)
            for key in ("cpu_seconds", "read_bytes", "write_bytes"):
                metric["sampled_" + key] = window[-1][key] - window[0][key]
    report = {
        "command": command,
        "scans": scans,
        "exit_code": code,
        "wall_seconds": time.perf_counter() - started,
        "sample_interval_seconds": 0.01,
        "sampled_peak_tree_rss_bytes": max(
            (r["rss_bytes"] for r in records), default=0
        ),
        "last_observed_process_counters": seen,
        "workloads": metrics,
        "samples": records,
        "limits": "CPU/I/O counters can miss final activity of short-lived children; RSS is sampled, includes fixture generation, and is not per-workload. OS file cache is uncontrolled; repeats are not an application cache. Test profile is not release.",
    }
    (directory / "profile.json").write_text(json.dumps(report, indent=2))
    if code:
        raise RuntimeError(f"Workload failed ({code}); retained logs in {directory}")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--scans", type=int, nargs="+", default=[1000, 10000])
    parser.add_argument("--executable", type=Path)
    parser.add_argument("--openms", action="store_true")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    if args.executable:
        executable = args.executable.resolve()
    else:
        build = subprocess.run(
            [
                "cargo",
                "test",
                "--locked",
                "--no-default-features",
                "--features",
                "mcp-headless",
                "--test",
                "performance",
                "--no-run",
                "--message-format=json",
            ],
            capture_output=True,
            text=True,
            check=True,
        )
        (args.output / "build.log").write_text(build.stdout + build.stderr)
        artifacts = [
            json.loads(line)
            for line in build.stdout.splitlines()
            if line.startswith("{")
        ]
        executable = Path(
            next(
                item["executable"]
                for item in artifacts
                if item.get("reason") == "compiler-artifact"
                and item.get("target", {}).get("name") == "performance"
                and item.get("executable")
            )
        )
    # Windows locks a running executable. Profile an immutable copy so another
    # Cargo invocation can relink the original without invalidating this run.
    original_executable = executable
    executable = args.output.resolve() / ("measured-executable" + executable.suffix)
    shutil.copy2(original_executable, executable)
    metadata = {
        "platform": platform.platform(),
        "python": platform.python_version(),
        "logical_cpus": psutil.cpu_count(),
        "physical_cpus": psutil.cpu_count(logical=False),
        "total_ram_bytes": psutil.virtual_memory().total,
        "executable": str(executable),
        "original_executable": str(original_executable),
        "executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
        "git_head": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], text=True
        ).strip(),
        "git_status": subprocess.check_output(["git", "status", "--short"], text=True),
    }
    (args.output / "environment.json").write_text(json.dumps(metadata, indent=2))
    for scans in args.scans:
        directory = args.output / str(scans)
        directory.mkdir()
        result = profile(
            [
                str(executable),
                "analytical_workloads",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ],
            directory,
            scans,
        )
        print(
            f"scans={scans} wall={result['wall_seconds']:.3f}s sampled RSS={result['sampled_peak_tree_rss_bytes']/1024**2:.1f} MiB",
            flush=True,
        )
    if args.openms:
        directory = args.output / "openms-reference-cache-alignment"
        directory.mkdir()
        profile(
            [
                str(executable),
                "openms_workloads",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ],
            directory,
            0,
        )


if __name__ == "__main__":
    main()
