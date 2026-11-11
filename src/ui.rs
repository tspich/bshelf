use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect, Alignment},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Clear, Wrap},
};

use bshelf::{
    authors_to_string,
    chunks_to_string,
    date_to_year_string,
    has_supp_files,
    list_supp_files,
    pdf_path_for_doi,
    publisher_string,
    supp_dir_for_doi,
};

use crate::app::{App, ImportKind, Mode, mode_name};
use crate::keybindings::{help_lines, mode_color};

/// Nerd Font "file-pdf" glyph (U+F1C1), the one yazi shows for PDFs.
/// Needs a Nerd Font in the terminal, as yazi does.
const PDF_ICON: &str = "\u{f1c1}";
const SI_MARK: &str = "[SI]";

/// Cut `s` to at most `max` chars, ending in "…" when shortened.
fn truncate_to(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else if max == 0 {
        String::new()
    } else {
        let mut t: String = s.chars().take(max - 1).collect();
        t.push('…');
        t
    }
}

/// Greedily pack hint items onto lines no wider than `width`, separated by
/// three spaces, so a narrow window wraps the hints instead of cutting them.
fn pack_hints(items: &[&str], width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for item in items {
        let candidate = if cur.is_empty() {
            format!(" {item}")
        } else {
            format!("{cur}   {item}")
        };
        if candidate.chars().count() > width && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
            cur = format!(" {item}");
        } else {
            cur = candidate;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Draw the entire TUI for one frame.
pub fn draw(f: &mut Frame, app: &mut App) {
    // Vertical layout: panels | search box | status bar
    let vchunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ].as_ref())
        .split(f.size());

    // top row: three horizontal panels
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(30),
            Constraint::Percentage(50),
        ])
        .split(vchunks[0]);

    // ── Sync scroll positions ───────────────────────────────────────────────
    app.sync_ref_scroll(panels[1].height as usize);
    app.sync_project_scroll(panels[0].height as usize);

    // ── Left panel: projects ────────────────────────────────────────────────
    let visible_projects: Vec<ListItem> = app
        .projects
        .iter()
        .enumerate()
        .skip(app.project_scroll)
        .take(panels[0].height.saturating_sub(2) as usize)
        .map(|(i, p)| {
            let style = if i == app.selected_project {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Span::styled(p, style))
        })
        .collect();

    let project_list = List::new(visible_projects)
        .block(Block::default().title("Projects").borders(Borders::ALL));
    f.render_widget(project_list, panels[0]);

    // ── Middle panel: references ───────────────────────────────────────────────
    let refs_to_show = if !app.search_query.is_empty() {
        app.filtered_refs.clone()
    } else {
        app.references.clone()
    };

    let ref_inner_w = panels[1].width.saturating_sub(2) as usize;
    let visible_refs: Vec<ListItem> = refs_to_show
        .iter()
        .enumerate()
        .skip(app.ref_scroll)
        .take(panels[1].height.saturating_sub(2) as usize)
        .map(|(i, r)| {
            let key = r.key.to_string();
            let style = if i == app.selected_reference {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };
            // Only visible rows get here, so this is a handful of
            // stat/read_dir calls per frame.
            let doi = r.doi().ok().unwrap_or_default();
            let has_pdf = !doi.is_empty() && pdf_path_for_doi(&app.config.pdfs_dir, &doi).exists();
            let has_si = supp_dir_for_doi(&app.config.pdfs_dir, &doi)
                .map(|d| has_supp_files(&d))
                .unwrap_or(false);
            // Fixed right-hand columns, so markers line up row to row:
            // "[SI] " then the PDF icon. Key is truncated to make room.
            let markers_w = SI_MARK.len() + 1 + 1;
            let key_room = ref_inner_w.saturating_sub(markers_w + 1);
            let key = truncate_to(&key, key_room);
            let pad = ref_inner_w.saturating_sub(key.chars().count() + markers_w);
            // DIM fades the row's own colour instead of picking a fixed grey:
            // "dark gray" is the background colour in many dark themes.
            let si_style = style.add_modifier(Modifier::DIM);
            ListItem::new(Line::from(vec![
                Span::styled(key, style),
                Span::raw(" ".repeat(pad)),
                Span::styled(if has_si { SI_MARK } else { "    " }, si_style),
                Span::raw(" "),
                if has_pdf {
                    Span::styled(PDF_ICON, Style::default().fg(Color::Red))
                } else {
                    Span::raw(" ")
                },
            ]))
        })
        .collect();

    let ref_list = List::new(visible_refs)
        .block(Block::default().title("References").borders(Borders::ALL));
    f.render_stateful_widget(ref_list, panels[1], &mut app.list_state);

    // ── Right panel: details ────────────────────────────────────────────────
    let details = if !refs_to_show.is_empty() && app.selected_reference < refs_to_show.len() {
        let r = &refs_to_show[app.selected_reference];
        let doi = r.doi().ok().unwrap_or_default();
        let pdf_line = if !doi.is_empty() && pdf_path_for_doi(&app.config.pdfs_dir, &doi).exists() {
            "available (Enter to open)"
        } else {
            "none"
        };
        let supp_count = supp_dir_for_doi(&app.config.pdfs_dir, &doi)
            .map(|d| list_supp_files(&d).len())
            .unwrap_or(0);
        let supp_line = match supp_count {
            0 => "none".to_string(),
            1 => "1 file (S to show)".to_string(),
            n => format!("{n} files (S to show)"),
        };
        format!(
            "Title:\n{}\nAuthors:\n{}\nYear: {}\nJournal: {}\nDOI: {}\nPublisher: {}\nPDF: {}\nSupplementary: {}\nAbstract:\n {}",
            r.title().ok().map(chunks_to_string).unwrap_or_else(|| "<no title>".to_string()),
            r.author().ok().map(authors_to_string).unwrap_or_else(|| "no authors".to_string()),
            r.date().ok().and_then(date_to_year_string).unwrap_or_else(|| "<no year>".to_string()),
            r.journal().ok().map(chunks_to_string).unwrap_or_else(|| "<no journal>".to_string()),
            r.url().ok().as_deref().unwrap_or(""),
            r.publisher().ok().map(publisher_string).unwrap_or_else(|| "<no issn>".to_string()),
            pdf_line,
            supp_line,
            r.abstract_().ok().map(chunks_to_string).unwrap_or_else(|| "<no abstract>".to_string()),
        )
    } else {
        "No reference selected.".to_string()
    };

    f.render_widget(Clear, Block::default().borders(Borders::ALL).inner(panels[2]));

    let ref_para = Paragraph::new(details)
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll as u16, 0))
        .block(Block::default().title("Details").borders(Borders::ALL));
    f.render_widget(ref_para, panels[2]);

    // ── Bottom: search box ──────────────────────────────────────────────────
    let search_text = if matches!(app.mode, Mode::Search) {
        format!("/{}", app.search_query)
    } else if !app.search_query.is_empty() {
        format!("Filter: {}", app.search_query)
    } else {
        String::from("Press / to search")
    };
    let search_para = Paragraph::new(search_text)
        .block(Block::default().title("Search").borders(Borders::ALL));
    f.render_widget(search_para, vchunks[1]);

    // ── Status bar ──────────────────────────────────────────────────────────
    draw_status_bar(f, app, vchunks[2]);

    // ── Alert overlay ───────────────────────────────────────────────────────
    if let Some(msg) = &app.alert_message.clone() {
        draw_alert(f, msg);
    }

    // ── Mode overlays ───────────────────────────────────────────────────────
    if matches!(app.mode, Mode::NewProject) {
        draw_input_popup(f, "New Project Name", &app.new_project_name, Color::White);
    }

    if matches!(app.mode, Mode::Adding) {
        draw_input_popup(f, "New reference DOI", &app.new_ref, Color::White);
    }

    if matches!(app.mode, Mode::RenameProject) {
        draw_rename_popup(f, &app.rename_project_name);
    }

    if matches!(app.mode, Mode::Moving) {
        draw_moving_popup(f, app);
    }

    if matches!(app.mode, Mode::ConfirmDelete) {
        draw_confirm_delete(f, app);
    }

    if matches!(app.mode, Mode::ConfirmRemoveRef) {
        draw_confirm_remove_ref(f, app);
    }

    if matches!(app.mode, Mode::ConfirmReplacePdf) {
        draw_confirm_replace_pdf(f, app);
    }

    // Drawn before the file browser: while adding files the popup stays
    // visible underneath it.
    if matches!(app.mode, Mode::Supplementary)
        || (matches!(app.mode, Mode::FileBrowser) && app.supp_view.is_some())
    {
        draw_supp_popup(f, app);
    }

    if matches!(app.mode, Mode::FileBrowser) {
        draw_file_browser(f, app);
    }

    if matches!(app.mode, Mode::Help) {
        draw_help(f, app);
    }

    if matches!(app.mode, Mode::PdfDoi) {
        draw_pdf_doi_popup(f, app);
    }

    if matches!(app.mode, Mode::ImportProject) {
        draw_import_project_popup(f, app);
    }
    if matches!(app.mode, Mode::ImportNewProject) {
        draw_input_popup(f, " New project name ", &app.import_new_project_name, Color::White);
    }
}

