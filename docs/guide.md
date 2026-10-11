# Using Folio

[Build and run](../README.md#build-and-run) · [Contributing](../CONTRIBUTING.md)

## Projects

Folio works with projects. **Cmd/Ctrl+N** creates one and **Cmd/Ctrl+O** opens a
project folder. Both actions are also on the welcome screen and in Commands
(Cmd/Ctrl+Shift+P). The desktop startup argument accepts a project folder or its
`folio.project.toml`, not an individual source file.
Press **Cmd/Ctrl+B** to toggle the project browser, or right-click the editor and
choose **Project files** from Commands.
Names in the browser follow `[editor] font_size`, one point smaller, and rows scale
with them. Folders start folded. Click a folder to expand/collapse it and a source file to
open it; the folders containing the active file open automatically. The active file
is highlighted; **•** marks unsaved changes. Figure assets open in your system
viewer. Right-click a file to copy its relative path, add a project file or import
a figure. Right-click the browser heading for those project actions and its hide
command. The sidebar resizes and toggles with the same shortcut; its binding is
`project_files` in `folio.toml`.

TOML and JSON files use their own syntax colors for keys, strings, numbers,
booleans and punctuation, including TOML comments, tables and multiline strings.
Both appear in the project browser and surface switcher. Put supporting
configuration/data files in `data/`; the project manifest stays at the root.

Place the caret next to `{}`, `[]` or `()` to outline both matching brackets.
An unmatched bracket gets a muted red outline. Matching respects strings,
comments and LaTeX verbatim content, and also handles `\(…\)` and `\[…\]`.
Highlights stay out of the way while selecting text.

New projects use a fixed layout of ordinary, Git-friendly files:

```text
paper/
  folio.project.toml
  main.tex
  supplement.tex
  references.bib
  sections/introduction.tex
  figures/
  tables/
```

Keep chapter/section sources in `sections/`, figure assets in `figures/`, and table
sources in `tables/`. Optional `bibliography/`, `styles/`, and `data/` hold extra
bibliographies, local packages/classes/styles, and supporting data. `preamble.tex`
may live at the root. `supplement.tex` is a standalone document for supplementary
material: it has its own preamble, `main.tex` does not include it, and projects
without it remain valid. Required empty folders contain `.gitkeep` so a Git checkout
retains the layout. Build output stays in `.folio/`.

**Add project file** (Cmd/Ctrl+Shift+N with the default settings) creates a file by relative
path, such as `sections/methods.tex` or `tables/results.tex`. Include it from
`main.tex` with `\input{sections/methods}` or `\input{tables/results}`.
**Import figure** copies an image into `figures/` without replacing an existing
file, and shows the corresponding `\includegraphics` reference. The surface
switcher finds your source, bibliography and settings files by name or path.

**Project settings** opens the manifest as an editable surface:

```toml
schema = 1

[project]
name = "Paper"
main = "main.tex"

[build]
engine = "pdflatex" # pdflatex, xelatex, lualatex
timeout_seconds = 120 # 0 disables the time limit
```

Save to apply project settings without restarting. The manifest determines the
main file, compiler and timeout for both GUI and CLI builds. Managed builds use
`latexmk -norc` and disable shell escape; custom `latexmkrc` files and arbitrary
build commands are not part of schema 1. **Validate project** checks the structure
and settings. Managed builds/export reject misplaced files, symlinks, missing
required folders, unknown fields and invalid settings with an actionable error.
Hidden files and folders (`.github/`, `.gitattributes`) and editor backup files
(`main.tex~`, `.swp`, `.bak`) are ignored, not rejected.

Opening a different project asks how to handle unsaved files. Standalone sources
and folders without a manifest are refused. To bring existing LaTeX into Folio,
create a project, then place the sources and assets in its documented folders.
Files within the current project open through the browser or surface switcher;
external dependencies cannot become editable buffers. Files and the manifest
remain readable by any text editor, Git tool or local LLM.

### Project file and bibliographies

`folio.project.toml` holds every per-project setting. Unknown keys are rejected,
so a typo is reported rather than ignored. This is the complete file:

```toml
schema = 1 # manifest format version; always 1

[project]
name = "Paper"    # shown in the window caption
main = "main.tex" # the document Folio typesets; must be main.tex

[build]
engine = "pdflatex"   # pdflatex, xelatex or lualatex
timeout_seconds = 120 # a first build with a bibliography runs LaTeX several times; 0 disables the limit
```

There is no bibliography setting. Folio runs `latexmk`, which reads `main.tex`,
decides whether BibTeX or Biber is needed, runs it, and reruns LaTeX until the
citations settle. What you write in `main.tex` selects the tool:

| | BibTeX | BibLaTeX with Biber |
| --- | --- | --- |
| Preamble | nothing required | `\usepackage[backend=biber]{biblatex}` and one `\addbibresource{references.bib}` per file, with the extension |
| Where the list appears | `\bibliographystyle{plain}` then `\bibliography{references}`, without the extension; separate several files with commas | `\printbibliography` |
| Tool you need installed | `bibtex` (in every TeX Live and MacTeX) | the `biblatex` package and the `biber` program |
| Template | [examples/bibtex](../examples/bibtex/) | [examples/biblatex](../examples/biblatex/) |

Keep entries in `references.bib` at the project root, and further `.bib` files in
`bibliography/`; a local `.bst` style goes in `styles/`. Paths in
`\bibliography` and `\addbibresource` are relative to `main.tex`, for example
`bibliography/books`. Both templates are complete projects with a commented
`folio.project.toml`: copy one, or copy its project file beside your own `main.tex`.

If a bibliography does not appear, open the compiler output from the build panel.
A citation shown as **[?]** means its key is not in any listed `.bib` file. A
build that stops with Biber not found needs the `biber` program installed; see
[development setup](../CONTRIBUTING.md#development-setup) for the package names.

### Flat export

Choose **Export flat project** in Commands, select a bibliography mode, then choose
a **new folder outside the authoring project**:

- **Keep bibliography files (.bib):** retain BibTeX/BibLaTeX data and normal citation
  commands. The compiled `.bbl` is also included when one is produced.
- **Embed compiled bibliography in main.tex:** expand traditional BibTeX's rendered
  bibliography into the document. For BibLaTeX, embed its compiled data using
  `filecontents*`, preserving BibLaTeX formatting without needing Biber or `.bib`
  files for the exported build.

Both modes expand included sections and tables into **one `main.tex`**, put required
figures/resources **beside it**, and resolve duplicate asset names deterministically.
The folder includes a verified `main.pdf` and `BUILD.txt` with the exact build command.
For embedded bibliographies, that command includes `-bibtex-`. When the project has
a `supplement.tex`, it is flattened and verified the same way and exported beside the
paper as `supplement.tex` and `supplement.pdf`. The editor preview and
`folio-cli build` still typeset `main.tex` only.

Export runs in the background from a saved snapshot. It builds the snapshot,
flattens it, builds the export, and only then creates the destination. **Cancel
typesetting or export** is available in Commands. Failures remove staging files;
existing output folders are never intentionally replaced. Authoring files remain
structured. The destination is a normal LaTeX folder and needs no Folio manifest.

The expansion engine is TeX Live's [latexpand](https://ctan.org/pkg/latexpand),
with Folio handling validation, protected comments/verbatim, asset rewriting,
bibliography modes and verification. Literal `\input`/`\include` and supported
`import` includes are handled. Macro-computed paths, cycles, `\includeonly`,
`\subfile`, and changing/scoped `\graphicspath` declarations require simplification
before export; use explicit project-relative resource paths. TeX is programmable,
so successful compilation is a useful check, not a proof of visual equivalence
for arbitrary macros. Custom package resource conventions may need explicit paths.

CLI equivalents (after `cargo build --bin folio-cli`):

```sh
./target/debug/folio-cli project new ../paper xe
./target/debug/folio-cli project check ../paper
./target/debug/folio-cli build ../paper
./target/debug/folio-cli project export ../paper ../submission --bibliography inline
./target/debug/folio-cli project export ../paper ../submission-bib --bibliography bibtex
```

## Settings

Folio reads **`folio.toml`** at launch, in this order:

1. Next to the executable. In the macOS bundle this is
   `Folio.app/Contents/MacOS/folio.toml`; on Linux it is beside `folio`.
2. `$HOME/.config/folio/folio.toml` (normally `/Users/$USER/.config/folio/folio.toml`
   on macOS, `/home/$USER/.config/folio/folio.toml` on Linux).
3. Bundled defaults if neither file exists.

The package includes an editable file beside the executable (on macOS, a link to
`Contents/Resources/folio.toml`). To use your home configuration instead, copy its
contents to `~/.config/folio/folio.toml` and remove the adjacent file/link. Packaging
preserves existing settings. Direct Cargo builds use the same lookup; you can
copy [the default file](../folio.toml) beside the built binary or into the home
directory above. The working directory is never searched.

All Folio application shortcuts, completion/search keys, navigation clicks and
wheel modifiers are defined in [folio.toml](../folio.toml), alongside build defaults for newly created project manifests,
editor font size/line numbers/automatic completion, PDF fit/zoom/cache and initial window size.
`Primary` means Cmd on macOS and Ctrl on Linux; `Ctrl` always means Control.
Existing projects always compile using their manifest. Old `open`/`new` shortcut
keys migrate to `open_project`/`new_project`; explicitly configured nonempty
project bindings take precedence. Retired engine/main-file/copy-file bindings
are accepted but ignored. Explicit empty shortcut arrays remain disabled.
Native text editing (copy/paste, undo, cursor movement) and OS dialogs keep their
platform bindings. Interface zoom shortcuts are configurable too.

Files can contain only the options you want to change. Shortcut arrays replace
the defaults for that action; `[]` disables it. For example:

```toml
[keybindings]
save = ["Primary+Shift+S"]
go = []

[mouse]
go = "Primary+DoubleClick"

[build]
engine = "xelatex"

[editor]
font_size = 17
line_numbers = true

[theme]
mode = "light"
```

`[theme] mode` selects the built-in `dark` (default) or `light` palette; `system`
follows the desktop appearance, also while Folio is running. The palette covers
the editor, syntax colors, file browser, menus, search and Git marks. PDF pages
keep their own colors.

Keyboard shortcuts match the exact modifiers. A shortcut cannot be assigned to
two actions that are active in the same context. An empty mouse `go = ""` disables
the navigation gesture. Unknown keys, invalid values and unreadable/malformed
files produce an error identifying the setting and file; the app opens with
defaults so the file can be corrected. A broken adjacent file does not silently
fall through to the home configuration. Restart Folio to apply changes. Open the
command palette and choose **Keyboard shortcuts** to see active bindings and the
loaded settings path.

## Workflow

One window shows one focused surface: a source file or the PDF. A thin caption
shows the project and file path, marks unsaved edits, and shows PDF page position.
There are no permanent action buttons, toolbars, tab strips or status bars. The
caption can move the window; macOS keeps its native window controls in the same
surface. Linux keeps its window manager decorations. Commands and source/PDF
switching use a searchable overlay; right-click opens Commands without a shortcut.
Native typography, a charcoal or light palette and shared spacing keep the editor, browser,
search and popovers consistent. Fonts load once from common platform locations
with bundled fallbacks. No webview, blur, widget transitions or blinking caret
is needed. File/command results are virtualized and refilter only when the query
changes. Line numbers and Git marks appear in the source gutter. Line numbers are enabled
by default; `[editor] line_numbers = false` hides them if desired.
Long source lines wrap at the window width; a wrapped line keeps one line number.

A buffer without unsaved edits follows changes made to its file outside Folio,
such as `git pull` or another editor. A buffer with unsaved edits is never
replaced: saving reports the conflict, and **Reload** discards your edits.
About a second after you stop typing, Folio keeps a copy of each unsaved buffer
in `.folio/recovery/`. If Folio or the computer stops before you save, opening
the file restores those edits as unsaved changes. A copy is removed when its
buffer is saved or deliberately discarded. If the file itself changed in the
meantime, the copy is not applied; it is kept beside the others with an
`.orphaned` suffix and Folio names its path.

**Cmd/Ctrl+F** opens search. **Enter** moves forward and **Shift+Enter** moves
backward while the search field keeps focus; **Escape** dismisses it. These map
to `find_next`, `find_previous`, and `dismiss` in the external settings.

Move through prose with two-key shortcuts while editing:

| Shortcut (macOS / Linux) | Destination |
| --- | --- |
| Cmd/Ctrl+D | Current paragraph end; repeat for the next |
| Cmd/Ctrl+U | Current paragraph start; repeat for the previous |
| Cmd/Ctrl+J | Current sentence end; repeat for the next |
| Cmd/Ctrl+K | Current sentence start; repeat for the previous |

These collapse a selection without changing text. Visible destinations keep the
viewport steady; offscreen destinations scroll into view. Blank lines and LaTeX
`\par` separate paragraphs; a single soft line break does not. Sentence motions
include closing quotes/brackets, skip punctuation in LaTeX comments, math and
verbatim, and recognize common abbreviations and decimal numbers. Sentence
detection uses punctuation heuristics, not TeX expansion or language analysis.
The four settings are `paragraph_down`, `paragraph_up`, `sentence_next`, and
`sentence_previous`. Commands also lists them. They only consume shortcuts
while the source editor has focus; search, launchers and PDF retain their input.

**Cmd/Ctrl+P** opens a searchable switcher for open buffers, project files and the
PDF. **Ctrl+Tab / Ctrl+Shift+Tab** cycle through open source files and PDF, retaining
each buffer's selection and scroll position. **Cmd/Ctrl+1** returns to the last
source file; **Cmd/Ctrl+2** opens the PDF. Switching surfaces does not perform a
SyncTeX jump.

Entering PDF saves and typesets stale content with pdfLaTeX, XeLaTeX or LuaLaTeX.
An unchanged preview reuses its cached pages and scroll position. Edits invalidate
it; the next switch rebuilds it. Changes to project files on disk also refresh the
PDF while Preview is open.

Hold **Cmd and double-click the left mouse button** on a PDF word to jump to and
select that exact word in its source file. On Linux, hold **Ctrl** instead.
An ordinary PDF click selects the word without switching views; Cmd/Ctrl+G goes
from the current selection. Word matching uses nearby context to distinguish
repeated words and handles PDF ligatures and line-end hyphenation.
Source-to-PDF navigation retains the source selection and uses SyncTeX's region.
Generated or ambiguous output falls back to a caret on the mapped source line;
arbitrary TeX macro expansions cannot always be matched to a literal source word.

Completion appears at the caret while typing commands, environments, references
or citations. Tab/Down cycles forward, Shift+Tab/Up cycles backward, and Enter
accepts. Citation suggestions
index project `.bib` files and declared BibTeX/BibLaTeX resources, showing author,
title and year. Unsaved open bibliography buffers are included. `latexmk` handles
the actual bibliography compilation.

**Cmd/Ctrl+Shift+P**, or a right-click, opens the command palette. Search for project commands,
Recompile, compiler output, Reload from disk, PDF controls or shortcut help. **Cmd/Ctrl+Shift+O** opens the section
outline. Secondary panels appear on demand; Escape dismisses them. The project manifest
always determines the main file, including when editing an included section.

Builds run in the background and can be cancelled. `build.timeout_seconds`
defaults to 120; increase it for long builds or use 0 to disable the time limit.
Cancellation gives the compiler process group a termination grace period.
Interrupted auxiliary files are cleaned before the next attempt.
Failures preserve the last successful PDF and its SyncTeX map; error messages
link back to source. Failed revisions do not repeatedly rebuild in the background.
PDFs without SyncTeX still open; source navigation explains when a map is missing.
Both compressed and uncompressed maps are supported.

The PDF layer offers zoom, width/page fitting, page navigation, indexed search
and drag selection/copy. Switching from source fits the PDF to **90% of the
available canvas width**, after the browser, caption, search and compiler panels
take their space. The page is centered horizontally and tall pages start at the
top. Fitting adapts to window and panel resizing. Manual zoom stays in effect
until the next source-to-PDF switch; recompiling in PDF keeps it.

In PDF view, **Space** immediately restores width fitting. **Double-Space** (two
taps within 350 ms) fits the whole current page. Holding Space does not count as
a double tap, and intervening commands, scrolling or focus changes break the
sequence. Source text, search fields and launchers keep ordinary space input.
Returning from whole-page view to width shows that page's beginning (or its
active search/SyncTeX target); restoring width after manual zoom keeps position.
These commands also appear in Commands. The settings are:

```toml
[keybindings]
pdf_fit = ["Space"]
pdf_fit_page = ["Space Space"]

[preview]
fit_on_switch = "width" # width, page, keep
width_fraction = 0.9    # 0.1 through 1.0
```

Both fitting commands accept a single chord or a repeated chord such as `F7 F7`;
`[]` disables the binding. Whole-page fitting also respects the width fraction
and limits the height, so it never enlarges a width-fitted page. SyncTeX retains
its highlight and centers the text target in width/manual mode, or shows its
whole page in page mode. `"keep"` retains the previous zoom; `fit_width` and
`zoom` then control the initial view. No extra toolbar is added.

Pixel scrolling is applied before painting, preserving the OS's
trackpad momentum without adding another smoothing tail. Pinch or Ctrl+wheel
zooms around the pointer; palette zoom commands preserve the center of the view.
SyncTeX jumps are immediate. Periodic project scans, including bibliography parsing, run in the background,
and the GPU presentation queue requests one frame of latency. While scrolling or zooming, it reuses cached pixels and requests
quick drafts for newly visible pages. After 120 ms without movement, visible
pages sharpen to display density. Neighboring drafts are prefetched, obsolete
render requests are replaced, and pixel conversion runs on the worker. The UI
uploads at most one page per frame, with a 96 MiB texture cache and two completed
images queued. Resolution is capped to fit the configured budget across resident
pages and the GPU texture limit; old resolutions are replaced. A failed render
does not block other pages. Only visible pages get UI widgets; text indexing remains separate.
Search spans whitespace and hyphenated line wraps while preserving selection geometry.

Completion reuses per-file indexes and only reparses edited buffers. Initial,
save-triggered and periodic project scans all run in the background. Source line
counts are updated on edits, with line numbers enabled by default.

## Git changes

Open a file inside a Git repository. The gutter compares its current text,
including unsaved edits, against **HEAD** by default. Open **Git comparison** with
**Cmd/Ctrl+Shift+D** or the command palette. Choose a recent commit, or
enter a commit, tag or expression such as `HEAD~1`. Green gutter bars mark added
lines, blue bars mark modifications, and red ticks mark deletion boundaries.
Click a marker to see old/new lines; use **Next change** / **Previous change** in
the comparison panel to navigate. Adjacent edits may form one hunk, as in `git diff`.

Git runs in the background after a short typing pause. The integration is
read-only: it does not stage, commit, checkout or modify your repository. New
files and repositories without commits show additions. Files outside a repository
remain editable; the comparison panel explains that no repository was found.

| Shortcut | Action |
| --- | --- |
| Cmd/Ctrl + P | Switch file or PDF |
| Cmd/Ctrl + Shift + P | Search commands |
| Ctrl + Tab / Ctrl + Shift + Tab | Next / previous surface |
| Cmd/Ctrl + 1 / Cmd/Ctrl + 2 | Source / PDF |
| Cmd/Ctrl + left double-click | Go to the selection in the other layer |
| Cmd/Ctrl + O | Open a project |
| Cmd/Ctrl + N | Create a project |
| Cmd/Ctrl + Shift + N | Add a project file |
| Cmd/Ctrl + B | Toggle project files |
| Cmd/Ctrl + S | Save open files |
| Cmd/Ctrl + Enter | Recompile and show Preview |
| Cmd/Ctrl + F | Find in the current layer |
| Cmd/Ctrl + Shift + O | Source outline |
| Cmd/Ctrl + Shift + D | Git comparison |
| Cmd/Ctrl + Shift + L | Toggle compiler output |
| PageDown / PageUp | Next / previous PDF page |
| Escape | Dismiss launcher, search or secondary panels |
| Ctrl + Space | Open contextual completion |
| Tab / Shift + Tab | Next / previous completion suggestion (also Down / Up) |
| Enter | Accept the selected completion suggestion |
| Cmd/Ctrl + Z | Undo in the editor |
