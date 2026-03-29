use biblatex::Bibliography;
use crossterm::event::KeyCode;
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
    refetch_metadata,
    remove_from_project,
    rename_project
};

use crate::app::{App, FileBrowser, FileBrowserMode, Mode};

// TODO: Should be impossible to create a 'all' project

/// Handle a single key event.  Returns `true` if the app should quit.
pub fn handle_key(
    app: &mut App,
    key: KeyCode,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> bool {
    match key {
        // ── Quit ────────────────────────────────────────────────────────────
        KeyCode::Char('q') if matches!(app.mode, Mode::Normal) => return true,

        // ── Search ──────────────────────────────────────────────────────────
        KeyCode::Char('/') if matches!(app.mode, Mode::Normal) => {
            app.enter_search_mode();
        }
        KeyCode::Char(c) if matches!(app.mode, Mode::Search) => {
            app.search_query.push(c);
        }
        KeyCode::Backspace if matches!(app.mode, Mode::Search) => {
            app.search_query.pop();
        }
        KeyCode::Esc if matches!(app.mode, Mode::Search) => {
            app.search_query.clear();
            app.clear_filtered_refs();
            app.mode = Mode::Normal;
        }
        KeyCode::Enter if matches!(app.mode, Mode::Search) => {
            app.apply_search();
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
                let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                let doi           = app.new_ref.trim().to_string();

                let existing_key = find_existing_by_doi(&all_bib_path, &doi);

                let result = if let Some(key) = existing_key {
                    app.show_alert(&format!("'{}' already in your shelf", key));
                    Ok(key)
                } else {
                    app.suspend_tui().ok();
                    println!("Fetching {}...", app.new_ref);
                    let r = add_reference(&all_bib_path, &doi);
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
            let len = if !app.filtered_refs.is_empty() { app.filtered_refs.len() } else { app.references.len() };
            if app.selected_reference + 1 < len {
                app.selected_reference += 1;
                app.detail_scroll = 0;
            }
        }
        KeyCode::Left | KeyCode::Char('h') if matches!(app.mode, Mode::Normal) => {
            if app.selected_project > 0 {
                app.selected_project -= 1;
                app.project_scroll = 0;
                app.load_references();
                app.clear_filtered_refs();
            }
        }
        KeyCode::Right | KeyCode::Char('l') if matches!(app.mode, Mode::Normal) => {
            if app.selected_project + 1 < app.projects.len() {
                app.selected_project += 1;
                app.project_scroll = 0;
                app.load_references();
                app.clear_filtered_refs();
            }
        }
        KeyCode::Down | KeyCode::Char('g') if matches!(app.mode, Mode::Normal) => {
            app.selected_reference = 0;
            app.detail_scroll = 0;
        }
        KeyCode::Down | KeyCode::Char('G') if matches!(app.mode, Mode::Normal) => {
            let len = if !app.filtered_refs.is_empty() { app.filtered_refs.len() } else { app.references.len() };
            app.selected_reference = len-1;
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
                let active_refs = if !app.filtered_refs.is_empty() {
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
            let active_refs = if !app.filtered_refs.is_empty() {
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
            if let Some(entry) = app.references.get(app.selected_reference) {
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
            }
        }

        // ── Re-fetch metadata ────────────────────────────────────────────────
        KeyCode::Char('F') if matches!(app.mode, Mode::Normal) => {
            let active_refs = if !app.filtered_refs.is_empty() {
                app.filtered_refs.clone()
            } else {
                app.references.clone()
            };
            if let Some(entry) = active_refs.get(app.selected_reference) {
                let key          = entry.key.clone();
                let all_bib_path = app.config.all_bib.to_string_lossy().to_string();
                app.suspend_tui().ok();
                println!("Fetching metadata for '{}'...", key);
                let result = refetch_metadata(&all_bib_path, &key);
                app.resume_tui().ok();
                terminal.clear().ok();
                match result {
                    Ok(_) => {
                        app.load_references();
                        if let Some(idx) = app.references.iter().position(|e| e.key == key) {
                            app.selected_reference = idx;
                        }
                        app.show_alert(&format!("Metadata updated for '{}'", key));
                    }
                    Err(e) => app.show_alert(&format!("Fetch failed: {e}")),
                }
            }
        }

        // ── Import .bib file ──────────────────────────────────────────────────
        KeyCode::Char('I') if matches!(app.mode, Mode::Normal) => {
            let start = std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            app.file_browser = Some(FileBrowser::new(start, FileBrowserMode::Bib));
            app.mode = Mode::FileBrowser;
        }

        // ── Import PDF ────────────────────────────────────────────────────────
        KeyCode::Char('P') if matches!(app.mode, Mode::Normal) => {
            let start = std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            app.file_browser = Some(FileBrowser::new(start, FileBrowserMode::Pdf));
            app.mode = Mode::FileBrowser;
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
            let active_refs = if !app.filtered_refs.is_empty() {
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
            let active_refs = if !app.filtered_refs.is_empty() {
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
                    let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                    let pdf_str       = pdf_path.to_string_lossy().to_string();
                    let pdfs_dir      = app.config.pdfs_dir.to_string_lossy().to_string();

                    app.suspend_tui().ok();
                    println!("Fetching metadata for DOI: {doi}...");
                    let result = add_reference(&all_bib_path, &doi);
                    app.resume_tui().ok();
                    terminal.clear().ok();

                    match result {
                        Ok(key) => {
                            let current = app.projects[app.selected_project].clone();
                            if current != "all" {
                                let _ = add_to_project(&proj_map_path, &current, &key);
                            }
                            match link_pdf_to_entry(&all_bib_path, &pdfs_dir, &key, &pdf_str) {
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
                    app.mode = Mode::Normal;
                }
            }
        }
        KeyCode::Up | KeyCode::Char('k') if matches!(app.mode, Mode::FileBrowser) => {
            if let Some(fb) = &mut app.file_browser {
                if fb.selected > 0 { fb.selected -= 1; }
            }
        }
        KeyCode::Down | KeyCode::Char('j') if matches!(app.mode, Mode::FileBrowser) => {
            if let Some(fb) = &mut app.file_browser {
                let count = fb.visible_entries().len();
                if fb.selected + 1 < count { fb.selected += 1; }
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
                do_bib_import(app, project);
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
                do_bib_import(app, Some(name));
            }
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

    let multi: Vec<std::path::PathBuf> = fb.multi_selected.iter().cloned().collect();

    if !multi.is_empty() {
        app.file_browser = None;

        for path in multi {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            match ext {
                "pdf" => {
                    let pdf_str       = path.to_string_lossy().to_string();
                    let all_bib_path  = app.config.all_bib.to_string_lossy().to_string();
                    let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                    let pdfs_dir      = app.config.pdfs_dir.to_string_lossy().to_string();

                    app.suspend_tui().ok();
                    println!("Processing: {}", path.display());
                    let doi = extract_doi_from_pdf(&pdf_str);

                    match doi {
                        Some(doi) => {

                            let existing_key = find_existing_by_doi(&all_bib_path, &doi);

                            let result = if let Some(key) = existing_key {
                                println!("  DOI: {doi}, already in your shelf");
                                Ok(key)
                            } else {
                                println!("  DOI found: {doi}, fetching metadata...");
                                let r = add_reference(&all_bib_path, &doi);
                                r
                            };

                            match result {
                                Ok(key) => {
                                    let current = app.projects[app.selected_project].clone();
                                    if current != "all" {
                                        let _ = add_to_project(&proj_map_path, &current, &key);
                                    }
                                    let _ = link_pdf_to_entry(&all_bib_path, &pdfs_dir, &key, &pdf_str);
                                    println!("  ✓ Added as '{key}'");
                                }
                                Err(e) => println!("  ✗ Failed: {e}"),
                            }

                        }
                        None => println!("  ✗ No DOI found in: {}", path.display()),
                    }
                    app.resume_tui().ok();
                    terminal.clear().ok();
                }
                // NOTE: Never been tested
                "bib" => {
                    app.pending_import_paths.push(path);
                    // let all_bib_path  = app.config.all_bib.to_string_lossy().to_string();
                    // let proj_map_path = app.config.projects_file.to_string_lossy().to_string();

                    // match import_bib_file(&all_bib_path, path.to_str().unwrap_or("")) {
                    //     Ok(keys) if keys.is_empty() => {}
                    //     Ok(keys) => {
                    //         let current = app.projects[app.selected_project].clone();
                    //         if current != "all" {
                    //             for key in &keys {
                    //                 let _ = add_to_project(&proj_map_path, &current, key);
                    //             }
                    //         }
                    //     }
                    //     Err(e) => app.show_alert(&format!("Import failed: {e}")),
                    // }
                }
                _ => {}
            }
        }

        //app.load_references();
        //app.show_alert("Batch import complete");
        //app.mode = Mode::Normal;

        // If any bib files were queued, go to project picker.
        // PDF processing already happened above (suspend/resume inline).
        if !app.pending_import_paths.is_empty() {
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
            fb.filter.clear();
            return;
        }
        fb.enter()
    };

    if let Some(path) = selected_file {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "bib" => {
                app.file_browser = None;

                app.pending_import_paths = vec![path];
                app.import_project_target = 0;
                app.mode = Mode::ImportProject;

                // app.mode = Mode::Normal;

                // let all_bib_path  = app.config.all_bib.to_string_lossy().to_string();
                // let proj_map_path = app.config.projects_file.to_string_lossy().to_string();

                // match import_bib_file(&all_bib_path, path.to_str().unwrap_or("")) {
                //     Ok(keys) if keys.is_empty() => app.show_alert("No new entries found"),
                //     Ok(keys) => {
                //         let current = app.projects[app.selected_project].clone();
                //         if current != "all" {
                //             for key in &keys {
                //                 let _ = add_to_project(&proj_map_path, &current, key);
                //             }
                //         }
                //         app.load_references();
                //         app.show_alert(&format!("Imported {} entries", keys.len()));
                //     }
                //     Err(e) => app.show_alert(&format!("Import failed: {e}")),
                // }
            }
            "pdf" => {
                app.file_browser = None;
                let pdf_str       = path.to_string_lossy().to_string();
                let all_bib_path  = app.config.all_bib.to_string_lossy().to_string();
                let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
                let pdfs_dir      = app.config.pdfs_dir.to_string_lossy().to_string();

                app.suspend_tui().ok();
                let doi = extract_doi_from_pdf(&pdf_str);
                app.resume_tui().ok();
                terminal.clear().ok();

                match doi {
                    Some(doi) => {
                        app.suspend_tui().ok();

                        let existing_key = find_existing_by_doi(&all_bib_path, &doi);

                        let result = if let Some(key) = existing_key {
                            println!("  DOI: {doi}, already in your shelf");
                            Ok(key)
                        } else {
                            println!("  DOI found: {doi}, fetching metadata...");
                            let r = add_reference(&all_bib_path, &doi);
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
                                match link_pdf_to_entry(&all_bib_path, &pdfs_dir, &key, &pdf_str) {
                                    Ok(_)  => app.show_alert(&format!("Linked PDF to '{}'", key)),
                                    Err(e) => app.show_alert(&format!("PDF copy failed: {e}")),
                                }
                                app.load_references();
                                if let Some(idx) = app.references.iter().position(|e| e.key == key) {
                                    app.selected_reference = idx;
                                }
                            }
                            Err(e) => app.show_alert(&format!("Failed to add reference: {e}")),
                        }

                        app.mode = Mode::Normal;
                    }
                    None => {
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

fn do_bib_import(app: &mut App, project: Option<String>) {
    let all_bib_path  = app.config.all_bib.to_string_lossy().to_string();
    let proj_map_path = app.config.projects_file.to_string_lossy().to_string();
    let paths = std::mem::take(&mut app.pending_import_paths);
    let mut total = 0usize;

    for path in &paths {
        match import_bib_file(&all_bib_path, path.to_str().unwrap_or("")) {
            Ok(keys) => {
                total += keys.len();
                if let Some(ref proj) = project {
                    for key in &keys {
                        let _ = add_to_project(&proj_map_path, proj, key);
                    }
                }
            }
            Err(e) => app.show_alert(&format!("Import failed: {e}")),
        }
    }

    app.load_references();
    let dest = project.as_deref().unwrap_or("all");
    app.show_alert(&format!("Imported {} entries into '{}'", total, dest));
    app.mode = Mode::Normal;
}


