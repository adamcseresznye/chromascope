//! Guards for what the repository tracks and ignores: data files, personal data, package metadata.
use std::{fs, path::Path, process::Command};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &str) -> String {
    fs::read_to_string(root().join(path))
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .replace("\r\n", "\n")
}

/// `Some(true)` when Git ignores `path`, `None` when Git is unavailable.
fn ignored(path: &str) -> Option<bool> {
    let status = Command::new("git")
        .args(["check-ignore", "-q", "--no-index", path])
        .current_dir(root())
        .status()
        .ok()?;
    match status.code()? {
        0 => Some(true),
        1 => Some(false),
        _ => None,
    }
}

#[test]
fn gitignore_keeps_mass_spectrometry_data_out_of_git_at_any_depth() {
    let text = read(".gitignore");
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    // A bare ".mzML" only matches a file with exactly that name; these match every file.
    for pattern in ["*.mzML", "*.mzml.gz", "*.mzXML", "*.raw", "*.d/"] {
        assert!(lines.contains(&pattern), ".gitignore lacks {pattern}");
    }
    for bare in [".mzML", ".raw", ".mzXML"] {
        assert!(
            !lines.contains(&bare),
            "{bare} only matches a file of that exact name"
        );
    }
    let Some(_) = ignored("probe.mzML") else {
        eprintln!("git is not available: only the pattern text was checked");
        return;
    };
    for path in [
        "data/run.mzML",
        "a/b/c/sample.mzML",
        "sample.mzml",
        "a/sample.mzML.gz",
        "a/sample.mzml.gz",
        "x/sample.mzXML",
        "sample.raw",
        "run1.d/analysis.baf",
    ] {
        assert_eq!(ignored(path), Some(true), "{path} must be ignored");
    }
    // The authored fixtures stay addable, and ordinary files are untouched.
    for path in [
        "test_file/new_fixture.mzML",
        "tests/fixtures/deep/new.mzML",
        "README.md",
        "src/main.rs",
        "examples/data.mzML.md",
    ] {
        assert_eq!(ignored(path), Some(false), "{path} must not be ignored");
    }
}

