# Validation history

Historical results and implementation notes. [Current validation](validation.md)
supersedes older platform limits and build instructions below.

Date: 11 October 2026. Host: macOS arm64. Rust: 1.99.0.

## Current v0.9.5 PDF width fitting

All **121 desktop correctness tests** pass (114 ordinary and 7 real TeX/PDF
integrations), with formatting and strict Clippy clean. Six new regressions
were written before implementation, alongside updated layout expectations.
Initial runs reproduced missing configuration/actions, the previous whole-page
default, incorrect width geometry, and tall pages opening at their middle.
Additional failing assertions caught double taps surviving an intervening
PageDown command and overview-to-width fitting landing on blank page space.

Full egui frames verify 90% of the actual canvas width with the browser open
and closed, portrait/landscape pages, resizing, current-page retention, delayed
PDF metadata, and no typesetting for fit commands. Input coverage includes
immediate single taps, released double taps within 350 ms, held keys, timeout,
multiple taps in one frame, custom/disabled bindings, invalid configuration,
and normal spaces in source, search and launcher fields. Context changes and
intervening commands cancel a pending double tap.

Native QA used an isolated release app and a disposable two-page portrait
project. Source-to-PDF fitted the available width, including beside the file
browser. Space restored width after manual zoom; double-Space showed the whole
current page. After advancing to page two, switching from its overview back to
width kept page two selected and showed its beginning. The isolated app was
closed afterward; the user's running Folio instance was left open.

The ordinary suite and strict Clippy were repeated after the final overview
positioning fix, and that final binary passed the native check. The seven real
TeX/PDF integrations passed earlier in this change, before that small command
positioning adjustment. The suite totals **193 correctness tests**, including
the unchanged 70 core and two native-block tests from previous runs; those
unaffected suites were not repeated. The two manual profiling tests remain
excluded. No dependencies were added.

The packaged release is **v0.9.5 / build 18** at `dist/Folio.app`. Its external
TOML now uses width fitting, a 0.9 width fraction, Space for width and Space Space
for the whole page; other settings were preserved. The actual packaged TOML
also passed the completion input regression. The release build has no Rust
warnings, and its plist and strict recursive code signature validate. Linux
execution remains unverified on this macOS host.

## v0.9.4 prose navigation

All **115 desktop correctness tests** pass (108 ordinary and 7 real TeX/PDF
integrations), with formatting and strict Clippy clean. Ten new regressions
were written before implementation: the initial runs failed on missing boundary
behavior, unregistered settings and unchanged editor carets. They now cover
blank-line runs, soft lines, CRLF, actual versus quoted/commented `\par`, Unicode,
quotes/closing brackets, common abbreviations, decimals, comments, math,
verbatim, incomplete input and cache refresh after unsaved edits.

Full egui frames verify Cmd/Ctrl+D/U/J/K against the live caret, collapsed mouse
selections, unchanged text/revisions, completion dismissal, native arrow movement,
custom/disabled bindings and explicit command-palette invocation. Focus tests
cover Find, launcher search, confirmation dialogs, PDF and an empty workspace.
Geometry tests check that visible jumps leave the viewport steady and offscreen
destinations become visible. Defaults also pass shortcut conflict validation
for both macOS and Linux modifier aliases.

Native QA used an isolated release app and a disposable project. After a mouse
click, Cmd+J/K moved to the sentence end/start; Cmd+D crossed a soft line break
to the paragraph end, and Cmd+U returned to a paragraph start. A jump through a
65-line paragraph revealed its end on line 73. The buffer stayed clean. The
test app was closed after verification. User project buffers were not opened.

The suite totals **187 correctness tests**, including the unchanged 70 core and
two native-block tests from previous runs; those unaffected suites were not
repeated. The two manual profiling tests remain excluded. Motion ranges reuse
syntax tokens and are cached on edits, with binary search for repeated boundary
lookups. Sentence recognition uses lightweight punctuation heuristics; it does
not expand TeX or perform language analysis. No dependencies were added.

The packaged release is **v0.9.4 / build 17** at `dist/Folio.app`. Its external
TOML now includes the four motion bindings, with other settings preserved. The
packaged configuration also passed the completion input regression. The release
build has no Rust warnings; the plist and strict recursive code signature
validate. Linux execution remains unverified on this macOS host.

## v0.9.3 completion bindings

The existing app bundle retained older external settings where Tab accepted a
suggestion. Its three completion bindings now match the source defaults: Tab/
Down cycles forward, Shift+Tab/Up cycles backward, and Enter accepts. Other
external settings were preserved. The app must restart to load this change;
the bundle was re-signed and its strict recursive signature verified.

A new input regression first failed against the actual packaged configuration
because Tab accepted and closed the menu. It now passes against both that file
and the defaults. Full egui frames check selection wraparound, keyboard focus,
no indentation while cycling, and no extra newline when accepting. To repeat
the packaged-settings check, set `FOLIO_TEST_COMPLETION_CONFIG` to the path of
the bundle's `Contents/Resources/folio.toml` and run the desktop test filter
`completion_tab_cycles`.

All **98 ordinary desktop tests** pass, with formatting and strict Clippy clean.
The full suite now totals **177 correctness tests** including the unchanged
seven real-PDF, 70 core and two native-block tests from previous runs; those
unaffected checks were not repeated for this configuration-only change.

