# Performance, reliability and scientific verification

Local engineering pass: 2026-10-08, America/Los_Angeles. Baseline Git HEAD
`ff7975aedfdcd894e31dd1e85dc3707d2cdd01f5`; checkout already contained extensive
uncommitted development. Existing work, removed assets, raw inputs and scientific
references were preserved. No staging, commit, push, publication or user-data deletion.

## Reproduction and retained evidence

Use the existing locked Rust and scientific runtime; nothing is auto-installed.

```powershell
python tests/reference/profile_performance.py NEW_OUTPUT --scans 1000 10000 50000 --openms
python tests/reference/verify_references.py NEW_REFERENCE_OUTPUT
cargo test --locked --no-default-features --features mcp-headless --test reliability
cargo test --locked --no-default-features --features mcp-headless --test untargeted openms_multisample_reference_positive_negative_cli_cache_and_project -- --ignored --nocapture
python tests/reference/test_openms_adapter.py
```

Both Python tools refuse an existing output directory. The profiler builds the
actual Rust integration-test executable, retains build/stdout/stderr/profile logs,
records executable SHA256, Git HEAD/dirty state, platform/RAM/CPU count, and samples
the process tree every nominal 10 ms. JSON contains workload wall times and, when
enough samples exist, CPU time, RSS and I/O counter deltas. Sub-10ms work can have
no samples; that means unmeasured, not zero. The benchmark uses real parsing,
scientific kernels, calibration, library import/search and project commits.

Compact measurements and original workload output are retained in
[performance/2026-10-08](performance/2026-10-08/environment.json). Full time-series
samples and build/check logs remain in `target/performance-*`. The retained test
executable hash identifies the measured build; later kernel-label/panic-guard
changes do not alter the numerical workloads. See the validation report for final
checks. The profiler now runs an immutable executable copy to avoid Windows linker locks.

## Environment and measurement boundaries

Windows 11 10.0.22621, 8 physical/16 logical CPUs, 16.89 GB installed RAM;
Rust/Cargo 1.99.0, Python 3.13.7, psutil 7.0.0, pyOpenMS 3.5.0. Scientific references
use the existing NumPy 2.3.3, SciPy 1.16.2 and mpmath 1.3.0 environment.
Rust test profile: application core unoptimized, dependency opt-level=2.
These are not release throughput numbers. Source fixtures/projects live in local
temporary directories; this is not a OneDrive transactional durability study.

Synthetic mzML inputs are uncompressed, MS1 positive centroid acquisitions with
1,024 sorted peaks/scan, explicit minute RT, scan windows, and absent TIC/BPC CV
metadata. Values are exact authored integers. Increasing scan counts exercises
streaming and source traversal, but not increasing peak-array size or instrument
complexity. The profiler includes generation, teardown and library work in its
overall RSS; per-workload sampled RSS is also available. OS cache is uncontrolled.
Repeated XICs are reader/OS reuse, not an application result cache. Windows I/O
counters report transfer activity, not physical disk reads.

## Observed workload times

Measurements before the final raw-unit/full-bounds/gzip hardening (seconds; individual observations, no p95 claim). Final measurements are recorded below after rebuilding; this table remains evidence of the earlier pass:

| Workload | 1,000 scans, 23.14 MB | 10,000 scans, 231.44 MB | 50,000 scans, 1,157.31 MB |
|---|---:|---:|---:|
| Inspect bounds | 0.057 | 0.527 | 3.179 |
| XIC first extraction | 0.067 | 0.649 | 3.837 |
| XIC second extraction | 0.066 | 0.643 | 3.881 |
| TIC, absent metadata | 0.104 | 1.031 | 6.634 |
| BPC, absent metadata | 0.120 | 1.139 | 10.137 |
| Concurrent two-reader open + XIC | 0.141 | 2.267 | 10.090 |
| Running XIC cancellation after first processed scan | 0.000168 | 0.000839 | 0.002143 |
| Whole-workload sampled peak RSS, MiB | 77.36 | 78.28 | 82.48 |