/// Paths of every tracked file, or `None` when Git is unavailable.
fn tracked_files() -> Option<Vec<String>> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(
        String::from_utf8_lossy(&output.stdout)
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

#[test]
fn tracked_files_carry_no_account_name_or_local_machine_path() {
    // Built at run time so that this file does not contain what it forbids.
    let needles = [
        ["s0212", "777"].concat(),
        ["universiteit", " antwerpen"].concat(),
        ["bureau", "blad"].concat(),
    ];
    let Some(files) = tracked_files() else {
        eprintln!("git is not available: tracked files were not scanned");
        return;
    };
    assert!(files.len() > 100, "unexpectedly few tracked files");
    let binary = [
        "png", "mp4", "ico", "icns", "gif", "ttf", "otf", "woff", "woff2",
    ];
    let mut offenders = Vec::new();
    for file in &files {
        let extension = Path::new(file)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if binary.contains(&extension.as_str()) {
            continue;
        }
        let Ok(bytes) = fs::read(root().join(file)) else {
            continue; // listed but deleted in the working tree
        };
        let text = String::from_utf8_lossy(&bytes).to_lowercase();
        for needle in &needles {
            if text.contains(needle.as_str()) {
                offenders.push(format!("{file}: {needle}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "personal data in tracked files:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn indexed_mzml_fixture_was_anonymised_without_moving_a_single_byte() {
    // test_file/data_dependent_02.mzML is an indexed mzML: a <sourceFile location> of a different
    // length would shift every byte offset in its index. The original path was replaced by a
    // string of exactly the same length, so the file keeps the size it had before.
    let bytes = fs::read(root().join("test_file/data_dependent_02.mzML")).unwrap();
    assert_eq!(bytes.len(), 517_498, "same-length edit only");
    let text = String::from_utf8_lossy(&bytes);
    let location = text
        .split("<sourceFile ")
        .nth(1)
        .and_then(|s| s.split("location=\"").nth(1))
        .and_then(|s| s.split('"').next())
        .expect("sourceFile location");
    assert!(location.starts_with("file:///C:"));
    assert_eq!(location.len(), 70, "length of the original location");
    assert!(location.contains("anonymised"));
}

#[test]
fn package_metadata_feeds_the_deb_and_rpm_packages() {
    // cargo-deb and cargo-generate-rpm take the package synopsis, URL and licence from
    // [package]; without them the packages ship with an empty description.
    let manifest: toml::Value = read("Cargo.toml").parse().unwrap();
    let package = &manifest["package"];
    let text = |key: &str| {
        package[key]
            .as_str()
            .unwrap_or_else(|| panic!("[package] {key} missing"))
            .to_owned()
    };
    let description = text("description");
    assert!(
        (40..=200).contains(&description.len()),
        "a one-line description: {description:?}"
    );
    assert!(!description.ends_with('.') && !description.contains('\n'));
    assert_eq!(
        text("repository"),
        "https://github.com/chromascope-ms/chromascope"
    );
    assert_eq!(
        text("homepage"),
        "https://chromascope-ms.github.io/chromascope/"
    );
    assert_eq!(text("license"), "GPL-3.0-only");
    assert_eq!(text("readme"), "README.md");
    assert!(root().join("README.md").is_file());
    let keywords = package["keywords"].as_array().unwrap();
    assert!(
        (1..=5).contains(&keywords.len()),
        "crates.io allows at most five keywords"
    );
    assert!(!package["categories"].as_array().unwrap().is_empty());
    // The Debian package adds its own long description on top of the synopsis.
    assert!(
        manifest["package"]["metadata"]["deb"]["extended-description"]
            .as_str()
            .is_some_and(|d| d.len() > description.len())
    );
}

fn gui_sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<(String, String)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
                out.push((path.display().to_string(), text));
            }
        }
    }
    let mut out = Vec::new();
    walk(&root().join("src/gui"), &mut out);
    out.push(("src/gui/mod.rs".into(), read("src/gui/mod.rs")));
    out
}

#[test]
fn the_audit_package_has_one_name_in_the_tab_the_window_and_the_workflow() {
    let sources = gui_sources();
    let all: String = sources.iter().map(|(_, text)| text.as_str()).collect();
    for old in [
        "Audit package (optional)",
        "Report and audit package",
        "\"Reports\"",
    ] {
        assert!(
            !all.contains(old),
            "{old:?} is no longer a name of this tab"
        );
    }
    // The literal exists once, as the constant every place uses.
    assert_eq!(all.matches("\"Audit package\"").count(), 1);
    let workflow = read("src/gui/workflow.rs");
    assert!(workflow.contains("const AUDIT_PACKAGE: &str = \"Audit package\";"));
    for user in [
        "src/gui/delivery.rs",
        "src/gui/workbench.rs",
        "src/gui/panels.rs",
    ] {
        assert!(
            read(user).contains("AUDIT_PACKAGE"),
            "{user} must use the constant"
        );
    }
    // Being optional is a status of the workflow step, not part of the name.
    assert!(workflow.contains("Optional. Export a readable report"));
}

#[test]
fn every_csv_text_cell_goes_through_the_formula_safe_helper() {
    // Ad-hoc quoting (`s.replace('"', "\"\"")`) cannot neutralise a leading "=" or "@", so the
    // only place allowed to double quotes for CSV is the helper itself.
    fn walk(dir: &Path, hits: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, hits);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = fs::read_to_string(&path).unwrap();
                if text.contains("replace('\"', \"\\\"\\\"\")") && !path.ends_with("csv_text.rs") {
                    hits.push(path.display().to_string());
                }
            }
        }
    }
    let mut hits = Vec::new();
    walk(&root().join("src"), &mut hits);
    assert!(
        hits.is_empty(),
        "quote CSV cells with csv_text::cell instead: {hits:?}"
    );
    assert!(read("src/csv_text.rs").contains("pub fn cell("));
}