## v0.9.3 source highlighting

All **104 desktop correctness tests** pass (97 ordinary and 7 real-PDF
integrations), with formatting and strict Clippy clean. Twelve new regressions
were written before implementation. The initial runs reproduced missing TOML/
JSON colors and zero bracket outlines in the actual editor rendering path.
Coverage includes file-type dispatch, escaped/Unicode strings, TOML table and
dotted keys, multiline/literal strings, inline tables and numeric/date/literal
values. Lossless tokenization is checked at every UTF-8 edit boundary in mixed
fixtures. Bracket tests cover nested groups, both sides of the caret, comments,
verbatim and false blocks, math delimiters, malformed nesting, and replacement
of stale positions after edits. Headless egui tests verify glyph alignment,
selection suppression, project/switcher inclusion and absence of LaTeX
completions/diagnostics/outline entries in configuration surfaces.

The suite totals **176 correctness tests**, including the unchanged 70 core and
two native-block tests from previous runs; those unaffected suites were not
repeated. The two manual profiling tests remain excluded. No new dependencies
were introduced. Syntax and bracket indexing are cached on text changes;
caret movement only looks up and paints the relevant pair.

Native QA used an isolated release bundle and disposable project. JSON opened
from the surface switcher and TOML from the file browser, with distinct key,
string, number and literal colors. Matching array brackets showed paired
outlines, while brackets inside a JSON string stayed unhighlighted. Typing an
unfinished TOML array showed an unmatched outline; completing it immediately
updated both brackets. Undo restored the clean buffer before the test app was
closed. User project files and settings were not changed.

The packaged release is **v0.9.3 / build 16** at `dist/Folio.app`. Its release
build has no Rust warnings, and the plist and strict recursive code signature
validate. Linux execution remains unverified on this macOS host.

## v0.9.2 whole-page preview fitting

All **92 desktop correctness tests** pass (85 ordinary and 7 real-PDF
integrations), along with formatting and strict Clippy. Five added regressions
cover whole-page fitting on source-to-PDF transitions, the browser's actual width,
height-only/window/browser changes while retaining the current page, TOML policy
validation, manual-zoom opt-out and width fitting, delayed metadata, mixed page
sizes, page-count shrinkage and fitted SyncTeX destinations. Before implementation,
the transition test reproduced a page extending to 1,270 points in a canvas ending
at 743; the new setting was also rejected as unknown. Both now pass.

The default is `[preview] fit_on_switch = "page"`; `"width"` and `"keep"` are
available. Manual zoom persists while staying in PDF, and a cached surface switch
does not trigger compilation or discard cached page textures. Existing external
configuration files inherit the new default without being overwritten. The
suite totals 164 correctness tests including the unchanged core and native-block
coverage from the preceding runs; those unaffected suites were not repeated.

Native QA used an isolated release bundle and a disposable two-page landscape
project. The first compilation displayed a complete page beside the browser.
Page 2 stayed current after manual zoom, source/PDF switching, and hiding the
browser; hiding it visibly enlarged the fitted page. Browser widths and window
resizes are covered by headless egui geometry tests. The native automation's
splitter drag did not change its width, so a native drag-resize result is not
claimed. The test instance was closed without touching the user's running Folio
instance or project buffers.

The packaged release is **v0.9.2 / build 15** at `dist/Folio.app`, with validated
plist and strict recursive code signature. The release build has no Rust warnings.
Linux execution remains unverified on this macOS host.

## v0.9.1 Rust compatibility

The warning was traced to `block` 0.1.6 through both `metal` and `wgpu-hal`.
Compiling its unmodified source as a local package with
`-D future-incompatible` failed on `_NSConcreteStackBlock`'s uninhabited `Class`.
The same check passes after the documented patch in `vendor/block`. Explicit C
ABI declarations also remove deprecation warnings that Cargo previously capped
for the registry dependency. No diagnostics are suppressed.

Validation for this change passed all **87 desktop correctness tests** (80
ordinary and 7 real-PDF integrations), formatting and strict Clippy. Two new
native callback tests and two upstream documentation examples pass in both debug
and release profiles. They cover arguments, captured state, macOS heap copying,
retaining and exactly-once destruction. The macOS check script includes the
compiler regression check and callback tests. Together with the unchanged 70
core checks from the preceding full run, the suite now has 159 correctness tests,
plus the two vendored documentation examples.

The release build with `--future-incompat-report` reports **0 dependencies with
future-incompatible warnings**. Cargo's Linux dependency tree does not include
`block`; Linux execution was not repeated or verified on this host.

The packaged release is **v0.9.1 / build 14** at `dist/Folio.app`. Its plist and
strict recursive code signature validate; the vendored MIT notice is bundled.
The package script's `replacing existing signature` lines are expected when
re-signing an existing local bundle. A separate copy of the final bundle launched
and rendered a responsive command palette through the native GPU backend, then
was closed. The user's running Folio instance and project buffers were untouched.

## v0.9.0 native visual system

The full check script passed **157 correctness tests**: 70 core/CLI/Git/real-TeX
tests and 87 desktop/real-PDF tests. Formatting and strict Clippy passed. The two
manual profiling tests remain excluded. The test process used the temporary
Biber compatibility wrapper described below. An initial run hit a transient
failure in the existing process-termination fixture (`TERM must precede KILL`);
that unchanged test passed in isolation and in the final full run.