The 50,000-scan run overlapped Rust compilation/linking and had substantially
slower unrelated small kernels too. It is retained as a contention observation,
not an isolated throughput comparison. The cancellation timer starts at the cancel
request after progress becomes nonzero; it does not measure source indexing startup,
native adapter shutdown, huge-spectrum decode or all cancellation cases.

10k-run fixed workloads: 1,000 integrations of a 10,001-point trace, 0.408 s;
SG/chord chromatographic processing of 10,001 points, 0.440 s; 1,000 weighted
100-standard calibration fits, 0.434 s; 64-sample raw targeted batch including
calibration, 0.203 s; import 10,000-entry synthetic MSP library, 0.286 s; search
that library, 0.387 s; 100 synced metadata-only project revisions, 1.508 s;
open that project, 0.009 s; restore as a new revision, 0.019 s. Project tests here
do not measure large result-artifact histories; spectral precursor indexing selects
one matching entry, so this is not a worst-case all-candidate similarity benchmark.

The six-sample OpenMS workflow (820 MS1 scans plus MS/MS per sample) measured
2.276 s cold and 1.451 s with verified sample checkpoints reused. Full feature and
alignment JSON was identical between runs. This includes startup, feature detection,
isotope/adduct grouping, affine alignment, correspondence, gap filling, evidence
and verification; it does not isolate native C++ phase costs.

## Measured bottlenecks and implemented changes

Bounds inspection previously decoded all arrays while reading only metadata.
Direct same-file reader measurements at 10k scans were Full 0.419 s, Lazy 0.368 s,
MetadataOnly 0.351 s. The metadata-only traversal was about 16% faster in that
observation; bounds inspection was 0.597 s at baseline and 0.527 s in the final
10k measurement. An intermediate run was 0.900 s: uncontrolled runs are noisy and
these observations do not establish a statistically stable speedup. The new metadata
pass also computes real RT extrema and verifies observed scan count.

TIC/BPC now share one extraction implementation. Missing/invalid metadata gets
raw-array fallback per scan, supplied valid values are retained, and the prior
all-zero retry remains. Fallback visits missing rows in acquisition order using
a linear cursor rather than hashing every scan. All decoded arrays are validated
before sum/max/range indexing; invalid data returns an error. Baseline unsafe
10k fallback times were TIC 0.823 s and BPC 0.900 s; final validated fallback is
slower (1.031/1.139 s). This cost is reported, not hidden or treated as a speedup.
Valid-data summation order, inclusive mass range and base-peak tie behavior remain.

At 10k, sampled XIC CPU time was about 0.61â€“0.63 s of 0.64â€“0.65 s wall time,
consistent with CPU-heavy parsing/validation. Library search reached about 78 MiB
RSS; its retained library/verification copies are a distinct memory bottleneck.
Project commit CPU was about 0.03 s over 1.51 s wall, consistent with filesystem
flush/transaction latency. Checkpoint reuse saves OpenMS compute while still
revalidating inputs and recomputing alignment/evidence. Full counters accompany
the observations; no allocator/call-stack attribution or physical-disk claim.

Reliability fixes also cover atomic concurrent scan admission, actual-byte limits
during source hashing and unwind-enabled release worker failure reporting. Extraction
outputs are labeled `extraction-v2`: old raw-output revisions remain intact and
version-mismatched delivery replay is refused. Integration and other independent
scientific kernel labels remain unchanged.

## Scientific evidence and interface equivalence

New exact references: every XIC value is 151; TIC is 524,800; BPC is 1,024;
source indices are the original scan sequence. The authored triangle integrates
to 10 intensity*minutes within 1e-9, and 100-point weighted linear fits satisfy
`response(50)=103` within 1e-9. Synthetic 64-sample targeted results are verified
by the existing retained-evidence/calibration validator. No expected number is
injected into production output.

Independent references for calibration (NumPy least-squares), chromatography
(SciPy SG/sparse AsLS), QC, spectra/formulas (dense dot product/Decimal with retained
MassBank records) and statistics (60-digit mpmath) were regenerated separately
and matched the retained numeric JSON exactly. Only the chromatography generator's
file-location provenance was excluded from the regeneration comparison. Reference
inputs/generators/digests are retained; production modules are not used to generate
these expected values.

