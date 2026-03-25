# bshelf

A terminal-based reference manager built with 
[Ratatui](https://github.com/ratatui-org/ratatui). 
Manage your BibLaTeX library, organize references into projects, fetch metadata
from Crossref, and open PDFs — all without leaving the terminal.

![Rust](https://img.shields.io/badge/rust-stable-orange)
![License](https://img.shields.io/badge/license-MIT-blue)

---

## Features

- **Project-based organization** — group references into named projects, all 
backed by a single `all.bib` file
- **DOI-based import** — add a reference by DOI and have metadata fetched 
automatically from Crossref
- **PDF import** — select a local PDF, extract its DOI automatically via 
`pdftotext`, and link it to the correct entry
- **BibLaTeX import/export** — import existing `.bib` files and export any
project to its own `.bib` file
- **Metadata refetching** — fill in missing fields (abstract, journal, volume,
etc.) for any entry from Crossref
- **Inline search** — filter references by title or author with a vim-style `/`
search
- **Open PDFs** — press Enter to open the PDF for the selected reference
- **Edit in `$EDITOR`** — open the raw BibLaTeX entry directly in your editor
- **Copy citation key** — copy the BibLaTeX key to clipboard for use in LaTeX
- **Vim-style keybindings** throughout

---

## Dependencies

### System

| Tool | Purpose |
|---|---|
| `pdftotext` | DOI extraction from PDFs (`poppler-utils` on Debian/Ubuntu, `poppler` on Arch/macOS) |
| `xclip` / `xsel` / `wl-clipboard` | Clipboard support on Linux (X11 or Wayland) |

### Rust

Key crates used: `ratatui`, `crossterm`, `biblatex`, `crossref`, `reqwest`, `arboard`, `serde`, `anyhow`, `dirs`, `toml`, `regex`.

---

## Installation

```bash
git clone https://github.com/yourname/bshelf
cd bshelf
cargo build --release
```

The binary will be at `target/release/bshelf`. You can move it somewhere on your `$PATH`:

```bash
cp target/release/bshelf ~/.local/bin/
```

---

## Configuration

On first launch, bshelf will walk you through creating a config file at:

```
~/.config/bshelf/config.toml
```

You will be prompted for three paths (defaults shown in brackets, press Enter to accept):

```
Path to your all.bib file [~/.local/share/bshelf/all.bib]:
Path to your PDFs directory [~/.local/share/bshelf/pdfs]:
Path to your projects.json file [~/.local/share/bshelf/projects.json]:
```

All files and directories are created automatically if they do not exist.

The resulting config file looks like:

```toml
all_bib       = "~/.local/share/bshelf/all.bib"
pdfs_dir      = "~/.local/share/bshelf/pdfs"
projects_file = "~/.local/share/bshelf/projects.json"
```

---

## Layout

```
┌─ Projects ────┬─ References ──────────┬─ Details ───────────────────────────┐
│ all           │ smith_2023            │ Title:                              │
│ > physics     │ > jones_2024          │ On the Origin of Species            │
│ biology       │ darwin_1859           │ Authors:                            │
│               │                       │ - Darwin Charles                    │
│               │                       │ Year: 1859                          │
│               │                       │ Journal: ...                        │
├───────────────┴───────────────────────┴─────────────────────────────────────┤
│ Press / to search                                                            │
├──────────────────────────────────────────────────────────────────────────────┤
│ NORMAL  📁 physics                                                  3 refs  │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## Keybindings

### Navigation

| Key | Action |
|---|---|
| `h` / `←` | Previous project |
| `l` / `→` | Next project |
| `j` / `↓` | Next reference |
| `k` / `↑` | Previous reference |
| `d` / `u` | Scroll details panel down / up |

### Actions

| Key | Action |
|---|---|
| `A` | Add reference by DOI |
| `c` | Copy citation key to clipboard |
| `D` | Remove reference from project (asks confirmation) |
| `e` | Edit reference in `$EDITOR` |
| `E` | Export current project to `{project}.bib` |
| `F` | Re-fetch missing metadata from Crossref |
| `I` | Import a `.bib` file via file browser |
| `M` | Copy reference to another project |
| `N` | Create new project |
| `P` | Import a PDF and link to reference |
| `R` | Rename current project |
| `X` | Delete current project (asks confirmation) |
| `Enter` | Open PDF for selected reference |

### Search

| Key | Action |
|---|---|
| `/` | Enter search mode |
| `Enter` | Apply search |
| `Esc` | Clear search / cancel |

### Other

| Key | Action |
|---|---|
| `H` | Toggle help screen |
| `q` | Quit |

---

## How it works

### Adding a reference by DOI

Press `A`, type a DOI (e.g. `10.1038/s41586-020-2649-2`), and press Enter. bshelf will:

1. Check for duplicates in `all.bib` by DOI
2. Fetch metadata from [Crossref](https://www.crossref.org/)
3. Attempt to download a PDF via [Unpaywall](https://unpaywall.org/)
4. Generate a citation key (`authorsurname_year`, with `a/b/c` suffixes for duplicates)
5. Add the entry to `all.bib` and to the current project

### Importing a PDF

Press `P` to open the file browser. Select a `.pdf` file and bshelf will:

1. Run `pdftotext` on the first 3 pages to extract a DOI
2. If found, fetch metadata from Crossref and add the entry
3. If not found, prompt you to enter the DOI manually
4. Copy the PDF to your `pdfs_dir` as `{sanitized_DOI}.pdf`

### File browser

Both `I` and `P` open a file browser filtered to the relevant file type (`.bib` or `.pdf`). Navigate with `j`/`k`, open directories with Enter, press `/` to filter by filename, and `Esc` to cancel.

---

## Data format

- **`all.bib`** — a standard BibLaTeX file containing all references across all projects
- **`projects.json`** — a JSON map of project names to lists of citation keys:

```json
{
  "physics": ["smith_2023", "jones_2024"],
  "biology": ["darwin_1859"]
}
```

References are never duplicated in `all.bib` — projects only store keys.

---

## License

MIT