Six added regressions cover button-free writing with retained line numbers,
full-width launcher hit targets, backwards search with custom bindings, exact
gutter/source baselines, bounded painting and keyboard navigation over 4,000
files, and backwards PDF search. Test-first changes exposed permanent controls,
unclickable row space, the missing search binding, a 4.5-point gutter offset,
and an unconfigured Enter key stealing search focus. The existing browser
pointer/focus regression now opens and closes the browser by its shortcut.

The UI uses native platform fonts with bundled fallbacks, loaded once. Shared
flat surfaces and text rows need no new dependencies or image assets. Results
are virtualized, filtering is cached between query changes, and widget/caret
animation is disabled. This is not a measured speed comparison with Tauri or
web interfaces; no such comparison is claimed.

Native QA used an isolated release bundle and disposable project. Observed:
the welcome screen, transparent macOS titlebar with project/path caption,
browser/source switching, keyboard search with a visible source highlight,
successful PDF compilation, and Cmd-double-click selecting the exact source
word. The packaged release is **v0.9.0 / build 13** at `dist/Folio.app`; its plist
and strict recursive code signature validate. Existing external settings are
preserved. Linux execution remains unverified locally; the existing `block`
dependency future-compatibility warning remains.

The final native pass confirmed aligned source/gutter baselines and compact
palette rows. A missing return-arrow glyph was replaced with a plain `enter`
label; all 80 ordinary desktop tests, formatting and strict Clippy passed again
before the final bundle rebuild.

## v0.8.0 project-only workspaces

The full check script passed **151 correctness tests**: 70 core/CLI/Git/real-TeX
tests and 81 desktop/real-PDF tests. Formatting and strict Clippy passed. The two
manual profiling tests remain excluded. The test process used the temporary
Biber compatibility wrapper described below.

Seven new regression tests were written before implementation. They cover
directory/manifest-only opening, configured defaults in new manifests, CLI
refusal before compilation, preservation of an active project's unsaved buffers
when a loose file is refused, source/navigation containment including escaping
symlinks, welcome-screen actions, and migration of legacy shortcut settings.
The relative-manifest CLI case also reproduced an empty-parent path bug before
its fix. Existing desktop and CLI fixtures now create actual projects; real
PDFium/SyncTeX tests use the managed `examples/welcome` fixture.

Native QA used an isolated copy of the final bundle with the preserved legacy
settings file. The welcome screen shows New project and Open project. Cmd+O
opened an existing project through the folder chooser; Cmd+N created and opened
a new structured project; Cmd+Shift+N showed the separate Add project file popup.
Cmd+B revealed its inventory, clicking `sections/introduction.tex` opened that
source, and Cmd+2 compiled and displayed the project's PDF. Line numbers stayed
visible. The newly created project also passed CLI validation.

The packaged release is **v0.8.0 / build 12** at `dist/Folio.app`. The bundle plist
and strict recursive code-signature verification pass. Existing settings remain
intact. Linux execution remains unverified locally; the existing dependency
future-compatibility warning for `block` remains.

## v0.7.1 project file browser

The full check script passed **144 correctness tests**: 67 core/CLI/Git/real-TeX
tests and 77 desktop/real-PDF tests. Formatting and strict Clippy passed. After
the final icon-only adjustment, all 70 ordinary desktop tests, formatting and
Clippy passed again. The two manual profiling tests remain excluded. The test
process used the temporary Biber compatibility wrapper described below.

Seven new regression tests cover a local, sorted inventory with empty folders
and assets; hidden build/internal directories and symlinks; bounded traversal;
folder state across scans; external file additions/deletions; exclusion of
out-of-project recorder dependencies; configurable shortcuts; dirty-buffer
preservation; project switches; and real egui pointer events for opening Files,
opening a source, collapsing a folder and closing the browser. The pointer test
first reproduced a lost-focus bug after file opening. The fix reasserts requested
editor focus after processing the browser click; immediate typing now passes.

Native QA used an isolated copy of the packaged app and a disposable project.
Observed: the Files button and Cmd+B toggle the sidebar; clicking a source opens
it ready for typing; unsaved text and its dirty marker survive switching files;
line numbers remain visible with the browser open or closed; folders collapse;
the + menu exposes file creation and figure import; and a project PDF asset opens
in macOS Preview. Missing Unicode folder/asset glyphs discovered during native
QA were replaced with vector icons and checked in the final rebuilt bundle.

The browser uses background inventory and virtualized rows. Scans are bounded to
4,000 entries and 12 recursive levels, with an incomplete-inventory notice when
limited or unreadable. Hidden files/folders are omitted except `.gitignore`;
generated/internal folders and symlinks are excluded. Folder expansion and panel
visibility persist while the app runs, not across restarts. Asset viewing uses
`open` on macOS and `xdg-open` on Linux. Linux execution remains unverified locally.

The packaged release is **v0.7.1 / build 11** at `dist/Folio.app`. The bundle plist
and strict recursive code-signature verification pass. Existing user settings
are preserved; omitted `project_files` bindings inherit the new Cmd/Ctrl+B default.

## v0.7.0 structured projects and flat export