The opt-in positive/negative OpenMS scientific test passed: eight expected envelopes,
known absent/false-gap states, blank/QC flags, independent Gaussian integration
(relative error <0.002 over returned hull bounds), affine RT evidence, cached replay,
corruption/change rejection, CLI and MCP/project paths. Five Python OpenMS chemistry,
interval and false-gap reference tests passed separately. These are synthetic/reference
comparisons, not instrument study certification.

Existing engine/CLI/MCP numerical tests and GUI kernel/render tests remain intact.
New repaired-metadata tests compare GUI compatibility processing, engine and actual
CLI results exactly and verify retained request parameters. Actual stdio MCP tests
exercise the same engine, permissions, cancellation, paging, calibration/QC, history
and reports in the full suites. This verifies supported fixtures and explicit
parameters, not universal UI-default or native-interaction equivalence.

## Reliability acceptance and open gates

New tests exercise malformed/mismatched arrays, non-XML files, missing scan metadata,
unsorted RT, cancelled jobs, exact byte/scan limits, concurrent admission and eight
concurrent project writers. A partial temporary save plus abandoned lock preserves
original committed bytes; explicit restore appends history. Previous unit/integration
tests continue covering invalid domain units, missing evidence, adapter failures,
source changes, stale revisions, cache corruption and MCP policies.

The raw-unit probe confirmed an upstream boundary gap, now guarded by streaming
preflight: unknown/missing RT units and malformed/absent RT are rejected before
mzdata discards source evidence. Minute/second/millisecond conversion, XML/count
completeness and gzip are tested. Missing/partial scan windows use complete peak
bounds. This is not full mzML schema/CV validation or a certified XML-token/decompressed
memory bound; decompression/token limits and additional malformed cases remain open. No complete malformed-file, native UI, platform, regulatory,
instrument, OOM or power-loss validation is claimed. Large target counts, worst-case
spectral matching, persistent jobs, bounded-RSS histories, release benchmarks and
cloud-sync recovery remain open. The [defects register](REMAINING_DEFECTS.md) gives
priorities, evidence and acceptance steps. P9 is in progress, not release-validated.

## Post-hardening measurements and source-boundary corrections

The second retained run includes streaming raw-unit/XML preflight, complete mass
bounds when windows are absent/partial, and gzip support. It precedes the final
legal-empty-scan correction (the large fixture contains only nonempty scans).
Evidence: `performance/2026-10-08/post-hardening`; executable SHA256 and environment
are retained separately from earlier runs. The final executable recheck is recorded
below. Measurements remain test-profile, uncontrolled-background observations.

| Workload, seconds except RSS | 1,000 scans | 10,000 scans | 50,000 scans |
|---|---:|---:|---:|
| Open and validate bounds | 0.0628 | 0.8299 | 4.5613 |
| First XIC | 0.0798 | 0.7895 | 4.9153 |
| Second XIC | 0.0801 | 1.0596 | 5.1622 |
| TIC missing metadata | 0.1252 | 1.4771 | 26.5717 |
| BPC missing metadata | 0.1348 | 1.7513 | 22.3296 |
| Two concurrent readers | 0.1677 | 2.4175 | 12.7009 |
| Running XIC cancellation | 0.000349 | 0.001053 | 0.002821 |
| Sampled peak tree RSS, MiB | 77.43 | 78.66 | 83.03 |

At 10k scans: 1,000 integrations 0.7275 s; SG/chord 0.6245 s; 1,000 weighted fits
0.5935 s; 64-sample batch 0.3174 s; 10k-entry import/search 0.4543/0.6206 s;
100 synced project commits 2.0585 s; project open/restore 0.0130/0.0151 s.
OpenMS six-sample cold/checkpoint reuse measured 5.7123/6.7072 s, with identical
feature/alignment results. Cache reuse was slower in this contended run; the earlier
2.276/1.451 s observation does not establish a repeatable cache latency guarantee.
The 50k fallback times and unrelated kernels also deteriorated under compilation
and I/O contention. These are reproducible workloads, not reproducible hardware
latency or statistically controlled performance estimates.

