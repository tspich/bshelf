/// All user-facing key bindings, used both for the help screen and
/// for generating the mode-indicator colour in the status bar.
///
/// Add new bindings here so they automatically appear in the help overlay.

use ratatui::style::Color;
use crate::app::Mode;

// ---------------------------------------------------------------------------
// Mode colour
// ---------------------------------------------------------------------------

/// Returns the status-bar background colour for a given mode.
pub fn mode_color(mode: &Mode) -> Color {
    match mode {
        Mode::Normal                                 => Color::Green,
        Mode::Search                                 => Color::Yellow,
        Mode::ConfirmDelete | Mode::ConfirmRemoveRef => Color::Red,
        _                                            => Color::Blue,
    }
}

// ---------------------------------------------------------------------------
// Help text
// ---------------------------------------------------------------------------

/// Returns the lines shown in the Help overlay (Mode::Help).
/// Each `&'static str` is one line; empty strings become blank spacer lines.
pub fn help_lines() -> &'static [&'static str] {
    &[
        "  Press Esc, q or H to close   j/k to scroll",
        "",
        "  NAVIGATION",
        "  ──────────────────────────────────────",
        "  h / ←        Previous project",
        "  l / →        Next project",
        "  j / ↓        Next reference",
        "  k / ↑        Previous reference",
        "  d / u        Scroll details panel down / up",
        "  g            Jump to first reference",
        "  G            Jump to last reference",
        "",
        "  ACTIONS",
        "  ──────────────────────────────────────",
        "  A             Add reference by DOI",
        "  B             Export project to .bib",
        "  I             Import .bib file",
        "  M             Copy reference to project",
        "  N             Create new project",
        "  R             Rename current project",
        "  D             Delete reference from project",
        "  e             Edit reference in $EDITOR",
        "  F             Re-fetch missing metadata from Crossref",
        "  P             Import PDF and link to reference",
        "  c             Copy the current key to the clipboard",
        "  X             Delete current project",
        "  Enter         Open PDF (if available)",
        "",
        "  SEARCH",
        "  ──────────────────────────────────────",
        "  /             Enter search mode",
        "  Enter         Apply search",
        "  Esc           Clear search / cancel",
        "",
        "  OTHER",
        "  ──────────────────────────────────────",
        "  H             Toggle this help screen",
        "  q             Quit",
    ]
}