The final validation covers **137 passing correctness tests**: 67 core/CLI/Git/
real-TeX tests and 70 desktop/real-PDF tests. The full script passed 135 tests;
following native QA, two new regressions for add-file focus and surface cycling
during export first failed, then passed with the fixes. The final desktop suite
(63 ordinary tests), formatting and strict Clippy passed again. The seven real-PDF
integrations had already passed, including the new export-content/cancellation
workflow. The two manual profiling tests remain excluded.

TDD covered strict manifest parsing, portable project discovery, authoritative
engine/timeout settings, folder roles, safe file creation, required-folder Git
retention, symlink rejection (including the build-output directory), and CLI
creation/check/build policy. Project-switch tests preserve unsaved buffers until
Save/Discard is chosen, retain buffers after a save conflict, reject switching
during a build, and keep a malformed manifest editable without falling back to
loose-file build settings.

Export tests first reproduced missing APIs, dotted figure-name resolution failures,
and unsafe graphic-path interpretation. The resulting pipeline preserves comments
and verbatim examples, rejects include cycles/computed paths/unsupported include
constructs, expands sections and tables, disambiguates colliding asset names, and
copies local BibTeX styles. Real pdfLaTeX builds verify both bibliography modes with
BibTeX and BibLaTeX/Biber, including an independent rebuild of the published output.
A desktop integration uses PDFium to assert the exported PDF contains text from an
unsaved editor buffer, its included table, and the rendered bibliography. It also
checks export cancellation and staging cleanup. Existing directories are refused;
the author's original files remain intact.

Native QA used an isolated bundle and disposable project. Observed: New project
via the command palette and native folder dialog; Add project file into `tables/`;
Import figure into `figures/`; a successful preview with the table and PDF figure;
both bibliography options in the export dialog; a completed inline-mode export;
and editing/validating the manifest as a source surface with XeLaTeX and a changed
timeout. Inspection of the native export confirmed `main.tex`, `main.pdf`,
`figure.pdf`, and `BUILD.txt` at the root, table content expanded into `main.tex`,
and the structured authoring table still present. Line numbers stayed visible;
project controls remained transient. The final rebuilt bundle was relaunched,
Open project successfully loaded the existing folder, and Cmd+N followed by typing
immediately replaced the suggested filename without an extra click or Tab.

The packaged release is **v0.7.0 / build 10** at `dist/Folio.app`. Both macOS code
signatures and the bundle plist validate.

The installed macOS universal Biber 2.21 tries `lipo -extract_family`, which this
host's older Command Line Tools do not support. For Biber integration tests only,
its unmodified arm64 slice was extracted to `/tmp/folio-test-bin/biber` with
`lipo -thin arm64`, then placed first on the test process's PATH. No installed TeX
binary was changed. The full local test invocation therefore used
`PATH="/tmp/folio-test-bin:$PATH" ./scripts/check.sh --desktop`.

Flat export depends on TeX's installed `latexpand` (v1.7.2 here). It is intentionally
explicit about macro-computed paths, `includeonly`, `subfile`, and scoped/changing
`graphicspath`; these need literal paths or ordinary inputs. Compilation validation
does not prove visual equivalence for arbitrary TeX macros. Linux CI dependencies
and package workflow were updated, but Linux execution remains unverified on this
Mac. The existing `block` dependency's future-compatibility warning remains.

## v0.6.2 word-level PDF navigation

`scripts/check.sh --desktop` passes **116 correctness tests**, including strict
Clippy and formatting: 53 core/CLI/Git/real-TeX tests and 63 desktop/real-PDF tests.
The two manual profiling tests remain excluded.

Regression tests first demonstrated the old behavior: a PDF click selected one
glyph, the Go gesture used an earlier selection, and inverse navigation selected
the entire source line. The new behavior selects the clicked word and carries its
context through the asynchronous SyncTeX request. Refinement selects the exact
Unicode character range in the current source buffer, including unsaved text.
Tests cover repeated words, formatting commands, comments and metadata keys,
blocks starting before the local search window, ligatures, PDF line-wrap
hyphenation, ambiguous matches and generated output. Custom mouse bindings and
navigation cancellation remain covered by the desktop suite.

A real integration compiles an included chapter with **pdfLaTeX and XeLaTeX**,
indexes the actual PDF through PDFium, clicks each of two `marker` occurrences,
then `café` and `efficient`, and checks exact source ranges after inverse SyncTeX.
The existing forward/inverse build workflow also passes.

Native QA used an isolated v0.6.2 bundle and disposable two-file project. An
ordinary click selected only the first `marker` in the PDF. Cmd-double-click on
the second `marker` opened `chapter.tex` and selected precisely `marker` inside
`\textbf{second marker}` on line 2, overriding the earlier PDF selection. Line
numbers remained visible, and no permanent UI was added.

The packaged macOS release is **v0.6.2 / build 9**. Source-to-PDF still uses
SyncTeX's region. Inverse navigation falls back to a caret on the mapped line when
the rendered word has no unambiguous nearby literal source match; arbitrary macro
expansion and generated text cannot always provide an exact word correspondence.
Linux execution remains unverified on this Mac.

## v0.6.1 reliability audit

