![Rust](https://img.shields.io/badge/rust-stable-orange)
![License](https://img.shields.io/badge/license-MIT-blue)

# bshelf

Let reorganize your bookshelf!

A terminal-based reference manager built with 
[Ratatui](https://github.com/ratatui-org/ratatui). 
Manage your BibLaTeX library, organize references into projects, fetch metadata
from Crossref, and open PDFs — all without leaving the terminal.

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

---

## Installation

Not deployed so far

```bash
git clone https://github.com/tspich/bshelf
cd bshelf
cargo install --path .
```
The binary will be at `~/.cargo/bshelf`.

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

## Keybindings

### Navigation

| Key | Action |
|---|---|
| `h` / `←` | Previous project |
| `l` / `→` | Next project |
| `j` / `↓` | Next reference |
| `k` / `↑` | Previous reference |
| `d` / `u` | Scroll details panel down / up |
| `g`       | Jump to first reference |
| `G`       | Jump to last reference |

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

# Limitations and TODOs

- For now only `nvim` as editor.
- Keybindings are hard coded, should be configurable through the config file.
- While importing from `.bib` file, keys are taken over. Can be problematic to reuse those in LaTex.
