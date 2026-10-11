# Validation

Last checked: **11 October 2026**, macOS arm64, Rust 1.99.0, TeX Live 2026.

## Repository preparation

The checks passed from a temporary copy containing only the files eligible for
Git publication, with build outputs, downloaded runtime, Cargo cache and the
local scratch project omitted. Dependencies and PDFium were provided explicitly
from the existing cache; this verifies source completeness and relocation, not
a fresh network download.

| Check | Result |
| --- | --- |
| Core unit/contract/CLI/Git tests | 63 passed |
| Real TeX, SyncTeX and export integrations | 7 passed |
| Desktop input/layout/state tests | 114 passed |
| Real PDF rendering/index/navigation/export tests | 7 passed |
| Native block callback tests | 2 passed |
| Vendored block documentation examples | 2 passed |
| Repository script and packaging regressions | 8 passed |
| Rust formatting and strict Clippy, core and desktop | Passed |
| Patched macOS dependency, future incompatibilities denied | Passed |
| Shell syntax and workflow/issue-form YAML syntax | Passed |
| Local documentation links and publication whitespace | Passed |
| Welcome, multi-file and 120-page example projects | Validated and compiled |

That is **201 correctness tests plus 2 vendored doctests**. The two manual
profiling tests remain excluded. The script/ignore/packaging tests were written
first: initial failures reproduced mandatory TeX in ordinary checks, silently
accepted arguments, shared-example build output, missing release archives and
late failure for a missing PDFium runtime.

The installed universal macOS Biber binary failed during its own `lipo` startup,
before reading a project. The full integration run used an arm64 slice extracted
from that same installed binary into a temporary directory and placed first on
PATH. Biber reported version 2.21. No system TeX files were changed. Ordinary
checks and PDF/BibTeX workflows did not need this workaround.

The release build passed without Rust warnings. The macOS ZIP was checked for
CRC integrity, executable modes, the relative external-settings symlink and
version metadata. Extracting it with `ditto` retained a valid strict recursive
code signature. Packaging tests also exercised Linux archive creation using
stub build commands and verified that existing settings and notices survive.
This is script coverage, not native Linux execution.

## Repeat the checks

After [development setup](../CONTRIBUTING.md#development-setup):

```sh
python3 -m unittest discover -s scripts/tests -v
./scripts/check.sh --desktop --tex
./scripts/package.sh
```

Core and desktop suites run without a display. Real integrations are opt-in;
CI installs their dependencies and uses the same script on macOS and Linux.

## Native behavior and remaining coverage

The preceding v0.9.5 native QA verified 90% width fitting, browser-aware geometry,
single/double-Space input and current-page positioning in an isolated app.
Earlier checks covered editing, completion, prose movement, project switching,
Git gutter interactions, rendering, search and word-level SyncTeX navigation.
This repository preparation changes tooling/docs/fixtures, not runtime behavior;
it did not repeat those UI interactions. Detailed observations and the original
red/green cycles are retained in [validation history](validation-history.md).

GitHub Actions has not been run remotely for this prepared tree. Native Linux
execution, Rust 1.90 minimum-version verification, notarization, accessibility,
IME and complex third-party PDF coverage remain outstanding. Performance records
are host/fixture-specific; they are not general latency or memory guarantees.
