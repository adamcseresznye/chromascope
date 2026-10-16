#!/usr/bin/env python3
"""Generate THIRD_PARTY_LICENSES.txt from `cargo metadata`.

The file lists every Rust crate that can end up in a shipped binary (the normal-dependency
closure of this package, all targets and features, no dev- or build-only crates) together with
the licence text found in each crate's source, plus the bundled fonts.

    python scripts/generate_third_party_licenses.py            # rewrite the file
    python scripts/generate_third_party_licenses.py --check    # fail if the file is out of date

Run `cargo fetch --locked` first so all crate sources are present in the cargo registry.
Standard library only; no cargo-about needed.
"""
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "THIRD_PARTY_LICENSES.txt"
NL = chr(10)
CRLF = chr(13) + chr(10)
LICENSE_PREFIXES = ("LICENSE", "LICENCE", "COPYING", "UNLICENSE", "NOTICE", "COPYRIGHT")
# Files shipped by some crates that are not licence texts (e.g. LICENSE-INSTALL scripts, .rs).
SKIP_SUFFIXES = (".rs", ".py", ".sh", ".toml", ".json", ".yml", ".yaml", ".png", ".svg")
# Fonts embedded in the application, with the licence file that accompanies each.
FONTS = [
    ("Inter 4.1 (Regular, SemiBold)", "https://github.com/rsms/inter", "assets/fonts/inter/LICENSE.txt"),
    (
        "JetBrains Mono (Regular)",
        "https://github.com/JetBrains/JetBrainsMono",
        "assets/fonts/jetbrains-mono/OFL.txt",
    ),
]


def metadata():
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return json.loads(out)


def shipped_packages(meta):
    """Packages reachable from the root through normal (non-dev, non-build) dependencies."""
    by_id = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = meta["resolve"]["root"]
    seen, stack = set(), [root]
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in nodes[pid]["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    seen.discard(root)
    return sorted((by_id[i] for i in seen), key=lambda p: (p["name"], p["version"]))


def license_files(pkg):
    crate_dir = Path(pkg["manifest_path"]).parent
    files = []
    declared = pkg.get("license_file")
    if declared:
        files.append(crate_dir / declared)
    for path in sorted(crate_dir.iterdir()):
        if path.is_file() and path.name.upper().startswith(LICENSE_PREFIXES):
            if path.suffix.lower() not in SKIP_SUFFIXES and path not in files:
                files.append(path)
    # Crates that bundle fonts (egui/epaint) keep the font licences in a fonts/ directory.
    fonts = crate_dir / "fonts"
    if fonts.is_dir():
        for path in sorted(fonts.iterdir()):
            if path.suffix.lower() == ".txt" and path.stem.upper() in {"OFL", "UFL", "LICENSE"}:
                files.append(path)
    return files


def read_text(path):
    return path.read_bytes().decode("utf-8", errors="replace").replace("\r\n", "\n").strip()


def normalize(text):
    return NL.join(line.rstrip() for line in text.split(NL))


def spdx_ids(expression):
    for ch in "()/":
        expression = expression.replace(ch, " ")
    return [t for t in expression.split() if t not in ("OR", "AND", "WITH")]


def build():
    meta = metadata()
    packages = shipped_packages(meta)
    groups = {}  # licence text -> packages that ship exactly this text
    no_text = []
    for pkg in packages:
        texts = list(dict.fromkeys(t for t in (read_text(f) for f in license_files(pkg)) if t))
        if not texts:
            no_text.append(pkg)
        for text in texts:
            groups.setdefault(normalize(text), []).append(pkg)

    # Crates in a workspace often publish without the licence files. Their manifests still
    # declare an SPDX expression and authors, so attach the standard MIT / Apache-2.0 text (taken
    # from the texts the other crates ship) and name the declared authors as copyright holders.
    generic = {}
    for text in groups:
        if "Permission is hereby granted, free of charge" in text and len(text) < 2000:
            generic.setdefault("MIT", text)
        if text.startswith("Apache License") and "Version 2.0, January 2004" in text[:200]:
            generic.setdefault("Apache-2.0", text)
    declared, unresolved = [], []
    for pkg in no_text:
        ids = spdx_ids(pkg.get("license") or "")
        chosen = next((i for i in ("MIT", "Apache-2.0") if i in ids and i in generic), None)
        (declared if chosen else unresolved).append((pkg, chosen) if chosen else pkg)

    lines = [
        "THIRD-PARTY LICENCES",
        "====================",
        "",
        "Chromascope itself is licensed under GPL-3.0-only (see LICENSE). It includes the",
        f"third-party Rust crates ({len(packages)}) and fonts listed below, each under its own licence.",
        "Generated by scripts/generate_third_party_licenses.py from Cargo.lock; do not edit by hand.",
        "",
        "",
        "FONTS",
        "-----",
    ]
    for name, url, path in FONTS:
        lines += ["", f"{name} - {url}", "", read_text(ROOT / path), "", "." * 78]

    lines += ["", "", "RUST CRATES", "-----------", ""]
    lines += ["Crate, version, SPDX licence expression and repository:", ""]
    for pkg in packages:
        lines.append(
            f"  {pkg['name']} {pkg['version']} - {pkg.get('license') or pkg.get('license_file') or 'see licence text'}"
            + (f" - {pkg['repository']}" if pkg.get("repository") else "")
        )

    lines += ["", "", "LICENCE TEXTS", "-------------"]
    ordered = sorted(groups.items(), key=lambda kv: (-len(kv[1]), kv[1][0]["name"], kv[1][0]["version"]))
    for text, pkgs in ordered:
        lines += ["", "=" * 78, "Used by:"]
        lines += [f"  {p['name']} {p['version']}" for p in pkgs]
        lines += ["", text, ""]

    if declared:
        lines += ["", "=" * 78]
        lines += [
            "Crates whose published package ships no licence file. Each declares the SPDX",
            "licence shown and the authors named below; the standard text of that licence follows.",
            "",
        ]
        for spdx in ("MIT", "Apache-2.0"):
            members = [p for p, chosen in declared if chosen == spdx]
            if not members:
                continue
            lines += [f"--- {spdx} ---", ""]
            for p in members:
                authors = ", ".join(p.get("authors") or ["authors not declared in the manifest"])
                lines.append(f"  {p['name']} {p['version']} - Copyright holders: {authors}")
            lines += ["", generic[spdx], ""]
    if unresolved:
        lines += ["", "=" * 78, "Crates with no licence file under a licence other than MIT or Apache-2.0:"]
        lines += [f"  {p['name']} {p['version']} ({p.get('license')})" for p in unresolved]
        lines += [""]
    return NL.join(lines).rstrip() + NL, unresolved


def main():
    text, unresolved = build()
    if unresolved:
        print(
            f"note: {len(unresolved)} crate(s) ship no licence file and are not MIT/Apache-2.0: "
            + ", ".join(f"{p['name']} ({p.get('license')})" for p in unresolved),
            file=sys.stderr,
        )
    if "--check" in sys.argv:
        current = OUT.read_bytes().decode("utf-8").replace(CRLF, NL) if OUT.exists() else ""
        if current != text:
            print("THIRD_PARTY_LICENSES.txt is out of date; run scripts/generate_third_party_licenses.py", file=sys.stderr)
            sys.exit(1)
        print("THIRD_PARTY_LICENSES.txt is up to date")
        return
    OUT.write_text(text, encoding="utf-8", newline=NL)
    print(f"wrote {OUT} ({len(text)} bytes)")


if __name__ == "__main__":
    main()
