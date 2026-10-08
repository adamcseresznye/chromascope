#!/usr/bin/env python3
"""Exercise a real local MCP visual/numerical loop; no LLM dependencies.

Demonstration boundaries are selected from acquired RT points. They are not
scientific peak-quality decisions. A vision client should supply reviewed bounds.
"""
import argparse
import base64
import json
from pathlib import Path
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", required=True, type=Path)
    parser.add_argument("--file", required=True, type=Path)
    parser.add_argument("--mass", required=True, type=float)
    parser.add_argument("--ppm", default=10.0, type=float)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    source = args.file.resolve(strict=True)
    args.output.mkdir(parents=True, exist_ok=False)
    output = args.output.resolve()
    process = subprocess.Popen(
        [str(args.server.resolve(strict=True)), "--allow-root", str(source.parent),
         "--allow-root", str(output), "--allow-changes", "--allow-exports"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=sys.stderr,
        text=True, encoding="utf-8", bufsize=1,
    )
    counter = 0
    transcript = []

    def rpc(method, params, notification=False):
        nonlocal counter
        request = {"jsonrpc": "2.0", "method": method, "params": params}
        if not notification:
            counter += 1
            request["id"] = counter
        process.stdin.write(json.dumps(request, allow_nan=False) + "\n")
        process.stdin.flush()
        if notification:
            return None
        while True:
            line = process.stdout.readline()
            if not line:
                raise RuntimeError("MCP server disconnected; inspect stderr")
            response = json.loads(line)
            if response.get("id") == counter:
                if "error" in response:
                    raise RuntimeError(response["error"])
                return response["result"]

    def tool(name, **arguments):
        result = rpc("tools/call", {"name": name, "arguments": arguments})
        if result.get("isError"):
            raise RuntimeError(result)
        # Store structured numerical/provenance data, keeping base64 images separate.
        transcript.append({"tool": name, "arguments": arguments,
                           "result": result.get("structuredContent")})
        return result

    def image(name, filename):
        result = tool(name)
        content = next(c for c in result["content"] if c["type"] == "image")
        if content["mimeType"] != "image/png":
            raise RuntimeError("Expected PNG image")
        (output / filename).write_bytes(base64.b64decode(content["data"], validate=True))

    try:
        rpc("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                           "clientInfo": {"name": "chromascope-visual-example", "version": "1"}})
        rpc("notifications/initialized", {}, notification=True)
        tools = rpc("tools/list", {})
        print(f"Discovered {len(tools['tools'])} MCP tools")
        discovered = tool("list_mzml_files", directory=str(source.parent))["structuredContent"]["files"]
        if not any(Path(path).samefile(source) for path in discovered):
            raise RuntimeError("Input file was not discovered under the authorized root")
        loaded = tool("open_files", paths=[str(source)])["structuredContent"]["files"][0]
        if not loaded["success"]:
            raise RuntimeError(loaded)
        dataset = loaded["dataset_id"]
        tool("dataset_metadata", dataset_id=dataset)
        extracted = tool("extract_chromatogram", dataset_id=dataset, kind="xic",
                         polarity="positive", ms_level=1, smoothing=0,
                         mass=args.mass, tolerance_ppm=args.ppm,
                         display=True)["structuredContent"]
        if extracted.get("display_conflict") or not extracted["displayed"]:
            raise RuntimeError("Local GUI changed during extraction; retry display")
        points = tool("chromatogram_data", dataset_id=dataset, offset=0,
                      limit=10000)["structuredContent"]["points"]
        if len(points) < 6:
            raise RuntimeError("Choose a target with at least six measured RT points")
        image("chromatogram_image", "01-xic.png")
        lower, upper = points[1][0], points[-2][0]
        tool("set_view", retention_time_range=[lower, upper])
        initial = tool("integrate", dataset_id=dataset, start_minutes=lower,
                       end_minutes=upper, apply=True)["structuredContent"]
        image("chromatogram_image", "02-integration.png")
        # Demonstrate interpolation with a narrower interval; this is an API
        # exercise, not a scientifically recommended peak correction.
        apex = max(range(1, len(points) - 1), key=lambda i: points[i][1])
        revised_start = (points[apex - 1][0] + points[apex][0]) / 2
        revised_end = (points[apex][0] + points[apex + 1][0]) / 2
        corrected = tool("integrate", dataset_id=dataset, start_minutes=revised_start,
                         end_minutes=revised_end, apply=True)["structuredContent"]
        image("chromatogram_image", "03-modified-boundaries.png")
        tool("spectrum", dataset_id=dataset, retention_time_minutes=points[2][0],
             display=True, offset=0, limit=10000)
        image("spectrum_image", "04-spectrum.png")
        tool("export_csv", dataset_id=dataset, spectrum=False,
             path=str(output / "xic.csv"))
        tool("export_figure", spectrum=False, path=str(output / "chromatogram.svg"))
        (output / "workflow.json").write_text(json.dumps(transcript, indent=2,
                                                       allow_nan=False), encoding="utf-8")
        print(f"Area before: {initial['area']}; after: {corrected['area']} intensity*minutes")
        print(f"Images, numerical data and reproducible requests saved to {output}")
    finally:
        process.stdin.close()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.terminate()
            process.wait(timeout=5)


if __name__ == "__main__":
    main()