The raw-source probe now returns bounds 60–714 m/z, 0–6.825 minutes for the
independently generated OpenMS acquisition and rejects the unsupported-time-unit
copy with structured `adapter_failure`. Before correction it reported m/z 700 only
and accepted the wrong time unit as 409.5 minutes. Originals and failure evidence
were preserved. A subsequent probe exposed legitimate `defaultArrayLength=0`
spectra without binary arrays; the reader now emits zero summaries/XICs with
original indices and explicit Missing spectrum state. Declared nonempty spectra
without required arrays still fail. The new reliability regression tests both.

The preflight does not implement every mzML semantic representation (for example,
referenceable CV parameter groups need dedicated compatibility evidence), complete
schema validation, giant token/decompression budgets, or every native decoder
failure. These are explicit follow-up gates, not evidence of universal import support.

Finalization environmental evidence: LLVM GUI test compilation hit disk exhaustion;
reversible NTFS compression of generated PDB/rlib files recovered space without
source/raw/reference deletion. The standalone-window click tests now scroll within
the painted content clip; both original interaction assertions passed separately.
Strict Clippy's new AI-review worker type-complexity finding was fixed with a type
alias, preserving the lint. Full combined-tree checks are recorded below.

## Completed executable recheck — 2026-10-09

The final profiler rebuilt the real headless performance test and ran an immutable
executable copy. Both analytic workloads and the native OpenMS workload passed all
original exact-result assertions. Compact evidence, executable/environment hashes,
independent-reference hashes, source inventories and check-log digests are in
`performance/2026-10-08/final-oct09`. Full samples and stdout/stderr remain in
`target/performance-finalized-oct09-measurements`.

| Workload | 1,000 scans | 10,000 scans |
|---|---:|---:|
| Bounds + raw-source preflight, s | 0.1807 | 1.1316 |
| First XIC, s | 0.0805 | 0.7253 |
| Second XIC, s | 0.0779 | 0.7242 |
| Missing-metadata TIC, s | 0.1194 | 1.1247 |
| Missing-metadata BPC, s | 0.1351 | 1.2486 |
| Two concurrent readers, s | 0.1933 | 2.1837 |
| Cancellation after progress, s | 0.000273 | 0.000602 |
| Sampled peak process-tree RSS, MiB | 85.73 | 86.27 |

10k fixed workloads: 1,000 integrations 0.4176 s; SG/chord 0.4684 s;
1,000 weighted 100-standard fits 0.4338 s; 64-sample targeted batch 0.2483 s;
10k-entry import/search 0.2652/0.3665 s; 100 synced project commits 1.7310 s;
open/restore 0.0106/0.0137 s. Six-sample OpenMS cold/checkpoint reuse:
2.5918/1.9852 s, identical full feature/alignment JSON, sampled tree peak
131.25 MiB. These observations do not supersede the earlier retained 50k/1.157 GB
run or establish bounded RSS, isolated cache latency, release throughput or p95.
The executable and combined source changed during other development; memory/time
changes cannot be attributed to one patch. Original corrected extraction order
and scientific-result checks remain intact.

The unchanged positive/negative OpenMS reference/CLI/MCP/project test passed in
22.78 s total; its internal original 20-second running-job assertion was retained.
The earlier contended timeout remains evidence of an operational latency risk.
Five OpenMS Python tests and all five regenerated independent reference sets passed
again. The optimized release CLI independently returned exactly the corrected debug
metadata bounds (60–714 m/z and 0–6.825 minutes) and rejected unknown time units;
input digests were unchanged. An initial manual probe used the unsupported CLI verb
`analyze`; that failed invocation was retained and corrected to the actual `run`
command before accepting the result. This probe is not a release throughput study.

Final formatting and strict Clippy are checked without lint suppression. Successful
Rust test/build commands and retained failures are listed in VALIDATION_REPORT.md.
Before/after source inventories disclose concurrent GUI/project/export/MCP editing;
checks apply to their built artifacts. An immutable isolated release snapshot,
platform matrix, native GUI study, complete mzML schema/CV-group compatibility,
resource certification and real-instrument method validation remain open.