`scripts/check.sh --desktop` passes **107 correctness tests**: 47 core/CLI/Git/
real-TeX tests and 60 desktop/real-PDF tests. Format checks and strict Clippy pass.
The two manual performance measurements are excluded from the correctness count.

The supplied report was checked against executable regressions. The page-shrink
test initially panicked at old page index 7 after a 10-to-5-page rebuild. The
trailing-comma test initially returned one bibliography entry instead of three.
Non-UTF-8 logs and missing maps reproduced false build failures. The stationary
32 MiB PDF-cache fixture initially rendered 40 pages across twelve frames;
the corrected planner settles and stays within its budget. The report's isolated
oversized-page explanation was incomplete: the old planner already capped an
individual render near 32 MiB, but drafts, refinements and neighboring pages
still evicted each other. The fix budgets the whole resident set and respects
the device's maximum texture dimension.

| Finding | Result |
| --- | --- |
| Shrinking PDF / old anchor | Geometry clears with the generation; anchor lookup is checked. Empty replacements are covered too. |
| BibTeX trailing comma | Terminators are preserved; following entries and citation metadata remain independent. |
| Non-UTF-8 build log | Output is decoded lossily; successful PDFs remain successful. |
| Preview error freezes rendering | Errors distinguish opening, rendering, indexing and backend failure. Other pages and the previous document keep rendering; failed render requests cannot spin. |
| Cancellation / timeout | TERM precedes bounded KILL of the process group. An interruption marker causes auxiliary cleanup on retry, retaining successful snapshots and unrelated files. Timeout is configurable, including disabled. |
| Missing SyncTeX | Maps are optional; both formats are copied. Partial snapshots are removed on failure. Missing-map navigation has a specific error. |
| Completion on each keystroke | Disk indexes are prepared on the scan worker; tab revisions reuse every unchanged index. Open/save scans also leave the UI thread. |
| Cache churn | Resident pages share the budget and each page retains one resolution. |
| Wrapped search | Whitespace and line-end hyphenation are normalized with original glyph ranges preserved. |
| Source encoding | UTF-8 remains a deliberate limitation, now reported with the filename and conversion guidance. No lossy source save. |
| Save All | All conflicts are checked and reported before writes; later I/O errors are collected rather than skipping remaining tabs. This is not a filesystem-wide transaction. |
| Diagnostics | Timestamps/ratios are rejected while custom source extensions such as .tikz remain valid. Windows drive-letter paths already parsed correctly and remain covered. |
| Incomplete syntax | Unterminated math recovers at a line boundary; nested primitive conditionals skip to the matching `fi`. Balanced multiline math is preserved. |
| Completion context | Macros/braces in citation notes work; inline bibliography and label keys use case-insensitive prefixes. |
| SyncTeX columns | GUI and CLI use 0 for an unspecified column, preserving line navigation without guessing Unicode byte units. |
| Glyph/transform geometry | Missing, zero-area or nonfinite glyph boxes do not participate in hit testing. Invalid transforms cannot produce screen coordinates. |
| CRLF Git comparisons | Git ignores carriage returns at line endings while still detecting actual line changes. |
| Source frame/jump work | Newline count is cached per edit; line jumps stream characters without a whole-file `Vec<char>`. |

Additional regressions found during the audit cover malformed Unicode BibTeX
fields (previously a panic), stale errors cancelling newer PDF loads, stale
SyncTeX maps being paired with changed PDFs, the project walk exceeding its file
limit inside a wide directory, and bibliography references in included files
using the wrong working directory. A repeated real-PDF run also exposed an
initialization-order failure: `bind()` treated PDFium’s already-initialized global
bindings as unavailable. A deterministic two-bind test reproduced it; the adapter
now reuses the initialized library. Settings retain strict validation and
executable-before-home lookup. There is no individual tab-removal path in the
current workspace; source cycling clamps indices and opening selects an existing
or newly appended tab. The alleged tab-close panic was not present.