// ---------------------------------------------------------------------------
// Helper: status bar
// ---------------------------------------------------------------------------

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let current_project = &app.projects[app.selected_project];
    let active_refs = if !app.filtered_refs.is_empty() {
        &app.filtered_refs
    } else {
        &app.references
    };

    let mode_str   = format!(" {} ", mode_name(&app.mode));
    let project_str = format!(" 📁 {}", current_project);
    let count_str  = format!("{} refs ", active_refs.len());

    let left    = format!("{}  {}", mode_str, project_str);
    let padding = area.width
        .saturating_sub(left.len() as u16)
        .saturating_sub(count_str.len() as u16);
    let full_line = format!("{}{}{}", left, " ".repeat(padding as usize), count_str);

    let status = Paragraph::new(full_line)
        .style(Style::default().fg(Color::Black).bg(mode_color(&app.mode)));
    f.render_widget(status, area);
}

// ---------------------------------------------------------------------------
// Helper: centred alert banner
// ---------------------------------------------------------------------------

fn draw_alert(f: &mut Frame, msg: &str) {
    let size = f.size();
    let alert_height = 3;
    let alert_width  = msg.len() as u16 + 4;
    let x = (size.width.saturating_sub(alert_width)) / 2;
    let y = size.height.saturating_sub(alert_height) - 1;
    let area = Rect { x, y, width: alert_width, height: alert_height };

    let paragraph = Paragraph::new(Span::styled(
        msg,
        Style::default().fg(Color::White).bg(Color::Red),
    ))
    .block(Block::default().borders(Borders::ALL))
    .alignment(Alignment::Center);

    f.render_widget(paragraph, area);
}