#[test]
fn window_titles_use_sentence_case() {
    // Title Case is only for names: the app, a product or an acronym.
    const NAMES: [&str; 3] = ["Chromascope", "LC-HRMS", "MS"];
    let mut bad = Vec::new();
    for (file, text) in gui_sources() {
        for part in text.split("Window::new(\"").skip(1) {
            let title = part.split('"').next().unwrap();
            let shouted = title
                .split_whitespace()
                .skip(1)
                .filter(|word| word.chars().next().is_some_and(char::is_uppercase))
                .filter(|word| {
                    !NAMES.contains(&word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-'))
                })
                .count();
            if shouted > 0 {
                bad.push(format!("{file}: {title:?}"));
            }
        }
    }
    assert!(bad.is_empty(), "use sentence case: {bad:?}");
    let all: String = gui_sources().into_iter().map(|(_, text)| text).collect();
    assert!(all.contains("Window::new(\"Plot properties\")"));
    assert!(all.contains("Window::new(\"m/z range filter\")"));
}

/// Text of a tracked file that is not binary, or `None` for binary and unreadable files.
fn tracked_text(file: &str) -> Option<String> {
    let bytes = fs::read(root().join(file)).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).replace("\r\n", "\n"))
}

/// `text` without the web addresses in it: a web address is not a path in this repository.
fn without_urls(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = ["http://", "https://"]
        .iter()
        .filter_map(|scheme| rest.find(scheme))
        .min()
    {
        out.push_str(&rest[..at]);
        let end = rest[at..]
            .find(|c: char| c.is_whitespace() || "()<>\"'`[]".contains(c))
            .map_or(rest.len(), |n| at + n);
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

#[test]
fn no_tracked_file_refers_to_the_docs_folder() {
    // The folder is kept on the maintainer's disk only (it is ignored), so nothing a clone contains
    // may point into it. Built at run time so that this file does not contain what it forbids.
    let needle = ["do", "cs/"].concat();
    let Some(files) = tracked_files() else {
        eprintln!("git is not available: tracked files were not scanned");
        return;
    };
    assert!(files.len() > 100, "unexpectedly few tracked files");
    let mut offenders = Vec::new();
    for file in files.iter().filter(|f| f.as_str() != ".gitignore") {
        assert!(
            !file.starts_with(&needle),
            "{file} is tracked: the whole folder must stay out of the repository"
        );
        let Some(text) = tracked_text(file) else {
            continue;
        };
        for (number, line) in text.lines().enumerate() {
            if without_urls(line).contains(&needle) {
                offenders.push(format!("{file}:{}: {}", number + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "references to the untracked folder:\n{}",
        offenders.join("\n")
    );
    // Every ignore rule for the folder is the single one line, anchored to the root so that the
    // website folder (website/content/...) is not caught by it.
    let ignore = read(".gitignore");
    let rules: Vec<&str> = ignore.lines().filter(|l| l.contains("docs")).collect();
    let anchored = ["/", needle.as_str()].concat();
    assert_eq!(rules, [anchored.as_str()], "one rule keeps the folder out");
    if ignored("probe.mzML").is_some() {
        let root_note = ["do", "cs/RELEASING.md"].concat();
        let nested = ["website/content/do", "cs/page.md"].concat();
        assert_eq!(
            ignored(&root_note),
            Some(true),
            "{root_note} stays untracked"
        );
        for tracked in [
            "website/mkdocs.yml",
            "website/content/index.md",
            nested.as_str(),
        ] {
            assert_eq!(ignored(tracked), Some(false), "{tracked} must be trackable");
        }
    }
}

#[test]
fn no_tracked_file_points_at_the_old_repository_address() {
    // The project lives in the chromascope-ms organization. Built at run time so that this file
    // does not contain what it forbids. The author's name is not an address and stays.
    let account = ["adam", "cseresznye"].concat();
    let needles = [
        ["github.com/", account.as_str()].concat(),
        [account.as_str(), ".github.io"].concat(),
        ["io.github.", account.as_str()].concat(),
    ];
    let Some(files) = tracked_files() else {
        eprintln!("git is not available: tracked files were not scanned");
        return;
    };
    assert!(files.len() > 100, "unexpectedly few tracked files");
    let mut offenders = Vec::new();
    for file in &files {
        let Some(text) = tracked_text(file) else {
            continue;
        };
        for (number, line) in text.lines().enumerate() {
            if needles.iter().any(|n| line.to_lowercase().contains(n)) {
                offenders.push(format!("{file}:{}: {}", number + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "the old repository address is still used:\n{}",
        offenders.join("\n")
    );
}

/// The anchor GitHub gives a heading: lowercase, punctuation dropped, spaces turned into hyphens.
fn heading_anchor(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

/// Anchors of the headings of a Markdown text, with GitHub's `-1`, `-2` suffix for repeats.
fn anchors(markdown: &str) -> Vec<String> {
    let mut seen: std::collections::HashMap<String, usize> = Default::default();
    let mut out = Vec::new();
    let mut fenced = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        } else if !fenced && line.starts_with('#') {
            let base = heading_anchor(line.trim_start_matches('#'));
            let n = seen.entry(base.clone()).or_default();
            out.push(if *n == 0 { base } else { format!("{base}-{n}") });
            *n += 1;
        }
    }
    out
}

/// Targets of the Markdown links `[text](target)` and images in `markdown`, outside code fences.
fn link_targets(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut fenced = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let mut rest = line;
        while let Some(at) = rest.find("](") {
            rest = &rest[at + 2..];
            if let Some(end) = rest.find(')') {
                out.push(rest[..end].trim().to_owned());
                rest = &rest[end..];
            }
        }
    }
    out
}

/// `base` joined with a relative `target`, with `.` and `..` resolved; `None` if it leaves the repository.
fn resolve(base: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = base.split('/').collect();
    parts.pop(); // the file name
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

#[test]
fn every_relative_markdown_link_points_to_a_tracked_file() {
    let Some(files) = tracked_files() else {
        eprintln!("git is not available: links were not checked");
        return;
    };
    let markdown: Vec<&String> = files.iter().filter(|f| f.ends_with(".md")).collect();
    assert!(markdown.len() >= 5, "unexpectedly few Markdown files");
    let mut checked = 0;
    let mut broken = Vec::new();
    for file in markdown {
        let text = tracked_text(file).unwrap();
        for target in link_targets(&text) {
            if target.contains("://") || target.starts_with("mailto:") {
                continue;
            }
            let (path, anchor) = match target.split_once('#') {
                Some((p, a)) => (p, Some(a)),
                None => (target.as_str(), None),
            };
            let resolved = if path.is_empty() {
                Some(file.clone())
            } else {
                resolve(file, path.split('?').next().unwrap())
            };
            let Some(resolved) = resolved.filter(|r| files.contains(r)) else {
                broken.push(format!("{file}: ({target}) is not a tracked file"));
                continue;
            };
            checked += 1;
            if let Some(anchor) = anchor.filter(|_| resolved.ends_with(".md")) {
                let headings = anchors(&tracked_text(&resolved).unwrap());
                if !headings.iter().any(|h| h == anchor) {
                    broken.push(format!("{file}: ({target}) has no heading {anchor:?}"));
                }
            }
        }
    }
    assert!(
        checked >= 10,
        "too few relative links were found: {checked}"
    );
    assert!(broken.is_empty(), "broken links:\n{}", broken.join("\n"));
}

#[test]
fn the_link_checker_notices_what_it_should() {
    // The two guards above are only worth something if their helpers see what they claim to.
    let d = ["do", "cs/"].concat();
    assert_eq!(
        without_urls(&format!(
            "see https://a.example/{d}x and (http://b.example/{d}y) {d}z"
        )),
        format!("see  and () {d}z")
    );
    assert_eq!(
        link_targets("[a](x.md) ![b](img/p.png#c)\n```\n[no](ignored.md)\n```\n[c](../y.md#h)"),
        ["x.md", "img/p.png#c", "../y.md#h"]
    );
    assert_eq!(resolve("a/b/c.md", "../d.md").as_deref(), Some("a/d.md"));
    assert_eq!(resolve("c.md", "../d.md"), None);
    assert_eq!(
        anchors("# A title\n## Sub: part\n```\n# not a heading\n```\n## Sub: part"),
        ["a-title", "sub-part", "sub-part-1"]
    );
}

/// SHA-1 (FIPS 180-4), because the mzML `fileChecksum` is a SHA-1 and the project has no other
/// use for it.
fn sha1_hex(data: &[u8]) -> String {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in message.chunks(64) {
        let mut w = [0u32; 80];
        for (i, word) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }
    h.iter().map(|word| format!("{word:08x}")).collect()
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|at| at + from)
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let start = tag.find(&format!(" {name}=\""))? + name.len() + 3;
    tag[start..].split('"').next()
}

#[test]
fn the_mzml_fixture_index_points_at_its_elements_and_the_checksum_is_right() {
    assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    let bytes = fs::read(root().join("test_file/data_dependent_02.mzML")).unwrap();
    let text = String::from_utf8(bytes.clone()).expect("the fixture is UTF-8");
    let index_list = find(&bytes, b"<indexList ", 0).expect("<indexList>");
    // indexListOffset is the position of <indexList>.
    let stored = text
        .split("<indexListOffset>")
        .nth(1)
        .and_then(|s| s.split('<').next())
        .expect("indexListOffset");
    assert_eq!(
        stored.parse::<usize>().unwrap(),
        index_list,
        "indexListOffset"
    );
    // Every <offset> points at the start of the element of that id, and every element is indexed.
    let mut indexed = 0;
    for index in text[index_list..].split("<index name=\"").skip(1) {
        let (kind, body) = index.split_once("\">").unwrap();
        let (kind, tag) = match kind {
            "spectrum" => ("spectrum", "<spectrum "),
            "chromatogram" => ("chromatogram", "<chromatogram "),
            other => panic!("unknown index {other}"),
        };
        let body = body.split("</index>").next().unwrap();
        for entry in body.split("<offset idRef=\"").skip(1) {
            let (id, rest) = entry.split_once("\">").unwrap();
            let offset: usize = rest.split('<').next().unwrap().trim().parse().unwrap();
            assert!(
                bytes[offset..].starts_with(tag.as_bytes()),
                "{kind} {id}: offset {offset} does not point at {tag}"
            );
            let element = &text[offset..offset + text[offset..].find('>').unwrap()];
            assert_eq!(
                attribute(element, "id"),
                Some(id),
                "{kind} {id}: wrong element at {offset}"
            );
            indexed += 1;
        }
    }
    let elements = text[..index_list].matches("<spectrum ").count()
        + text[..index_list].matches("<chromatogram ").count();
    assert_eq!(
        indexed, elements,
        "every spectrum and chromatogram is indexed exactly once"
    );
    assert!(indexed >= 54);
    // fileChecksum: SHA-1 of every byte up to and including the <fileChecksum> start tag.
    let marker = b"<fileChecksum>";
    let start = find(&bytes, marker, index_list).unwrap() + marker.len();
    let stored = &text[start..start + 40];
    assert_eq!(sha1_hex(&bytes[..start]), stored, "fileChecksum");
    assert!(text[start + 40..].starts_with("</fileChecksum>"));
}