The installed `synctex help view` and the
[upstream API](https://github.com/jlaurens/synctex/blob/main/synctex_parser.h)
define 1-based positions and 0 as an unspecified column; the reported 1-based
GUI value was not itself an off-by-one bug. Real navigation tests cover both
directions and Unicode XeLaTeX output. A custom latexmkrc test switches between
compressed, disabled and uncompressed SyncTeX to verify optional/stale-map behavior.
Real TeX also successfully rebuilds after timeout with deliberately corrupt aux data.

Release-mode completion microbenchmark, this Mac: a project with 25,000 labels
and 10,000 bibliography entries averaged **16.94 ms/query** rebuilding indexes,
versus **52.26 µs/query** using cached indexes. This measures completion queries,
not complete editor frame latency. Reproduce with:
`cargo test --offline --release --test bibliography_completion profile_cached_project_completion -- --ignored --nocapture`.

Native QA used the v0.6.1 app in an isolated bundle with a 32 MiB texture budget.
While it displayed page 8 of a ten-page fixture, an external edit reduced the
document to five pages; automatic recompilation completed and the viewer remained
open on page 5. Scrolling displayed the changed text, palette zoom worked, and
selection plus Cmd-double-click navigated to line 14 of the included source and
back to the matching PDF region. The citation popup showed both trailing-comma
entries with their own metadata. Source line numbers remained visible. The final
macOS release is v0.6.1/build 8, ad-hoc signed with PDFium included. Linux execution
remains unverified on this host.

## v0.6.0 validation (historical)

`./scripts/check.sh --desktop` passes **77 correctness tests** (29 core/CLI/Git/
real-TeX and 48 desktop/real-PDF), format checks and strict Clippy. The manual PDF
profiling test remains excluded.

TDD first produced failures for surface cycling, switcher matching, the permanent
toolbar and missing workspace bindings. The implementation removes the toolbar,
source tabs, status bar and PDF toolbar, while tests verify cycling through open
buffers and cached PDF without rebuilding or performing SyncTeX navigation.
Selection is preserved on returning to source. The modal input test then exposed
a focus-restoration bug on dismissal; the fix waits for the editor layer to
become interactive before returning keyboard focus. Launcher tests cover custom
accept/dismiss keys, filtering without editing source, empty results and typing
after dismissal. Native QA exposed a second focus handoff error when choosing Find
from the palette. A failing regression reproduced search text entering source;
the passing test now verifies immediate typing goes to search in both surfaces.
PDF commands and selection copy work without persistent controls.
The existing configuration, Git, real compilation and bidirectional SyncTeX tests
remain green.

Native macOS QA used a separate v0.6.0 app and disposable Git-backed, multi-file
LaTeX project. The source and PDF surfaces have no persistent app toolbar, tabs
or footer. Verified Cmd+P file filtering/opening, the command palette and right-click
access, Ctrl+Tab cycling, Cmd+1/2 switching, retained source selection and PDF scroll,
source/PDF search focus, outline, Git marks/comparison, palette zoom/fit, and Escape
dismissal. Cmd+double-click on a source selection highlighted the compiled PDF;
selecting PDF text and repeating the gesture opened and selected line 3 in the
included chapter.tex. The release bundle is v0.6.0/build 7 and passes strict deep
signature verification. Linux execution remains unverified on this macOS host.

Line-number follow-up: source line numbers are enabled by default and in the
packaged configuration. Existing settings tests first failed on the old default,
then passed with explicit opt-out still supported. All 45 desktop unit tests pass;
native QA confirmed visible line numbers alongside Git marks. The app was rebuilt
and its signature verified.

## v0.5.0 validation (historical)

`./scripts/check.sh --desktop` passes **70 correctness tests** (29 core/CLI/Git/
real-TeX and 41 desktop/real-PDF), format checks and strict Clippy. The manual PDF
profiling test remains excluded.

TDD began with six failing settings tests, followed by failing app/gesture tests:
replacing Save initially left Cmd+S active and the alternate mouse modifier/button
initially did not navigate. Parsing, discovery and real input dispatch now pass.
Coverage includes executable-before-home precedence, absent files, partial
overrides, disabled keys, unreadable/malformed files without silent fallback,
unknown fields, bounds, binding conflicts and platform aliases. UI regressions
exercise remapped Save/Find, scoped search, completion selection/accept/dismiss,
disabled implicit interface zoom, configured font/engine, PDF wheel modifiers,
zoom/cache settings and selection-preserving gestures in both layers. The existing
real TeX/BibTeX/PDF/SyncTeX navigation integration still passes.

Native macOS QA launched a separate v0.5.0 app with an adjacent TOML file:
1100×700 window, 20-point editor font, XeLaTeX, Cmd+Shift+F for Find, Cmd+Shift+S
for Save, no keyboard Go, and Alt+double-click navigation. Cmd+F no longer opened
search; Cmd+Shift+F did. Shortcut help displayed the custom bindings and loaded
path. Alt+double-click on selected source compiled with XeLaTeX and highlighted
the PDF; the reverse gesture opened and selected line 3 in chapter.tex. The
packaged app and its executable-adjacent config link pass strict deep signature
verification. No user home configuration was written.

## v0.4.1 validation (historical)

`./scripts/check.sh --desktop` passes **58 correctness tests** (29 core/CLI/Git/
real-TeX and 29 desktop/real-PDF), format checks and strict Clippy.

The selection-preservation regressions initially failed in both views: the first
Cmd-click collapsed a source range to a caret and a PDF range to its final glyph.
They now preserve the selected range through the first click and navigate from
its start on the second. Tests also cover plain double-clicks, wrong mouse buttons,
click expiration, coalesced click events after modifier release, and exactly one
navigation action per frame. The real TeX/BibTeX/SyncTeX integration now exercises
both navigation directions with pointer gestures through the actual layer UI,
including compilation after an edit and navigation into an included chapter.

Native macOS QA also verified selecting source text then Cmd+left-double-click
to compile and highlight it in the PDF; selecting PDF text and repeating the
gesture opened and selected line 3 of the included chapter.tex. The normal click
stayed in Preview. The v0.4.1 macOS bundle was packaged and its signature verified.

## v0.4 validation (historical)

`./scripts/check.sh --desktop` passes **54 correctness tests** (29 core/CLI/Git/
real-TeX and 25 desktop/real-PDF), format checks and strict Clippy. The manual PDF
microbenchmark is excluded from that count. `scripts/package.sh` produced the
22 MiB macOS v0.4.0 bundle; its plist version and ad-hoc signatures were verified.

New regression tests first reproduced zero displayed displacement in the input
frame of a 120-point scroll and an ignored pinch gesture. After the input-path
change, all 120 points are drawn in that frame, with no additional movement over
12 subsequent empty frames. Pinch preserves the PDF point under the pointer to
within physical-pixel rounding. Additional coverage checks Ctrl+wheel, ignoring
wheel zoom outside the PDF area, immediate offscreen SyncTeX jumps, asynchronous
project-snapshot publication, and rejection of scans superseded by an explicit
refresh. Existing cache, Git, selection, compilation and bidirectional navigation
tests continue to pass.

Native QA used a separate release-built app on Apple M4 / Metal at 2x density,
with the [120-page fixture](../examples/performance/main.tex) and 1,000 additional
small .tex files to exercise periodic scanning. The PDF compiled in 0.80 seconds;
scrolls in both directions and zoom changes from 147% to 176% and 212% were
visually verified. This run used the v0.4 changes before the package-version bump,
so the unmodified CSV header still reports 0.3.0.

[Raw native frame trace](performance/native-frames-m4.csv): 1,800 samples over
30.009 seconds, continuous diagnostic redraws, including four discrete automated
wheel-input frames and zoom-button clicks.

| Measurement | Median | 95th percentile | Maximum |
| --- | ---: | ---: | ---: |
| Native callback interval | 16.677 ms | 17.709 ms | 55.558 ms |
| App update CPU time | 0.157 ms | 0.235 ms | 0.503 ms |
| Previous-frame CPU, including render submission | 0.891 ms | 1.207 ms | 17.946 ms |

These are actual native frame callbacks, not the earlier layout-only benchmark.
They do not measure physical input-to-photon latency or GPU timestamps. Discrete
wheel automation does not reproduce every physical trackpad gesture; the run had
outliers and does not establish a universal 60/120 FPS guarantee. There is no
before/after native trace for this version; the latency regression is demonstrated
by the deterministic input tests. Complex PDFs and Linux still need platform QA.

To record another native run, build release, then launch from a terminal:

```sh
FOLIO_FRAME_TRACE=/tmp/folio-frames.csv ./desktop/target/release/folio examples/performance/main.tex
```

Enter Preview and exercise scrolling/zooming for at least 30 seconds after the
PDF appears. The CSV is written locally; it contains timing/event counts and the
GPU name, without document contents. Normal runs do not enable this recorder.

## v0.3 validation (historical)

`./scripts/check.sh --desktop` passes: **49 correctness tests**, format checks and
strict Clippy for both packages. This includes 29 core/CLI/Git/real-TeX tests and
20 desktop/real-PDF tests. The separate profiling test is intentionally excluded
from this count. `scripts/package.sh` produced the signed macOS v0.3.0 bundle.

The v0.2 layer tests exercise reuse of unchanged PDFs, failure/retry state,
edits during a build, real BibTeX compilation, XeLaTeX Unicode output, selection
navigation through an included chapter and rejection of obsolete navigation
results. Cmd+G in both directions was also observed in the native v0.2 app.

New Git coverage uses real disposable repositories: create two commits, compare
an unsaved Unicode buffer against HEAD and an older commit, check additions,
modifications/deletions, untracked files, unborn HEAD and invalid revisions.
The repository index, HEAD and working file are asserted unchanged. A desktop
test rejects stale worker results after the buffer/baseline changes. Native v0.3
QA showed blue/green gutter markers, the commit picker, and a clickable popup
containing the exact removed and added lines.

PDF red/green coverage: a 10,000-page layout test initially created 10,000 page
widgets; it now creates one in the tested viewport. Scheduling tests verify that
new scroll/zoom requests replace obsolete work, drafts precede refinement,
cached pages are not rendered repeatedly, and only one image uploads per poll.
Tests also verify old pixels survive recompilation but cannot navigate against a
new SyncTeX map until refreshed. Real rendering, glyph hit testing and both
navigation directions still pass. Native v0.3 QA observed a draft page followed
by crisp text at Retina density. Native pointer automation became unavailable
for subsequent scroll/zoom actions (`noWindowsAvailable`), so those actions are
covered by the scheduling/layout tests rather than a claimed native FPS result.

Release measurements on this Mac, using the two-page welcome PDF:

| Stage | Before | After |
| --- | ---: | ---: |
| 10,000-page initial UI layout (synthetic page geometry) | 5.76 ms / 10,000 widgets | 0.56 ms / 1 widget |
| Warm UI layout, same geometry (100-frame average) | not recorded | 0.0069 ms |
| 1856-pixel page rasterization | 5.26 ms, worker | 5.36 ms, worker |
| Color conversion for that page | 3.58 ms, UI thread | 3.71 ms, worker |

These isolate CPU work, not total GPU frame time. The rasterizer itself was not
made faster: the improvements remove UI work and obsolete rendering, prioritize
visible pages, prefetch neighbors and pace uploads. Complex documents still need
real-world testing; there is no universal scrolling-FPS guarantee.

Reproduce the release profiling checks with:

```sh
FOLIO_TEST_PDF="$PWD/examples/.folio/welcome.pdf" CARGO_HOME="$PWD/.cargo-local" \
  cargo test --offline --release --manifest-path desktop/Cargo.toml profile_pdf_pipeline -- --ignored --nocapture
CARGO_HOME="$PWD/.cargo-local" cargo test --offline --release \
  --manifest-path desktop/Cargo.toml long_documents -- --nocapture
```

## Initial v0.1 validation (historical)

`./scripts/check.sh` completed successfully:

- 16 document/language/PDF geometry/build contract tests.
- 2 CLI tests, including invalid arguments and missing input files.
- 3 explicitly enabled real-TeX integration tests (including the macOS Finder PATH regression).
- `cargo fmt --check`.
- `cargo clippy --offline --all-targets -- -D warnings`.

The real-TeX tests compile a multi-file project in a directory containing spaces,
exercise forward and inverse SyncTeX into an included chapter, verify file/line
compiler diagnostics on invalid input, preserve the successful PDF/map after that
failed build, and kill a deliberately endless TeX loop within the timeout.

`folio-cli doctor` found latexmk, pdfLaTeX, XeLaTeX, LuaLaTeX and SyncTeX.
`folio-cli build examples/welcome.tex` produced a two-page PDF. Both pages were
rendered with the installed Ghostscript and visually inspected: headings, math,
references, text and pagination were intact. This checks the fixture; it does not
substitute for testing Folio's PDFium viewer.

`./scripts/check.sh --desktop` passes in full: **31 tests** (21 core/CLI/real-TeX,
6 grammar/layout, 1 native GPU backend, 2 viewer interaction/geometry, and 1
explicitly enabled real-PDF test). Both packages pass formatting and strict Clippy.
The PDFium test renders both fixture pages, searches their text, hit-tests glyph
bounds, and sends a character's page-space center through SyncTeX to the correct
source file and line.

`./scripts/package.sh` produces a **22 MiB** arm64 `dist/Folio.app`, including the
PDFium runtime and its license notices. The plist and local ad-hoc code signatures
were verified. The corrected release launched successfully with the Metal backend.

## GUI checks on macOS

The release app was opened through LaunchServices and inspected at 2x Retina
pixel density. Observed successfully:

- Native Open dialog, choosing `examples/welcome.tex`, and compiling from the GUI.
- Source highlighting, outline entries, source horizontal scrolling, and corrected
  one-to-one logical line numbering.
- PDFium page rendering with intact title, body text, equations and both pages.
- Clicking a PDF sentence selects the correct source line; Show in PDF scrolls
  to and highlights its rendered location.
- Searching for `Search works` finds and highlights the text on page two.
- Text selection visibly highlights PDF text and reveals the Copy control. A
  separate simulated-pointer test verifies selection includes its starting glyph.
- Editing a source comment, saving it to disk, and showing the stale-preview notice.
- Closing a clean window and reopening the packaged app.

The final viewer also reports the actual fit-width scale and uses the predominantly
visible page for its page indicator. Both have regression coverage. Clipboard
contents, undo/redo, dirty-exit dialogs, complex IME input and every possible zoom/
resize combination were not exhaustively verified through native automation.

## Observed red/green cycles

1. The initial 12 contract tests were written before implementation. `cargo test
   --offline` failed with missing document, lexer, PDF geometry, SyncTeX and build
   APIs. Implementing those APIs made all 12 pass.
2. A root-directive test failed before `resolve_root` existed, then passed after
   root resolution and cycle detection were implemented.
3. A regression test verified that a failed build could delete its SyncTeX map
   while the successful PDF remained visible. It failed with a missing map. The
   immutable PDF/map snapshot implementation made the test pass.
4. Two CLI tests failed before the CLI binary existed, then passed after the
   argument/error-handling implementation.
5. Tests for section short titles and a main file referring to itself in its root
   directive failed. The parser/root-resolution changes made both pass.

6. Raw-environment tests initially failed to link because the grammar's scanner
   was omitted from the published crate. They then caught empty-verbatim and
   raw-body outline cases. The scanner and lexer fixes made all five grammar
   tests pass.
7. A compiled-backend assertion reproduced the GUI launch failure: Metal was
   absent. Enabling Metal/Vulkan/OpenGL in the manifest made the test pass and
   the app launch successfully.
8. Tests for dotted document names and Finder's minimal PATH failed with an
   incorrect output filename and `pdflatex: command not found`. Both passed after
   output-path handling and child-process PATH construction were corrected.

9. A headless layout test showed 10 visual rows for two logical source lines.
   Disabling soft wrapping made it pass and restored correct gutter numbering.
10. A simulated PDF drag selected only the final glyph instead of the full range.
    Anchoring selection at the pointer-down position made the test pass.

## Remaining validation

Linux compilation/runtime is unverified. Docker is installed but its configured
OrbStack daemon socket is absent. CI is configured for macOS and Linux, with native
package artifacts, but has not been run remotely.

Startup/RAM, GPU timestamp profiling, complex-PDF and large-source benchmarks remain outstanding. The v0.4 native run above uses a real 120-page PDF; the earlier v0.3 long-document measurement uses synthetic geometry. The 22 MiB figure is bundle disk size, not runtime memory. No performance
parity claim is made. Rotated/cropped third-party PDFs, accessibility and IME need
additional platform QA. The native `block` dependency currently emits a Cargo
future-compatibility warning on Rust 1.99; it does not fail the present build/tests.

The automated checks can be repeated from the populated local dependency cache:

```sh
./scripts/check.sh --desktop
./scripts/package.sh
```