// ---------------------------------------------------------------------------
// Helper: generic centred input popup
// ---------------------------------------------------------------------------

fn draw_input_popup(f: &mut Frame, title: &str, value: &str, _border_color: Color) {
    let size = f.size();
    let area = Rect {
        x: size.width / 4,
        y: size.height / 2 - 2,
        width: size.width / 2,
        height: 3,
    };
    f.render_widget(Clear, area);
    let input = Paragraph::new(value)
        .block(Block::default().title(title).borders(Borders::ALL));
    f.render_widget(input, area);
}

// ---------------------------------------------------------------------------
// Helper: rename project popup
// ---------------------------------------------------------------------------

fn draw_rename_popup(f: &mut Frame, name: &str) {
    let size = f.size();
    let area = Rect {
        x: size.width / 4,
        y: size.height / 2 - 2,
        width: size.width / 2,
        height: 3,
    };
    f.render_widget(Clear, area);
    let input = Paragraph::new(name)
        .block(
            Block::default()
                .title(" Rename project ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        );
    f.render_widget(input, area);
}

// ---------------------------------------------------------------------------
// Helper: moving-reference popup
// ---------------------------------------------------------------------------

fn draw_moving_popup(f: &mut Frame, app: &App) {
    let targets: Vec<String> = app.projects.iter()
        .filter(|p| p.as_str() != "all")
        .cloned()
        .collect();

    let items: Vec<ListItem> = targets.iter().enumerate().map(|(i, p)| {
        let style = if i == app.moving_target {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        ListItem::new(p.as_str()).style(style)
    }).collect();

    let size = f.size();
    let height = (targets.len() as u16 + 2).min(size.height / 2);
    let area = Rect {
        x: size.width / 4,
        y: size.height / 2 - height / 2,
        width: size.width / 2,
        height,
    };

    let block = Block::default()
        .title(" Copy to project (↑↓ navigate, Enter confirm, Esc cancel) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    f.render_widget(Clear, area);
    f.render_widget(List::new(items).block(block), area);
}

// ---------------------------------------------------------------------------
// Helper: confirm-delete popup
// ---------------------------------------------------------------------------

fn draw_confirm_delete(f: &mut Frame, app: &App) {
    let current = &app.projects[app.selected_project];
    let size = f.size();
    let area = Rect {
        x: size.width / 4,
        y: size.height / 2 - 2,
        width: size.width / 2,
        height: 3,
    };
    f.render_widget(Clear, area);
    let msg = format!("Delete project '{}'? (y/n)", current);
    let confirm = Paragraph::new(msg)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(" Confirm ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red)),
        );
    f.render_widget(confirm, area);
}

// ---------------------------------------------------------------------------
// Helper: confirm-remove-ref popup
// ---------------------------------------------------------------------------

fn draw_confirm_replace_pdf(f: &mut Frame, app: &App) {
    let (key, source) = match &app.pending_replace_pdf {
        Some((k, p)) => (k.as_str(), p.as_path()),
        None => return,
    };
    let source_name = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?");

    let msg = format!(
        "PDF already linked for '{}'. Overwrite with '{}'? (y/n)",
        key, source_name
    );

    let size = f.size();
    let width = ((size.width * 2) / 3).max(20);
    let inner_w = width.saturating_sub(2).max(1) as usize;
    // Borders (2) + however many wrapped lines the message needs.
    let needed_lines = msg.chars().count().div_ceil(inner_w) as u16;
    let max_h = size.height.saturating_sub(4).max(3);
    let height = (needed_lines + 2).clamp(3, max_h);

    let area = Rect {
        x: (size.width.saturating_sub(width)) / 2,
        y: size.height.saturating_sub(height) / 2,
        width,
        height,
    };
    f.render_widget(Clear, area);
    let confirm = Paragraph::new(msg)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .title(" Confirm replace ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red)),
        );
    f.render_widget(confirm, area);
}

fn draw_confirm_remove_ref(f: &mut Frame, app: &App) {
    let active_refs = if !app.filtered_refs.is_empty() {
        &app.filtered_refs
    } else {
        &app.references
    };

    let key = active_refs
        .get(app.selected_reference)
        .map(|e| e.key.as_str())
        .unwrap_or("?");

    let size = f.size();
    let area = Rect {
        x: size.width / 4,
        y: size.height / 2 - 2,
        width: size.width / 2,
        height: 3,
    };
    f.render_widget(Clear, area);
    let msg = format!("Remove '{}' from project? (y/n)", key);
    let confirm = Paragraph::new(msg)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(" Confirm ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red)),
        );
    f.render_widget(confirm, area);
}

// ---------------------------------------------------------------------------
// Helper: file browser overlay
// ---------------------------------------------------------------------------

fn draw_file_browser(f: &mut Frame, app: &App) {
    let fb = match &app.file_browser {
        Some(fb) => fb,
        None => return,
    };

    let size = f.size();
    let area = Rect {
        x: size.width / 6,
        y: size.height / 8,
        width: size.width * 2 / 3,
        height: size.height * 3 / 4,
    };

    // Keep the end of the path (the part that changes). Counted in chars, not
    // bytes: byte slicing panics on non-ASCII paths. Budget: borders (2) plus
    // " 📂 " and the trailing space (5 cells, the emoji is double-width).
    let dir_str   = fb.current_dir.to_string_lossy();
    let max_title = area.width.saturating_sub(7) as usize;
    let n_chars   = dir_str.chars().count();
    let title = if n_chars > max_title {
        let tail: String = dir_str.chars().skip(n_chars + 1 - max_title.max(1)).collect();
        format!("…{tail}")
    } else {
        dir_str.to_string()
    };

    // Hints wrap to the window width; the file list gets what is left.
    let count = fb.multi_selected.len();
    let import_n = format!("Enter: import {count} selected");
    let hint_items: Vec<&str> = if fb.filtering {
        vec!["Enter: apply", "Esc: cancel filter"]
    } else if count > 0 {
        vec!["Space: toggle", &import_n, "Esc: close"]
    } else {
        vec![
            "Enter: open/select", "j/k: navigate", "Space: select", "/: filter",
            "~: home", "D: Downloads", "Esc: close",
        ]
    };
    let hint_lines = pack_hints(&hint_items, area.width.saturating_sub(2) as usize);
    // Borders (2) + search line (1) + hints, but always leave one list row.
    let hint_h = (hint_lines.len() as u16).min(area.height.saturating_sub(4));

    let inner_height  = area.height.saturating_sub(3 + hint_h).max(1) as usize;
    let visible       = fb.visible_entries();
    let scroll_offset = if fb.selected >= inner_height { fb.selected - inner_height + 1 } else { 0 };

    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(inner_height)
        .map(|(i, path)| {
            let is_dir   = path.is_dir();
            let is_multi = fb.multi_selected.contains(*path);

            let name = if path.ends_with("..") {
                "  ..".to_string()
            } else if is_dir {
                format!("  {}/", path.file_name().unwrap_or_default().to_string_lossy())
            } else {
                format!(
                    " {}  {}",
                    if is_multi { "●" } else { " " },
                    path.file_name().unwrap_or_default().to_string_lossy()
                )
            };

            let style = if i == fb.selected {
                if is_dir        { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) }
                else if is_multi { Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD) }
                else             { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) }
            } else if is_multi { Style::default().fg(Color::Green)
            } else if is_dir   { Style::default().fg(Color::Blue)
            } else             { Style::default() };

            ListItem::new(name).style(style)
        })
        .collect();

    let block = Block::default()
        .title(format!(" 📂 {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    f.render_widget(Clear, area);
    f.render_widget(List::new(items).block(block), area);

    // Search bar
    let search_area = Rect {
        x: area.x + 1,
        y: (area.y + area.height).saturating_sub(2 + hint_h),
        width: area.width.saturating_sub(2),
        height: 1,
    };
    let search_text = if fb.filtering {
        format!(" 🔍 {}_", fb.filter)
    } else if !fb.filter.is_empty() {
        format!(" 🔍 {} (Esc to clear)", fb.filter)
    } else {
        " Press / to filter".to_string()
    };
    let search_style = if fb.filtering || fb.filter.is_empty() {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Yellow)
    };
    f.render_widget(Paragraph::new(search_text).style(search_style), search_area);

    draw_hint_lines(f, area, &hint_lines, hint_h);
}

/// Render pre-packed hint lines on the bottom inner rows of a bordered `area`.
fn draw_hint_lines(f: &mut Frame, area: Rect, lines: &[String], height: u16) {
    if height == 0 {
        return;
    }
    let hint_area = Rect {
        x: area.x + 1,
        y: (area.y + area.height).saturating_sub(1 + height),
        width: area.width.saturating_sub(2),
        height,
    };
    f.render_widget(Clear, hint_area);
    f.render_widget(
        Paragraph::new(lines.join("\n")).style(Style::default().fg(Color::Green)),
        hint_area,
    );
}

// ---------------------------------------------------------------------------
// Helper: supplementary files popup
// ---------------------------------------------------------------------------

fn draw_supp_popup(f: &mut Frame, app: &App) {
    let v = match &app.supp_view {
        Some(v) => v,
        None => return,
    };

    let size = f.size();
    let area = Rect {
        x: size.width / 6,
        y: size.height / 8,
        width: size.width * 2 / 3,
        height: size.height * 3 / 4,
    };

    let hint_lines = pack_hints(
        &["Enter: open", "j/k: navigate", "a: add files", "o: open folder", "y: copy path", "Esc: close"],
        area.width.saturating_sub(2) as usize,
    );
    // Borders (2) + hints, but always leave one list row.
    let hint_h = (hint_lines.len() as u16).min(area.height.saturating_sub(3));
    let inner_height  = area.height.saturating_sub(2 + hint_h).max(1) as usize;
    let scroll_offset = if v.selected >= inner_height { v.selected - inner_height + 1 } else { 0 };

    let items: Vec<ListItem> = if v.files.is_empty() {
        vec![ListItem::new("  No supplementary files yet — press a to add some")
            .style(Style::default().add_modifier(Modifier::DIM))]
    } else {
        v.files
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(inner_height)
            .map(|(i, path)| {
                // Relative to the folder, so files from unpacked archives
                // keep their subfolder as context.
                let name = path.strip_prefix(&v.dir).unwrap_or(path).to_string_lossy().to_string();
                let style = if i == v.selected {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(format!("  {name}")).style(style)
            })
            .collect()
    };

    let block = Block::default()
        .title(format!(" Supplementary · {} ({}) ", v.key, v.files.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    f.render_widget(Clear, area);
    f.render_widget(List::new(items).block(block), area);

    draw_hint_lines(f, area, &hint_lines, hint_h);
}

// ---------------------------------------------------------------------------
// Helper: help overlay
// ---------------------------------------------------------------------------

fn draw_help(f: &mut Frame, app: &App) {
    let size = f.size();
    let area = Rect {
        x: size.width / 6,
        y: size.height / 8,
        width: size.width * 2 / 3,
        height: size.height * 3 / 4,
    };

    let lines      = help_lines();
    let text       = lines.join("\n");
    let total_lines = lines.len() as u16;
    let visible_lines = area.height.saturating_sub(2);
    let max_scroll = total_lines.saturating_sub(visible_lines) as usize;
    let scroll     = app.help_scroll.min(max_scroll);

    let title = if total_lines > visible_lines {
        format!(" Help ({}/{}) ", scroll + 1, max_scroll + 1)
    } else {
        " Help ".to_string()
    };

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .style(Style::default().fg(Color::White))
        .scroll((scroll as u16, 0));

    f.render_widget(Clear, area);
    f.render_widget(paragraph, area);
}

// ---------------------------------------------------------------------------
// Helper: PDF DOI entry popup
// ---------------------------------------------------------------------------

fn draw_pdf_doi_popup(f: &mut Frame, app: &App) {
    let size = f.size();
    let area = Rect {
        x: size.width / 4,
        y: size.height / 2 - 3,
        width: size.width / 2,
        height: 5,
    };
    f.render_widget(Clear, area);

    let filename = app.pending_pdf_path
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let text = format!("PDF: {}\nDOI: {}", filename, app.pdf_doi_input);
    let input = Paragraph::new(text)
        .block(
            Block::default()
                .title(" No DOI found — enter manually ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        );
    f.render_widget(input, area);
}


// ---------------------------------------------------------------------------
// Helper: Import bib file to project
// ---------------------------------------------------------------------------

fn draw_import_project_popup(f: &mut Frame, app: &App) {
    // Build the picker list: existing projects (minus "all") + two special entries
    let mut options: Vec<String> = app.projects.iter()
        .filter(|p| p.as_str() != "all")
        .cloned()
        .collect();
    options.push("[ New project… ]".to_string());
    options.push("[ No project — add to all only ]".to_string());

    let items: Vec<ListItem> = options.iter().enumerate().map(|(i, p)| {
        let style = if i == app.import_project_target {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        ListItem::new(p.as_str()).style(style)
    }).collect();

    let size = f.size();
    let height = (options.len() as u16 + 2).min(size.height / 2);
    let area = Rect {
        x: size.width / 4,
        y: size.height / 2 - height / 2,
        width: size.width / 2,
        height,
    };

    let n = app.pending_import_paths.len();
    let what = match app.pending_import_kind {
        ImportKind::Bib     => "file",
        ImportKind::DoiList => "DOI list",
    };
    let title = format!(
        " Import {} {}{} to… (↑↓ navigate, Enter confirm, Esc cancel) ",
        n, what, if n == 1 { "" } else { "s" }
    );

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    f.render_widget(Clear, area);
    f.render_widget(List::new(items).block(block), area);
}
