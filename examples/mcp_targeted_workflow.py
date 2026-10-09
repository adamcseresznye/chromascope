"""Run a real headless MCP targeted workflow into a new reproducible draft bundle.

Usage: python examples/mcp_targeted_workflow.py request.json NEW_OUTPUT
       --server target/debug/chromascope-engine-mcp.exe [--qc-rules rules.json]
The request must contain targeted_batch with actual existing source paths.
No automatic approvals, thresholds, exclusions or numerical algorithms are supplied.
"""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("request", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--server", required=True, type=Path)
    parser.add_argument("--qc-rules", type=Path)
    args = parser.parse_args()
    request = json.loads(args.request.read_text(encoding="utf-8-sig"))
    if request["operation"]["operation"] != "targeted_batch":
        raise ValueError("Requires a version-1 targeted_batch engine request")
    sources = [
        Path(sample["source"]).resolve(strict=True)
        for sample in request["operation"]["batch"]["samples"]
    ]
    roots = sorted({str(source.parent) for source in sources})
    rules = (
        json.loads(args.qc_rules.read_text(encoding="utf-8-sig"))
        if args.qc_rules
        else None
    )
    server = args.server.resolve(strict=True)
    args.output.mkdir(exist_ok=False)
    transcript = []
    jobs = []
    counter = 0
    command = [str(server)]
    for root in roots:
        command.extend(["--allow-root", root])

    def save(name, value):
        with (args.output / name).open("x", encoding="utf-8") as stream:
            json.dump(value, stream, indent=2, allow_nan=False)

    save("input-request.json", request)
    save(
        "inputs.json",
        {
            str(path.resolve()): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in [args.request] + ([args.qc_rules] if args.qc_rules else [])
        },
    )
    with (args.output / "stderr.txt").open("x", encoding="utf-8") as log:
        process = subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=log,
            text=True,
            encoding="utf-8",
            bufsize=1,
        )

        def rpc(method, params, notification=False):
            nonlocal counter
            counter += 1
            message = {"jsonrpc": "2.0", "method": method, "params": params}
            if not notification:
                message["id"] = counter
            process.stdin.write(json.dumps(message, allow_nan=False) + "\n")
            process.stdin.flush()
            if notification:
                return None
            while True:
                line = process.stdout.readline()
                if not line:
                    raise RuntimeError("MCP disconnected; inspect retained stderr")
                response = json.loads(line)
                if response.get("id") != counter:
                    continue
                transcript.append({"request": message, "response": response})
                if "error" in response:
                    raise RuntimeError(response["error"])
                result = response["result"]
                if result.get("isError"):
                    raise RuntimeError(result)
                return result.get("structuredContent", result)

        def tool(name, **arguments):
            return rpc("tools/call", {"name": name, "arguments": arguments})

        def analyze(operation):
            envelope = {
                "version": 1,
                "operation_id": str(uuid.uuid4()),
                "actor": "MCP targeted workflow example (AI client)",
                "operation": operation,
            }
            job = tool("start_analysis", path=str(sources[0]), request=envelope)[
                "job_id"
            ]
            jobs.append(job)
            deadline = time.monotonic() + 600
            while time.monotonic() < deadline:
                state = tool("analysis_status", job_id=job)
                if state["state"] in ("failed", "cancelled"):
                    raise RuntimeError(state)
                if state["state"] == "succeeded":
                    bundle = tool(
                        "prepare_analysis_report", job_ids=[job], explanation=""
                    )
                    response = bundle["bundle"]["artifacts"][0]["response"]
                    save(f"response-{len(jobs)}.json", response)
                    return response["output"]
                time.sleep(0.2)
            tool("cancel_analysis", job_id=job)
            raise TimeoutError("Analysis exceeded 600 seconds; cancellation requested")

        try:
            rpc(
                "initialize",
                {
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": {"name": "targeted-example", "version": "1"},
                },
            )
            rpc("notifications/initialized", {}, notification=True)
            save("capabilities.json", tool("analysis_capabilities"))
            batch = analyze(request["operation"])["batch"]
            if rules is not None:
                analyze(
                    {
                        "operation": "evaluate_targeted_qc",
                        "batch": batch,
                        "rules": rules,
                    }
                )
            tables = analyze({"operation": "export_targeted", "batch": batch})
            for key in ("csv", "calibration_csv"):
                with (args.output / f"{key}.csv").open("x", encoding="utf-8") as stream:
                    stream.write(tables[key])
            save(
                "report.json",
                tool(
                    "prepare_analysis_report",
                    job_ids=jobs,
                    explanation="Draft prepared by example client; no scientific explanation or approval is asserted.",
                ),
            )
        finally:
            save("transcript.json", transcript)
            process.stdin.close()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=10)
            process.stdout.close()
    print(f"Review draft saved in {args.output.resolve()}")


if __name__ == "__main__":
    main()
