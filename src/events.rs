use biblatex::Bibliography;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::{fs, io};

use bshelf::{
    add_reference,
    add_to_project,
    delete_project,
    export_project_bib,
    extract_doi_from_pdf,
    find_existing_by_doi,
    import_bib_file,
    link_pdf_to_entry,
    parse_doi_list,
    pdf_dest_for_entry,
    refetch_metadata,
    remove_from_project,
    rename_project
};

use crate::app::{App, FileBrowser, FileBrowserMode, ImportKind, Mode};

// TODO: Should be impossible to create a 'all' project

/// Handle a single key event.  Returns `true` if the app should quit.
pub fn handle_key(
    app: &mut App,
    key: KeyCode,
    modifiers: KeyModifiers,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> bool {
    let ctrl = modifiers.contains(KeyModifiers::CONTROL);
    match key {
        // ── Quit ────────────────────────────────────────────────────────────
        KeyCode::Char('q') if matches!(app.mode, Mode::Normal) => return true,

        // ── Search ──────────────────────────────────────────────────────────
        KeyCode::Char('/') if matches!(app.mode, Mode::Normal) => {
            app.enter_search_mode();
        }
        KeyCode::Char(c) if matches!(app.mode, Mode::Search) => {
            app.search_query.push(c);
            app.apply_search_live();
        }
        KeyCode::Backspace if matches!(app.mode, Mode::Search) => {
            app.search_query.pop();
            app.apply_search_live();
        }
        KeyCode::Esc if matches!(app.mode, Mode::Search) => {
            app.search_query.clear();
            app.clear_filtered_refs();
            app.mode = Mode::Normal;
        }
        KeyCode::Enter if matches!(app.mode, Mode::Search) => {
            // app.apply_search();
            app.mode = Mode::Normal;
        }
        KeyCode::Esc if matches!(app.mode, Mode::Normal) => {
            app.search_query.clear();
            app.clear_filtered_refs();
        }
        // ── Export project to bib ────────────────────────────────────────────
        KeyCode::Char('B') if matches!(app.mode, Mode::Normal) => {
            if let Some(project) = app.projects.get(app.selected_project) {
                if project != "all" {
                    let all_bib_path  = app.config.all_bib.to_string_lossy().to_string();
                    let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                    let output_path   = format!("{}.bib", project);
                    match export_project_bib(&all_bib_path, &proj_map_path, project, &output_path) {
                        Ok(_)  => app.show_alert(&format!("Exported to {output_path}")),
                        Err(e) => app.show_alert(&format!("Export failed: {e}")),
                    }
                } else {
                    app.show_alert("Cannot export 'all' — select a specific project first");
                }
            }
        }

        // ── New project ──────────────────────────────────────────────────────
        KeyCode::Char('N') if matches!(app.mode, Mode::Normal) => {
            app.mode = Mode::NewProject;
            app.new_project_name.clear();
        }
        KeyCode::Char(c) if matches!(app.mode, Mode::NewProject) => {
            app.new_project_name.push(c);
        }
        KeyCode::Backspace if matches!(app.mode, Mode::NewProject) => {
            app.new_project_name.pop();
        }
        KeyCode::Esc if matches!(app.mode, Mode::NewProject) => {
            app.mode = Mode::Normal;
        }
        KeyCode::Enter if matches!(app.mode, Mode::NewProject) => {
            if !app.new_project_name.is_empty() {
                if !app.projects.contains(&app.new_project_name) && app.new_project_name != "all" {
                    let _ = app.new_project(&app.new_project_name);
                    app.projects.push(app.new_project_name.clone());
                    app.projects.sort();
                    app.selected_project = app.projects
                        .iter()
                        .position(|p| p == &app.new_project_name)
                        .unwrap_or(0);
                    app.load_references();
                    app.show_alert(&format!("Created new project: {}", app.new_project_name));
                } else {
                    app.show_alert(&format!("Project {} already exists!", app.new_project_name));
                }
            }
            app.mode = Mode::Normal;
        }

        // ── Add reference by DOI ─────────────────────────────────────────────
        KeyCode::Char(c) if matches!(app.mode, Mode::Adding) => {
            app.new_ref.push(c);
        }
        KeyCode::Backspace if matches!(app.mode, Mode::Adding) => {
            app.new_ref.pop();
        }
        KeyCode::Esc if matches!(app.mode, Mode::Adding) => {
            app.mode = Mode::Normal;
        }
        KeyCode::Enter if matches!(app.mode, Mode::Adding) => {
            if !app.new_ref.is_empty() {
                let all_bib_path  = app.config.all_bib.to_string_lossy().to_string();
                let pdfs_dir      = app.config.pdfs_dir.to_string_lossy().to_string();
                let unpaywall_email = app.config.unpaywall_email.clone();
                let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                let doi           = app.new_ref.trim().to_string();

                let existing_key = find_existing_by_doi(&all_bib_path, &doi);

                let result = if let Some(key) = existing_key {
                    app.show_alert(&format!("'{}' already in your shelf", key));
                    Ok(key)
                } else {
                    app.suspend_tui().ok();
                    println!("Fetching {}...", app.new_ref);
                    let r = add_reference(&all_bib_path, &pdfs_dir, &doi, unpaywall_email.as_deref());
                    app.resume_tui().ok();
                    terminal.clear().ok();
                    r

                };

                match result {
                    Ok(key) => {
                        let current = &app.projects[app.selected_project];
                        if current != "all" {
                            add_to_project(&proj_map_path, current, &key).ok();
                        }
                        app.show_alert(&format!("New ref '{}' added to '{}'", key, current));
                        app.load_references();
                        if let Some(idx) = app.references.iter().position(|e| e.key == key) {
                            app.selected_reference = idx;
                        }

                    }
                    Err(e) => app.show_alert(&format!("Failed: {e}")),
                }
            }
            app.mode = Mode::Normal;
        }
        KeyCode::Char('A') if matches!(app.mode, Mode::Normal) => {
            app.mode = Mode::Adding;
            app.new_ref.clear();
        }

        // ── Navigation ───────────────────────────────────────────────────────
        KeyCode::Up | KeyCode::Char('k') if matches!(app.mode, Mode::Normal) => {
            if app.selected_reference > 0 {
                app.selected_reference -= 1;
                app.detail_scroll = 0;
            }
        }
        KeyCode::Down | KeyCode::Char('j') if matches!(app.mode, Mode::Normal) => {
            let len = if !app.search_query.is_empty() { app.filtered_refs.len() } else { app.references.len() };
            if app.selected_reference + 1 < len {
                app.selected_reference += 1;
                app.detail_scroll = 0;
            }
        }
        KeyCode::Left | KeyCode::Char('h') if matches!(app.mode, Mode::Normal | Mode::Search) => {
            if app.selected_project > 0 {
                app.selected_project -= 1;
                app.project_scroll = 0;
                app.load_references();
                if !app.search_all_refs.is_empty() {
                    app.update_filtered_for_project();
                }
            }
        }
        KeyCode::Right | KeyCode::Char('l') if matches!(app.mode, Mode::Normal | Mode::Search) => {
            if app.selected_project + 1 < app.projects.len() {
                app.selected_project += 1;
                app.project_scroll = 0;
                app.load_references();
                if !app.search_all_refs.is_empty() {
                    app.update_filtered_for_project();
                }
            }
        }
        KeyCode::Down | KeyCode::Char('g') if matches!(app.mode, Mode::Normal) => {
            app.selected_reference = 0;
            app.detail_scroll = 0;
        }
        KeyCode::Down | KeyCode::Char('G') if matches!(app.mode, Mode::Normal) => {
            let len = if !app.search_query.is_empty() { app.filtered_refs.len() } else { app.references.len() };
            app.selected_reference = len-1;
            app.detail_scroll = 0;
        }

        // ── Page-wise reference scroll (vim-style C-d / C-u) ─────────────────
        KeyCode::Char('d') if ctrl && matches!(app.mode, Mode::Normal) => {
            let len = if !app.search_query.is_empty() { app.filtered_refs.len() } else { app.references.len() };
            if len > 0 {
                let step = (app.ref_panel_visible / 2).max(1);
                app.selected_reference = (app.selected_reference + step).min(len - 1);
                app.detail_scroll = 0;
            }
        }
        KeyCode::Char('u') if ctrl && matches!(app.mode, Mode::Normal) => {
            let step = (app.ref_panel_visible / 2).max(1);
            app.selected_reference = app.selected_reference.saturating_sub(step);
            app.detail_scroll = 0;
        }

        // ── Detail panel scroll ───────────────────────────────────────────────
        KeyCode::Char('u') if matches!(app.mode, Mode::Normal) => {
            app.detail_scroll = app.detail_scroll.saturating_sub(3);
        }
        KeyCode::Char('d') if matches!(app.mode, Mode::Normal) => {
            app.detail_scroll += 3;
        }

        // ── Moving a reference to another project ─────────────────────────────
        KeyCode::Up | KeyCode::Char('k') if matches!(app.mode, Mode::Moving) => {
            if app.moving_target > 0 { app.moving_target -= 1; }
        }
        KeyCode::Down | KeyCode::Char('j') if matches!(app.mode, Mode::Moving) => {
            let targets: Vec<&String> = app.projects.iter().filter(|p| p.as_str() != "all").collect();
            if app.moving_target + 1 < targets.len() { app.moving_target += 1; }
        }
        KeyCode::Esc if matches!(app.mode, Mode::Moving) => {
            app.mode = Mode::Normal;
        }
        KeyCode::Enter if matches!(app.mode, Mode::Moving) => {
            let targets: Vec<String> = app.projects.iter()
                .filter(|p| p.as_str() != "all")
                .cloned()
                .collect();
            if let Some(target_project) = targets.get(app.moving_target) {
                let active_refs = if !app.search_query.is_empty() {
                    &app.filtered_refs
                } else {
                    &app.references
                };
                if let Some(entry) = active_refs.get(app.selected_reference) {
                    let key           = entry.key.clone();
                    let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                    match add_to_project(&proj_map_path, target_project, &key) {
                        Ok(_)  => app.show_alert(&format!("Copied '{}' to '{}'", key, target_project)),
                        Err(e) => app.show_alert(&format!("Failed: {e}")),
                    }
                }
            }
            app.mode = Mode::Normal;
        }
        KeyCode::Char('M') if matches!(app.mode, Mode::Normal) => {
            if !app.references.is_empty() {
                app.mode = Mode::Moving;
                app.moving_target = 0;
            }
        }

        // ── Open PDF / Enter ──────────────────────────────────────────────────
        KeyCode::Enter if matches!(app.mode, Mode::Normal) => {
            let active_refs = if !app.search_query.is_empty() {
                app.filtered_refs.clone()
            } else {
                app.references.clone()
            };
            if let Some(r) = active_refs.get(app.selected_reference) {
                let safe_name = r.doi().ok()
                    .as_deref()
                    .unwrap_or("")
                    .replace('/', "-");
                let pdf_path = app.config.pdfs_dir.join(format!("{safe_name}.pdf"));
                if pdf_path.exists() {
                    if let Err(err) = std::process::Command::new("xdg-open").arg(&pdf_path).spawn() {
                        app.show_alert(&format!("Failed to open PDF: {}", err));
                    }
                } else {
                    app.show_alert(&format!("PDF not found: {}", pdf_path.display()));
                }
            }
        }

        // ── Edit reference in $EDITOR ─────────────────────────────────────────
        // TODO: One should be able to set the editor in the config file
        KeyCode::Char('e') if matches!(app.mode, Mode::Normal) => {
            let search_active = !app.search_query.is_empty();
            let active_refs = if search_active { &app.filtered_refs } else { &app.references };
            if let Some(entry) = active_refs.get(app.selected_reference) {
                let all_bib_path = app.config.all_bib.to_string_lossy().to_string();
                let key = entry.key.clone();
                app.suspend_tui().ok();
                let editor = std::env::var("EDITOR").unwrap_or("nvim".into());
                let _ = std::process::Command::new(editor)
                    .arg(format!("+/@.*{{{},", key))
                    .arg(&all_bib_path)
                    .status();
                app.resume_tui().ok();
                terminal.clear().ok();

                if let Ok(content) = fs::read_to_string(&all_bib_path) {
                    if let Ok(bib) = Bibliography::parse(&content) {
                        if bib.get(&key).is_none() {
                            app.show_alert(&format!(
                                "⚠️ Key '{key}' no longer exists in all.bib — update projects.json manually or re-add."
                            ));
                            if let Some(project) = app.projects.get(app.selected_project) {
                                let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                                let _ = remove_from_project(&proj_map_path, project, &key);
                            }
                        }
                    }
                }
                app.load_references();
                if search_active {
                    app.apply_search_live();
                    if let Some(idx) = app.filtered_refs.iter().position(|e| e.key == key) {
                        app.selected_reference = idx;
                    }
                } else if let Some(idx) = app.references.iter().position(|e| e.key == key) {
                    app.selected_reference = idx;
                }
            }
        }

        // ── Re-fetch metadata ────────────────────────────────────────────────
        KeyCode::Char('F') if matches!(app.mode, Mode::Normal) => {
            let active_refs = if !app.search_query.is_empty() {
                app.filtered_refs.clone()
            } else {
                app.references.clone()
            };
            if let Some(entry) = active_refs.get(app.selected_reference) {
                let key             = entry.key.clone();
                let all_bib_path    = app.config.all_bib.to_string_lossy().to_string();
                let pdfs_dir        = app.config.pdfs_dir.to_string_lossy().to_string();
                let unpaywall_email = app.config.unpaywall_email.clone();
                app.log(&format!("Refetching metadata for '{}'", key));
                app.suspend_tui().ok();
                println!("Fetching metadata for '{}'...", key);
                if unpaywall_email.is_some() {
                    println!("Will also look for an open-access PDF if one is missing...");
                }
                let result = refetch_metadata(
                    &all_bib_path,
                    &pdfs_dir,
                    &key,
                    unpaywall_email.as_deref(),
                );
                app.resume_tui().ok();
                terminal.clear().ok();
                match result {
                    Ok(got_pdf) => {
                        app.load_references();
                        if let Some(idx) = app.references.iter().position(|e| e.key == key) {
                            app.selected_reference = idx;
                        }
                        if got_pdf {
                            app.log(&format!("  Metadata updated and PDF downloaded for '{}'", key));
                            app.show_alert(&format!("Metadata + PDF updated for '{}'", key));
                        } else {
                            app.log(&format!("  Metadata updated for '{}'", key));
                            app.show_alert(&format!("Metadata updated for '{}'", key));
                        }
                    }
                    Err(e) => {
                        app.log(&format!("  Fetch failed for '{}': {}", key, e));
                        app.show_alert(&format!("Fetch failed: {e}"));
                    }
                }
            }
            app.search_query.clear();
            app.clear_filtered_refs();
            app.mode = Mode::Normal;
        }

        // ── Import .bib file ──────────────────────────────────────────────────
        KeyCode::Char('I') if matches!(app.mode, Mode::Normal) => {
            let start = std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            app.file_browser = Some(FileBrowser::new(start, FileBrowserMode::Bib));
            app.mode = Mode::FileBrowser;
        }

        // ── Import DOIs from a text file (one per line) ───────────────────────
        KeyCode::Char('i') if matches!(app.mode, Mode::Normal) => {
            let start = std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            app.file_browser = Some(FileBrowser::new(start, FileBrowserMode::DoiList));
            app.mode = Mode::FileBrowser;
        }

        // ── Import PDF ────────────────────────────────────────────────────────
        KeyCode::Char('P') if matches!(app.mode, Mode::Normal) => {
            let start = std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            app.file_browser = Some(FileBrowser::new(start, FileBrowserMode::Pdf));
            app.mode = Mode::FileBrowser;
        }

        // ── Link PDF to the currently-selected reference ──────────────────────
        KeyCode::Char('p') if matches!(app.mode, Mode::Normal) => {
            let active_refs = if !app.search_query.is_empty() {
                &app.filtered_refs
            } else {
                &app.references
            };
            if let Some(entry) = active_refs.get(app.selected_reference) {
                let key = entry.key.clone();
                app.pending_link_key = Some(key);
                let start = std::env::current_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("."));
                app.file_browser = Some(FileBrowser::new(start, FileBrowserMode::Pdf));
                app.mode = Mode::FileBrowser;
            } else {
                app.show_alert("No reference selected");
            }
        }

        // ── Delete reference from project ─────────────────────────────────────
        KeyCode::Char('D') if matches!(app.mode, Mode::Normal) => {
            let current = &app.projects[app.selected_project];
            if current == "all" {
                app.show_alert("Cannot delete from 'all' — select a specific project");
            } else {
                app.mode = Mode::ConfirmRemoveRef;
            }
        }
        KeyCode::Char('y') | KeyCode::Char('Y') if matches!(app.mode, Mode::ConfirmRemoveRef) => {
            let current = app.projects[app.selected_project].clone();
            let active_refs = if !app.search_query.is_empty() {
                app.filtered_refs.clone()
            } else {
                app.references.clone()
            };
            if let Some(entry) = active_refs.get(app.selected_reference) {
                let key           = entry.key.clone();
                let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                match remove_from_project(&proj_map_path, &current, &key) {
                    Ok(_)  => {
                        app.load_references();
                        app.show_alert(&format!("Removed '{}' from '{}'", key, current));
                    }
                    Err(e) => app.show_alert(&format!("Remove failed: {e}")),
                }
            }
            app.mode = Mode::Normal;
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc
            if matches!(app.mode, Mode::ConfirmRemoveRef) =>
        {
            app.show_alert("Removal cancelled");
            app.mode = Mode::Normal;
        }

        // ── Confirm PDF replacement (`p` linked PDF already exists) ──────────
        KeyCode::Char('y') | KeyCode::Char('Y') if matches!(app.mode, Mode::ConfirmReplacePdf) => {
            if let Some((key, source)) = app.pending_replace_pdf.take() {
                let all_bib_path = app.config.all_bib.to_string_lossy().to_string();
                let pdfs_dir     = app.config.pdfs_dir.to_string_lossy().to_string();
                let pdf_str      = source.to_string_lossy().to_string();
                app.log(&format!("Replacing PDF for '{}' with {}", key, source.display()));
                match link_pdf_to_entry(&all_bib_path, &pdfs_dir, &key, &pdf_str, true) {
                    Ok(_)  => app.show_alert(&format!("Replaced PDF for '{}'", key)),
                    Err(e) => app.show_alert(&format!("PDF replace failed: {e}")),
                }
                app.load_references();
                if let Some(idx) = app.references.iter().position(|e| e.key == key) {
                    app.selected_reference = idx;
                }
            }
            app.mode = Mode::Normal;
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc
            if matches!(app.mode, Mode::ConfirmReplacePdf) =>
        {
            app.pending_replace_pdf = None;
            app.show_alert("PDF replacement cancelled");
            app.mode = Mode::Normal;
        }

        // ── Rename project ────────────────────────────────────────────────────
        KeyCode::Char('R') if matches!(app.mode, Mode::Normal) => {
            let current = &app.projects[app.selected_project];
            if current == "all" {
                app.show_alert("Cannot rename 'all'");
            } else {
                app.rename_project_name = current.clone();
                app.mode = Mode::RenameProject;
            }
        }
        KeyCode::Char(c) if matches!(app.mode, Mode::RenameProject) => {
            app.rename_project_name.push(c);
        }
        KeyCode::Backspace if matches!(app.mode, Mode::RenameProject) => {
            app.rename_project_name.pop();
        }
        KeyCode::Esc if matches!(app.mode, Mode::RenameProject) => {
            app.mode = Mode::Normal;
        }
        KeyCode::Enter if matches!(app.mode, Mode::RenameProject) => {
            let new_name = app.rename_project_name.trim().to_string();
            let old_name = app.projects[app.selected_project].clone();
            if !new_name.is_empty() && new_name != old_name {
                let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                match rename_project(&proj_map_path, &old_name, &new_name) {
                    Ok(_) => {
                        app.projects[app.selected_project] = new_name.clone();
                        app.projects.sort();
                        app.selected_project = app.projects
                            .iter()
                            .position(|p| p == &new_name)
                            .unwrap_or(0);
                        app.show_alert(&format!("Renamed '{}' to '{}'", old_name, new_name));
                    }
                    Err(e) => app.show_alert(&format!("Rename failed: {e}")),
                }
            }
            app.mode = Mode::Normal;
        }

        // ── Delete project ────────────────────────────────────────────────────
        KeyCode::Char('X') if matches!(app.mode, Mode::Normal) => {
            let current = app.projects[app.selected_project].clone();
            if current == "all" {
                app.show_alert("Cannot delete 'all'");
            } else {
                app.mode = Mode::ConfirmDelete;
            }
        }
        KeyCode::Char('y') | KeyCode::Char('Y') if matches!(app.mode, Mode::ConfirmDelete) => {
            let current       = app.projects[app.selected_project].clone();
            let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
            match delete_project(&proj_map_path, &current) {
                Ok(_) => {
                    app.projects.remove(app.selected_project);
                    app.selected_project = app.selected_project
                        .saturating_sub(1)
                        .min(app.projects.len().saturating_sub(1));
                    app.load_references();
                    app.show_alert(&format!("Deleted project '{}'", current));
                }
                Err(e) => app.show_alert(&format!("Delete failed: {e}")),
            }
            app.mode = Mode::Normal;
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc
            if matches!(app.mode, Mode::ConfirmDelete) =>
        {
            app.show_alert("Delete cancelled");
            app.mode = Mode::Normal;
        }

        // ── Help screen ───────────────────────────────────────────────────────
        KeyCode::Char('H') if matches!(app.mode, Mode::Normal) => {
            app.mode = Mode::Help;
        }
        KeyCode::Char('q') | KeyCode::Char('H') | KeyCode::Esc
            if matches!(app.mode, Mode::Help) =>
        {
            app.mode = Mode::Normal;
        }
        KeyCode::Char('j') | KeyCode::Down if matches!(app.mode, Mode::Help) => {
            app.help_scroll = app.help_scroll.saturating_add(1);
        }
        KeyCode::Char('k') | KeyCode::Up if matches!(app.mode, Mode::Help) => {
            app.help_scroll = app.help_scroll.saturating_sub(1);
        }

        // ── Copy key to clipboard ─────────────────────────────────────────────
        KeyCode::Char('c') if matches!(app.mode, Mode::Normal) => {
            let active_refs = if !app.search_query.is_empty() {
                &app.filtered_refs
            } else {
                &app.references
            };
            if let Some(entry) = active_refs.get(app.selected_reference) {
                let key = entry.key.clone();
                match app.clipboard.as_mut().map(|cb| cb.set_text(&key)) {
                    Some(Ok(_))  => app.show_alert(&format!("Copied '{}' to clipboard", key)),
                    Some(Err(e)) => app.show_alert(&format!("Clipboard error: {e}")),
                    None         => app.show_alert("Clipboard not available"),
                }
            }
        }

        // ── Copy whole bib entry to clipboard ─────────────────────────────────
        KeyCode::Char('C') if matches!(app.mode, Mode::Normal) => {
            let active_refs = if !app.search_query.is_empty() {
                &app.filtered_refs
            } else {
                &app.references
            };
            if let Some(entry) = active_refs.get(app.selected_reference) {
                let key = entry.key.clone();
                let bib_str = entry.to_biblatex_string();
                match app.clipboard.as_mut().map(|cb| cb.set_text(&bib_str)) {
                    Some(Ok(_))  => app.show_alert(&format!("Copied entry '{}' to clipboard", key)),
                    Some(Err(e)) => app.show_alert(&format!("Clipboard error: {e}")),
                    None         => app.show_alert("Clipboard not available"),
                }
            }
        }

        // ── PDF DOI manual entry ───────────────────────────────────────────────
        KeyCode::Char(c) if matches!(app.mode, Mode::PdfDoi) => {
            app.pdf_doi_input.push(c);
        }
        KeyCode::Backspace if matches!(app.mode, Mode::PdfDoi) => {
            app.pdf_doi_input.pop();
        }
        KeyCode::Esc if matches!(app.mode, Mode::PdfDoi) => {
            app.pending_pdf_path = None;
            app.pdf_doi_input.clear();
            app.mode = Mode::Normal;
        }
        KeyCode::Enter if matches!(app.mode, Mode::PdfDoi) => {
            let doi = app.pdf_doi_input.trim().to_string();
            if !doi.is_empty() {
                if let Some(pdf_path) = app.pending_pdf_path.take() {
                    let all_bib_path  = app.config.all_bib.to_string_lossy().to_string();
                    let unpaywall_email = app.config.unpaywall_email.clone();
                    let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                    let pdf_str       = pdf_path.to_string_lossy().to_string();
                    let pdfs_dir      = app.config.pdfs_dir.to_string_lossy().to_string();

                    app.suspend_tui().ok();
                    let result = if let Some(key) = find_existing_by_doi(&all_bib_path, &doi) {
                        println!("DOI {doi} already in your shelf as '{key}'");
                        app.log(&format!("  DOI {} already in your shelf as '{}'", doi, key));
                        Ok(key)
                    } else {
                        println!("Fetching metadata for DOI: {doi}...");
                        add_reference(&all_bib_path, &pdfs_dir, &doi, unpaywall_email.as_deref())
                    };
                    app.resume_tui().ok();
                    terminal.clear().ok();

                    match result {
                        Ok(key) => {
                            let current = app.projects[app.selected_project].clone();
                            if current != "all" {
                                let _ = add_to_project(&proj_map_path, &current, &key);
                            }
                            match link_pdf_to_entry(&all_bib_path, &pdfs_dir, &key, &pdf_str, false) {
                                Ok(_)  => app.show_alert(&format!("Linked PDF to '{}'", key)),
                                Err(e) => app.show_alert(&format!("PDF copy failed: {e}")),
                            }
                            app.load_references();
                            if let Some(idx) = app.references.iter().position(|e| e.key == key) {
                                app.selected_reference = idx;
                            }
                        }
                        Err(e) => app.show_alert(&format!("Failed: {e}")),
                    }
                }
            }
            app.mode = Mode::Normal;
        }

        // ── File browser ──────────────────────────────────────────────────────
        KeyCode::Char('/') if matches!(app.mode, Mode::FileBrowser) => {
            if let Some(fb) = &mut app.file_browser {
                fb.filtering = true;
                fb.filter.clear();
                fb.selected = 1;
            }
        }
        KeyCode::Esc if matches!(app.mode, Mode::FileBrowser) => {
            if let Some(fb) = &mut app.file_browser {
                if fb.filtering {
                    fb.filtering = false;
                    fb.filter.clear();
                    fb.selected = 0;
                } else {
                    app.file_browser = None;
                    app.pending_link_key = None;
                    app.mode = Mode::Normal;
                }
            }
        }
        KeyCode::Up | KeyCode::Char('k') if matches!(app.mode, Mode::FileBrowser) => {
            if let Some(fb) = &mut app.file_browser {
                if fb.filtering{
                    fb.filter.push('k');
                } else if fb.selected > 0 { 
                    fb.selected -= 1; 
                }
            }
        }
        KeyCode::Down | KeyCode::Char('j') if matches!(app.mode, Mode::FileBrowser) => {
            if let Some(fb) = &mut app.file_browser {
                if fb.filtering {
                    fb.filter.push('j');
                } else {
                    let count = fb.visible_entries().len();
                    if fb.selected + 1 < count { fb.selected += 1; }
                }
            }
        }
        KeyCode::Char(' ') if matches!(app.mode, Mode::FileBrowser) => {
            if let Some(fb) = &mut app.file_browser {
                fb.toggle_current();
                let count = fb.visible_entries().len();
                if fb.selected + 1 < count { fb.selected += 1; }
            }
        }
        KeyCode::Char(c) if matches!(app.mode, Mode::FileBrowser) => {
            if let Some(fb) = &mut app.file_browser {
                if fb.filtering {
                    fb.filter.push(c);
                    fb.selected = 1;
                }
            }
        }
        KeyCode::Backspace if matches!(app.mode, Mode::FileBrowser) => {
            if let Some(fb) = &mut app.file_browser {
                if fb.filtering {
                    fb.filter.pop();
                    fb.selected = 1;
                }
            }
        }
        KeyCode::Enter if matches!(app.mode, Mode::FileBrowser) => {
            handle_file_browser_enter(app, terminal);
        }

        // ── Import project picker ─────────────────────────────────────────────
        KeyCode::Up | KeyCode::Char('k') if matches!(app.mode, Mode::ImportProject) => {
            if app.import_project_target > 0 {
                app.import_project_target -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') if matches!(app.mode, Mode::ImportProject) => {
            // options = non-all projects + 2 special entries
            let count = app.projects.iter().filter(|p| p.as_str() != "all").count() + 2;
            if app.import_project_target + 1 < count {
                app.import_project_target += 1;
            }
        }
        KeyCode::Esc if matches!(app.mode, Mode::ImportProject) => {
            app.pending_import_paths.clear();
            app.mode = Mode::Normal;
        }
        KeyCode::Enter if matches!(app.mode, Mode::ImportProject) => {
            let non_all: Vec<String> = app.projects.iter()
                .filter(|p| p.as_str() != "all")
                .cloned()
                .collect();
            let new_project_idx   = non_all.len();
            let no_project_idx    = non_all.len() + 1;
            let target            = app.import_project_target;
        
            if target == new_project_idx {
                // Switch to new-project-name input
                app.import_new_project_name.clear();
                app.mode = Mode::ImportNewProject;
            } else {
                // Existing project or "no project"
                let project = if target == no_project_idx {
                    None
                } else {
                    non_all.get(target).cloned()
                };
                do_import(app, project, terminal);
            }
        }
        
        // ── Import new project name input ─────────────────────────────────────
        KeyCode::Char(c) if matches!(app.mode, Mode::ImportNewProject) => {
            app.import_new_project_name.push(c);
        }
        KeyCode::Backspace if matches!(app.mode, Mode::ImportNewProject) => {
            app.import_new_project_name.pop();
        }
        KeyCode::Esc if matches!(app.mode, Mode::ImportNewProject) => {
            // Go back to project picker
            app.mode = Mode::ImportProject;
        }
        KeyCode::Enter if matches!(app.mode, Mode::ImportNewProject) => {
            let name = app.import_new_project_name.trim().to_string();
            if name.is_empty() || name == "all" {
                app.show_alert("Invalid project name");
            } else if app.projects.contains(&name) {
                app.show_alert(&format!("Project '{}' already exists", name));
            } else {
                let _ = app.new_project(&name);
                app.projects.push(name.clone());
                app.projects.sort();
                app.selected_project = app.projects.iter().position(|p| *p == name).unwrap_or(0);
                do_import(app, Some(name), terminal);
            }
        }

        // ── Show logs ─────────────────────────────────────
        KeyCode::Char('L') if matches!(app.mode, Mode::Normal) => {
            let log_path = app.log_path.to_string_lossy().to_string();
            app.suspend_tui().ok();
            // `+G` opens at the end — the newest entries are what you want.
            let _ = std::process::Command::new("less")
                .arg("+G")
                .arg(&log_path)
                .status();

            app.resume_tui().ok();
            terminal.clear().ok();

        }

        _ => {}
    }

    false
}

// ---------------------------------------------------------------------------
// File-browser Enter — separated for readability
// ---------------------------------------------------------------------------

fn handle_file_browser_enter(
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) {
    let fb = match &mut app.file_browser {
        Some(fb) => fb,
        None => return,
    };

    let browser_mode = fb.browser_mode;
    let multi: Vec<std::path::PathBuf> = fb.multi_selected.iter().cloned().collect();

    if !multi.is_empty() {
        app.file_browser = None;

        // DOI lists are queued and fetched after the target project is picked.
        if browser_mode == FileBrowserMode::DoiList {
            queue_doi_lists(app, multi);
            return;
        }

        // Link-only path (`p`): use the first PDF, ignore the rest.
        if let Some(key) = app.pending_link_key.take() {
            let all_bib_path = app.config.all_bib.to_string_lossy().to_string();
            let pdfs_dir     = app.config.pdfs_dir.to_string_lossy().to_string();
            if let Some(path) = multi.iter().find(|p| p.extension().and_then(|e| e.to_str()) == Some("pdf")) {
                let pdf_str = path.to_string_lossy().to_string();
                let exists = pdf_dest_for_entry(&all_bib_path, &pdfs_dir, &key)
                    .map(|d| d.exists())
                    .unwrap_or(false);
                if exists {
                    app.pending_replace_pdf = Some((key, path.clone()));
                    app.mode = Mode::ConfirmReplacePdf;
                    return;
                }
                app.log(&format!("Linking PDF {} to '{}'", path.display(), key));
                match link_pdf_to_entry(&all_bib_path, &pdfs_dir, &key, &pdf_str, false) {
                    Ok(_)  => app.show_alert(&format!("Linked PDF to '{}'", key)),
                    Err(e) => app.show_alert(&format!("PDF link failed: {e}")),
                }
                app.load_references();
                if let Some(idx) = app.references.iter().position(|e| e.key == key) {
                    app.selected_reference = idx;
                }
            } else {
                app.show_alert("No PDF selected");
            }
            app.mode = Mode::Normal;
            return;
        }

        for path in multi {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            match ext {
                "pdf" => {
                    let pdf_str       = path.to_string_lossy().to_string();
                    let all_bib_path  = app.config.all_bib.to_string_lossy().to_string();
                    let unpaywall_email = app.config.unpaywall_email.clone();
                    let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                    let pdfs_dir      = app.config.pdfs_dir.to_string_lossy().to_string();

                    app.suspend_tui().ok();
                    println!("Processing: {}", path.display());
                    app.log(&format!("Processing PDF: {}", path.display()));
                    let doi = extract_doi_from_pdf(&pdf_str);

                    match doi {
                        Some(doi) => {
                            app.log(&format!("  DOI found: {}", doi));

                            let existing_key = find_existing_by_doi(&all_bib_path, &doi);

                            let result = if let Some(key) = existing_key {
                                println!("  DOI: {doi}, already in your shelf");
                                Ok(key)
                            } else {
                                println!("  DOI found: {doi}, fetching metadata...");
                                let r = add_reference(&all_bib_path, &pdfs_dir, &doi, unpaywall_email.as_deref());
                                r
                            };

                            match result {
                                Ok(key) => {
                                    let current = app.projects[app.selected_project].clone();
                                    if current != "all" {
                                        let _ = add_to_project(&proj_map_path, &current, &key);
                                    }
                                    let _ = link_pdf_to_entry(&all_bib_path, &pdfs_dir, &key, &pdf_str, false);
                                    println!("  ✓ Added as '{key}'");
                                    app.log(&format!("  Added as '{}'", key));
                                }
                                Err(e) => {
                                    app.log(&format!("  Failed to fetch metadata '{}'", e));
                                    println!("  ✗ Failed: {e}");
                                }
                            }

                        }
                        None => {
                            app.log(&format!("  No DOI found in: {}", path.display()));
                            println!("  ✗ No DOI found in: {}", path.display());
                        }
                    }
                    app.resume_tui().ok();
                    terminal.clear().ok();
                }
                // NOTE: Never been tested
                "bib" => {
                    app.pending_import_paths.push(path);
                }
                _ => {}
            }
        }

        // If any bib files were queued, go to project picker.
        // PDF processing already happened above (suspend/resume inline).
        if !app.pending_import_paths.is_empty() {
            app.pending_import_kind = ImportKind::Bib;
            app.import_project_target = 0;
            app.mode = Mode::ImportProject;
        } else {
            app.load_references();
            app.show_alert("Batch import complete");
            app.mode = Mode::Normal;
        }
        return;
    }

    // Single selection
    let selected_file = {
        let fb = app.file_browser.as_mut().unwrap();
        if fb.filtering {
            fb.filtering = false;
            // fb.filter.clear();
            // return;
        }
        fb.enter()
    };

    if let Some(path) = selected_file {
        // DOI lists have no reliable extension, so they go by browser mode.
        if browser_mode == FileBrowserMode::DoiList {
            app.file_browser = None;
            queue_doi_lists(app, vec![path]);
            return;
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "bib" => {
                app.file_browser = None;

                app.pending_import_paths = vec![path];
                app.pending_import_kind = ImportKind::Bib;
                app.import_project_target = 0;
                app.mode = Mode::ImportProject;
            }
            "pdf" => {
                app.file_browser = None;
                let pdf_str       = path.to_string_lossy().to_string();
                let all_bib_path  = app.config.all_bib.to_string_lossy().to_string();
                let unpaywall_email = app.config.unpaywall_email.clone();
                let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                let pdfs_dir      = app.config.pdfs_dir.to_string_lossy().to_string();

                // Link-only path (`p`): skip DOI extraction and Crossref.
                if let Some(key) = app.pending_link_key.take() {
                    let exists = pdf_dest_for_entry(&all_bib_path, &pdfs_dir, &key)
                        .map(|d| d.exists())
                        .unwrap_or(false);
                    if exists {
                        app.pending_replace_pdf = Some((key, path.clone()));
                        app.mode = Mode::ConfirmReplacePdf;
                        return;
                    }
                    app.log(&format!("Linking PDF {} to '{}'", path.display(), key));
                    match link_pdf_to_entry(&all_bib_path, &pdfs_dir, &key, &pdf_str, false) {
                        Ok(_)  => app.show_alert(&format!("Linked PDF to '{}'", key)),
                        Err(e) => app.show_alert(&format!("PDF link failed: {e}")),
                    }
                    app.load_references();
                    if let Some(idx) = app.references.iter().position(|e| e.key == key) {
                        app.selected_reference = idx;
                    }
                    app.mode = Mode::Normal;
                    return;
                }

                app.log(&format!("Processing PDF: {}", path.display()));

                app.suspend_tui().ok();
                let doi = extract_doi_from_pdf(&pdf_str);
                app.resume_tui().ok();
                terminal.clear().ok();

                match doi {
                    Some(doi) => {
                        app.log(&format!("  DOI found: {}", doi));
                        app.suspend_tui().ok();

                        let existing_key = find_existing_by_doi(&all_bib_path, &doi);

                        let result = if let Some(key) = existing_key {
                            println!("  DOI: {doi}, already in your shelf");
                            Ok(key)
                        } else {
                            println!("  DOI found: {doi}, fetching metadata...");
                            let r = add_reference(&all_bib_path, &pdfs_dir, &doi, unpaywall_email.as_deref());
                            r
                        };

                        app.resume_tui().ok();
                        terminal.clear().ok();

                        match result {
                            Ok(key) => {
                                let current = app.projects[app.selected_project].clone();
                                if current != "all" {
                                    let _ = add_to_project(&proj_map_path, &current, &key);
                                }
                                match link_pdf_to_entry(&all_bib_path, &pdfs_dir, &key, &pdf_str, false) {
                                    Ok(_)  => {
                                        app.show_alert(&format!("Linked PDF to '{}'", key));
                                        app.log(&format!("  Added as '{}', linked PDF", key));
                                    }
                                    Err(e) => {
                                        app.log(&format!("  PDF copy failed: '{}'", e));
                                        app.show_alert(&format!("PDF copy failed: {e}"));
                                    }
                                }
                                app.load_references();
                                if let Some(idx) = app.references.iter().position(|e| e.key == key) {
                                    app.selected_reference = idx;
                                }
                            }
                            Err(e) => {
                                app.log(&format!("  Failed to add reference: '{}'", e));
                                app.show_alert(&format!("Failed to add reference: {e}"));
                            }
                        }

                        app.mode = Mode::Normal;
                    }
                    None => {
                        app.log(&format!("  No DOI found in: {}", path.display()));
                        // Prompt user for DOI manually
                        app.pending_pdf_path = Some(path);
                        app.pdf_doi_input.clear();
                        app.mode = Mode::PdfDoi;
                    }
                }
            }
            _ => {}
        }
    }
}

/// Queue DOI-list files and hand over to the project picker.
fn queue_doi_lists(app: &mut App, mut paths: Vec<std::path::PathBuf>) {
    paths.sort();
    app.pending_import_paths = paths;
    app.pending_import_kind = ImportKind::DoiList;
    app.import_project_target = 0;
    app.mode = Mode::ImportProject;
}

fn do_import(
    app: &mut App,
    project: Option<String>,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) {
    match app.pending_import_kind {
        ImportKind::Bib     => do_bib_import(app, project),
        ImportKind::DoiList => do_doi_list_import(app, project, terminal),
    }
}

/// Fetch every DOI listed in the queued text files. Crossref is slow enough
/// that this runs with the TUI suspended so progress is visible.
fn do_doi_list_import(
    app: &mut App,
    project: Option<String>,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) {
    let all_bib_path  = app.config.all_bib.to_string_lossy().to_string();
    let pdfs_dir      = app.config.pdfs_dir.to_string_lossy().to_string();
    let unpaywall_email = app.config.unpaywall_email.clone();
    let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
    let paths = std::mem::take(&mut app.pending_import_paths);

    let (mut added, mut existing, mut failed) = (0usize, 0usize, 0usize);

    app.suspend_tui().ok();

    for path in &paths {
        app.log(&format!("Importing DOI list: {}", path.display()));
        println!("Reading {}...", path.display());

        let list = match parse_doi_list(path.to_str().unwrap_or("")) {
            Ok(list) => list,
            Err(e) => {
                app.log(&format!("  Read failed: {}", e));
                println!("  ✗ Read failed: {e}");
                failed += 1;
                continue;
            }
        };

        for line in &list.skipped {
            app.log(&format!("  No DOI in line: {}", line));
            println!("  ⚠ No DOI in line: {line}");
        }

        let total = list.dois.len();
        app.log(&format!("  {} DOIs found", total));

        for (i, doi) in list.dois.iter().enumerate() {
            println!("[{}/{}] {}", i + 1, total, doi);

            let result = if let Some(key) = find_existing_by_doi(&all_bib_path, doi) {
                println!("  already in your shelf as '{key}'");
                app.log(&format!("  {} already in your shelf as '{}'", doi, key));
                existing += 1;
                Ok(key)
            } else {
                match add_reference(&all_bib_path, &pdfs_dir, doi, unpaywall_email.as_deref()) {
                    Ok(key) => {
                        println!("  ✓ added as '{key}'");
                        app.log(&format!("  {} added as '{}'", doi, key));
                        added += 1;
                        Ok(key)
                    }
                    Err(e) => Err(e),
                }
            };

            match result {
                Ok(key) => {
                    if let Some(ref proj) = project {
                        match add_to_project(&proj_map_path, proj, &key) {
                            Ok(_)  => app.log(&format!("  Added '{}' to project '{}'", key, proj)),
                            Err(e) => app.log(&format!("  Failed to add '{}' to project '{}': {}", key, proj, e)),
                        }
                    }
                }
                Err(e) => {
                    println!("  ✗ failed: {e}");
                    app.log(&format!("  {} failed: {}", doi, e));
                    failed += 1;
                }
            }
        }
    }

    app.resume_tui().ok();
    terminal.clear().ok();

    app.load_references();
    let dest = project.as_deref().unwrap_or("all");
    app.log(&format!(
        "DOI import complete: {} added, {} already present, {} failed (into '{}')",
        added, existing, failed, dest
    ));
    app.show_alert(&format!(
        "DOI import: {} added, {} already present, {} failed",
        added, existing, failed
    ));
    app.mode = Mode::Normal;
}

fn do_bib_import(app: &mut App, project: Option<String>) {
    let all_bib_path  = app.config.all_bib.to_string_lossy().to_string();
    let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
    let paths = std::mem::take(&mut app.pending_import_paths);
    let mut total = 0usize;

    for path in &paths {
        app.log(&format!("Importing bib: {}", path.display()));
        match import_bib_file(&all_bib_path, path.to_str().unwrap_or("")) {
            Ok(keys) => {
                app.log(&format!("  {} entries found: {}", keys.len(), keys.join(", ")));
                total += keys.len();
                if let Some(ref proj) = project {
                    for key in &keys {
                        match add_to_project(&proj_map_path, proj, key) {
                            Ok(_)  => app.log(&format!("  Added '{}' to project '{}'", key, proj)),
                            Err(e) => app.log(&format!("  Failed to add '{}' to project '{}': {}", key, proj, e)),
                        }
                    }
                }
            }
            Err(e) => {
                app.log(&format!("  Import failed: {}", e));
                app.show_alert(&format!("Import failed: {e}"));
            }
        }
    }

    app.load_references();
    let dest = project.as_deref().unwrap_or("all");
    app.log(&format!("Import complete: {} entries into '{}'", total, dest));
    app.show_alert(&format!("Imported {} entries into '{}'", total, dest));
    app.mode = Mode::Normal;
}


