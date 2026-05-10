use regex::Regex;
use serde::Deserialize;
use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use std::ops::Range;
use reqwest::blocking;
use serde_json::Value;
use biblatex::{Bibliography, Chunks, DateValue};
use biblatex::{Entry, EntryType, PermissiveType};
use biblatex::{Chunk, Spanned};
use crossref::Crossref;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io;
use std::process::Command;
use std::collections::HashMap;

fn field<S: AsRef<str>>(s: S) -> Vec<Spanned<Chunk>> {
    vec![Spanned::detached(Chunk::Normal(s.as_ref().to_string()))]
}

// Produce double braces {{...}} can be helpeful in some context like title?
// fn field<S: AsRef<str>>(s: S) -> Vec<Spanned<Chunk>> {
//     vec![Spanned::detached(Chunk::Verbatim(s.as_ref().to_string()))]
// }

fn uuid_fallback() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos().to_string())
        .unwrap_or_else(|_| "dup".to_string())
}

/// Strip JATS XML tags (e.g. `<jats:p>`, `<jats:title>`, `<jats:italic>`) that
/// Crossref wraps abstracts in, plus any other XML/HTML tags, and decode the
/// handful of named entities that show up in practice. Whitespace is collapsed
/// so paragraph breaks don't leave ragged gaps in the rendered details panel.
pub fn clean_jats(s: &str) -> String {
    let tag_re = Regex::new(r"<[^>]+>").unwrap();
    let ws_re  = Regex::new(r"\s+").unwrap();
    let stripped = tag_re.replace_all(s, " ");
    let decoded = stripped
        .replace("&amp;",  "&")
        .replace("&lt;",   "<")
        .replace("&gt;",   ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;",  "'")
        .replace("&nbsp;", " ");
    ws_re.replace_all(&decoded, " ").trim().to_string()
}

pub type ProjectsMap = HashMap<String, Vec<String>>;

pub fn chunks_to_string(chunks: &[Spanned<Chunk>]) -> String {
    chunks
        .iter()
        .map(|c| c.v.get())
        .collect::<String>()
}

pub fn authors_to_string(authors: Vec<biblatex::Person>) -> String {
    authors
        .into_iter()
        .map(|p| {
            match (&p.name, &p.given_name) {
                (g, f) => format!("- {g} {f}"),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn publisher_string(publisher: Vec<Chunks>) -> String {
    publisher
        .into_iter()
        .map(|p| {
            chunks_to_string(&p)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn date_to_year_string(
    date: PermissiveType<biblatex::Date>,
) -> Option<String> {
    match date {
        PermissiveType::Typed(d) => {
            match d.value {
                DateValue::At( year )
                | DateValue::After( year ) => Some(year.to_string()),
                _ => None,
            }
        }
        PermissiveType::Chunks(_) => None,
    }
}

pub fn volume_string(
    vol: PermissiveType<i64>,
) -> Option<String> {
    match vol {
        PermissiveType::Typed(v) => {
            Some(v.to_string())
        }
        PermissiveType::Chunks(_) => None,
    }
}

pub fn pages_string(
    pages: PermissiveType<Vec<Range<u32>>>,
) -> Option<String> {
    match pages {
        PermissiveType::Typed(page) => {
            Some(page
                .into_iter()
                .map(|p| {
                    match (&p.start, &p.end) {
                        (s, e) => format!("{s} {e}"),
                    }
                })
                .collect::<Vec<_>>()
                .join(", "))

        }
        PermissiveType::Chunks(_) => None,
    }
}

pub fn load_projects_map(path: &str) -> Result<ProjectsMap> {
    if !std::path::Path::new(path).exists() {
        return Ok(HashMap::new());
    }
    let content = fs::read_to_string(path)?;
    if content.trim().is_empty(){
        return Ok(HashMap::new());
    }
    Ok(serde_json::from_str(&content)?)
}

pub fn save_projects_map(path: &str, map: &ProjectsMap) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(map)?)?;
    Ok(())
}

pub fn add_to_project(proj_map_path: &str, project: &str, key: &str) -> Result<()> {
    let mut map = load_projects_map(proj_map_path)?;
    let keys = map.entry(project.to_string()).or_default();
    if !keys.contains(&key.to_string()) {
        keys.push(key.to_string());
        save_projects_map(proj_map_path, &map)?;
    }
    Ok(())
}

pub fn remove_from_project(proj_map_path: &str, project: &str, key: &str) -> Result<()> {
    let mut map = load_projects_map(proj_map_path)?;
    if let Some(keys) = map.get_mut(project) {
        keys.retain(|k| k != key);
        save_projects_map(proj_map_path, &map)?;
    }
    Ok(())
}

pub fn project_entries<'a>(
    bib: &'a Bibliography,
    keys: &[String],
) -> Vec<&'a Entry> {
    keys.iter()
    .filter_map(|k| bib.get(k))
    .collect()
}

pub fn add_reference(all_bib: &str, doi: &str) -> Result<String> {
    // 1. Load bibliography
    let content = fs::read_to_string(&all_bib)?;
    let mut bib = Bibliography::parse(&content)?;

    // let doi_url = format!("https://doi.org/{doi}");

    // // 2. Duplicate check
    // if let Some(existing) = bib.iter().find(|e| {
    //     e.get("doi")
    //         .map(|chunks| chunks.iter().any(|c| c.v.get() == doi_url)) 
    //         .unwrap_or(false)
    // }){
    //     return Ok(existing.key.clone());
    // }

    // 3. Fetch Crossref
    let client = Crossref::builder()
        .build()
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let work = client
        .work(doi)
        .map_err(|e| anyhow::anyhow!("crossref work error: {e:?}"))?;

    let title = work
        .title
        .get(0)
        .cloned()
        .unwrap_or_else(|| "<no title>".to_string());

    let journal = work
        .container_title
        .as_ref()
        .and_then(|v| v.get(0))
        .cloned()
        .unwrap_or_default();

    let year = work
        .issued
        .date_parts
        .0
        .get(0)
        .and_then(|d| d.get(0))
        .and_then(|d| d.map(|y| y.to_string()))
        .unwrap_or_default();

    let authors: Vec<String> = work
        .author
        .as_ref()
        .map(|v| {
            v.iter()
                .map(|c| {
                    let given = c.given.as_deref().unwrap_or("").trim();
                    let family = c.family.clone();
                    format!("{} {}", given, family).trim().to_string()
                })
                .collect()
        })
        .unwrap_or_default();

    let volume = work.volume.clone().unwrap_or_default();
    let issue = work.issue.clone().unwrap_or_default();
    let pages = work.page.clone().unwrap_or_default(); // note `page` field in Work
    let issn = work.issn.as_ref().map(|v| v.join(", ")).unwrap_or_default();
    let publisher = work.publisher.clone();
    let doi_url = work.url.clone();

    let abstract_text = work
        .abstract_
        .as_ref()
        .map(|s| clean_jats(s))
        .unwrap_or_default();

    // 4. Citation key
    // let key = format!(
    //     "{}_{}",
    //     authors
    //         .get(0)
    //         .and_then(|a| a.split_whitespace().last())
    //         .unwrap_or("ref")
    //         .to_lowercase(),
    //     year,
    //     //title
    //     //    .split_whitespace()
    //     //    .next()
    //     //    .unwrap_or("ref")
    //     //    .to_lowercase(),
    // );

    let base_key = format!(
        "{}_{}",
        authors
            .get(0)
            .and_then(|a| a.split_whitespace().last())
            .unwrap_or("ref")
            .to_lowercase(),
        year,
    );
    
    let key = if bib.get(&base_key).is_none() {
        base_key.clone()
    } else {
        // Find the first available suffix: smith_2023a, smith_2023b, ...
        ('a'..='z')
            .map(|c| format!("{}{}", base_key, c))
            .find(|candidate| bib.get(candidate).is_none())
            .unwrap_or_else(|| format!("{}_{}", base_key, uuid_fallback()))
    };

    // 5. Build BibTeX entry
    let mut entry = Entry::new(key.clone(), EntryType::Article);

    entry.set("title",     field(title));
    entry.set("author",    field(authors.join(" and ")));
    entry.set("date",      field(&year));
    entry.set("journal",   field(journal));
    entry.set("volume",    field(&volume));
    entry.set("issue",     field(&issue));
    entry.set("pages",     field(&pages));
    entry.set("issn",      field(&issn));
    entry.set("publisher", field(&publisher));
    entry.set("doi",       field(&doi));
    entry.set("url",       field(&doi_url));
    entry.set("abstract",  field(&abstract_text));


    // 6. Unpaywall PDF
    let unpaywall_url = format!(
        "https://api.unpaywall.org/v2/{doi}?email=your@email.com"
    );
    let up: Value = blocking::get(&unpaywall_url)?.json()?;

    if let Some(url) = up["best_oa_location"]["url_for_pdf"].as_str() {
        fs::create_dir_all("pdfs")?;
        let filename = doi.replace("/", "-") + ".pdf";
        let path = format!("pdfs/{filename}");

        let resp = blocking::get(url)?;
        if resp
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .map(|ct| ct.contains("pdf"))
            .unwrap_or(false)
        {
            fs::write(&path, resp.bytes()?)?;
            entry.set("file", field(path));
        }
    }

    // 7. Add + save
    bib.insert(entry);
    // fs::write(&all_bib, bib.to_bibtex_string())?;
    fs::write(&all_bib, bib.to_biblatex_string())?;

    Ok(key)
}

pub fn entry_matches(entry: &biblatex::Entry, query: &str) -> bool {
    let query = query.to_lowercase();

    // -------- TITLE --------
    let title_match = entry
        .get("title")
        .map(|chunks| {
            chunks
                .iter()
                .map(|span| span.v.get())
                .collect::<String>()
        })
        .map(|title| title.to_lowercase().contains(&query))
        .unwrap_or(false);

    if title_match {
        return true;
    }

    // -------- AUTHORS --------
    let author_match = entry
        .author()
        .ok()
        .map(|authors| {
            authors.iter().any(|a| {
                // a.given_name is a String; use as_str()
                let full_name = if a.given_name.trim().is_empty() {
                    a.name.clone()
                } else {
                    format!("{} {}", a.given_name.as_str(), a.name)
                };

                full_name.to_lowercase().contains(&query)
            })
        })
        .unwrap_or(false);
    author_match
}

pub fn open_editor(path: &str, key: &str) -> io::Result<()> {
    let mut stdout = io::stdout();

    // 1️⃣ Restore terminal before launching nvim
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;

    // 2️⃣ Launch nvim and wait for it
    Command::new("nvim")
        .arg(format!("+/@.*{{{},", key))
        .arg(path)
        .status()?;   // <-- BLOCK until exit

    // 3️⃣ Reinitialize TUI
    execute!(stdout, EnterAlternateScreen)?;
    enable_raw_mode()?;

    Ok(())
}

pub fn export_project_bib(all_bib_path: &str, proj_map_path: &str, project: &str, output_path: &str) -> Result<()> {
    let content = fs::read_to_string(all_bib_path)?;
    let bib = Bibliography::parse(&content)?;

    let map = load_projects_map(proj_map_path)?;
    let keys = map.get(project).cloned().unwrap_or_default();

    let mut project_bib = Bibliography::new();
    for key in &keys {
        if let Some(entry) = bib.get(key) {
            project_bib.insert(entry.clone());
        }
    }

    fs::write(output_path, project_bib.to_biblatex_string())?;
    Ok(())
}

pub fn import_bib_file(all_bib_path: &str, import_path: &str) -> Result<Vec<String>> {
    let import_content = fs::read_to_string(import_path)?;
    let import_bib = Bibliography::parse(&import_content)?;

    let all_content = fs::read_to_string(all_bib_path).unwrap_or_default();
    let mut all_bib = if all_content.trim().is_empty() {
        Bibliography::new()
    } else {
        Bibliography::parse(&all_content)?
    };

    let normalize_doi = |s: &str| -> String {
        let lower = s.trim().to_lowercase();
        lower
            .strip_prefix("https://doi.org/")
            .or_else(|| lower.strip_prefix("http://doi.org/"))
            .unwrap_or(&lower)
            .to_string()
    };

    let entry_doi = |e: &Entry| -> Option<String> {
        e.get("doi")
            .map(|c| normalize_doi(&chunks_to_string(c)))
            .filter(|s| !s.is_empty())
    };

    let entry_title = |e: &Entry| -> Option<String> {
        e.get("title")
            .map(|c| chunks_to_string(c).trim().to_lowercase())
            .filter(|s| !s.is_empty())
    };

    let entry_first_author = |e: &Entry| -> Option<String> {
        e.author().ok()
            .and_then(|a| a.into_iter().next())
            .map(|p| p.name.trim().to_lowercase())
            .filter(|s| !s.is_empty())
    };

    let mut keys = Vec::new();

    for entry in import_bib.iter() {
        let existing_key: Option<String> = if let Some(doi) = entry_doi(entry) {
            all_bib.iter()
                .find(|e| entry_doi(e).as_deref() == Some(doi.as_str()))
                .map(|e| e.key.clone())
        } else if let Some(title) = entry_title(entry) {
            let author = entry_first_author(entry);
            all_bib.iter()
                .find(|e| {
                    entry_title(e).as_deref() == Some(title.as_str())
                        && entry_first_author(e) == author
                })
                .map(|e| e.key.clone())
        } else {
            all_bib.get(&entry.key).map(|e| e.key.clone())
        };

        let key = if let Some(k) = existing_key {
            k
        } else {
            let base = entry.key.clone();
            let new_key = if all_bib.get(&base).is_none() {
                base
            } else {
                ('a'..='z')
                    .map(|c| format!("{}{}", base, c))
                    .find(|cand| all_bib.get(cand).is_none())
                    .unwrap_or_else(|| format!("{}_{}", base, uuid_fallback()))
            };

            let mut new_entry = entry.clone();
            new_entry.key = new_key.clone();
            all_bib.insert(new_entry);
            new_key
        };

        keys.push(key);
    }

    fs::write(all_bib_path, all_bib.to_biblatex_string())?;
    Ok(keys)
}

pub fn rename_project(proj_map_path: &str, old_name: &str, new_name: &str) -> Result<()> {
    let mut map = load_projects_map(proj_map_path)?;

    if !map.contains_key(old_name) {
        anyhow::bail!("Project '{}' does not exist", old_name);
    }
    if map.contains_key(new_name) {
        anyhow::bail!("Project '{}' already exists", new_name);
    }

    let keys = map.remove(old_name).unwrap_or_default();
    map.insert(new_name.to_string(), keys);
    save_projects_map(proj_map_path, &map)?;
    Ok(())
}

pub fn delete_project(proj_map_path: &str, project: &str) -> Result<()> {
    let mut map = load_projects_map(proj_map_path)?;

    if !map.contains_key(project) {
        anyhow::bail!("Project '{}' does not exist", project);
    }

    map.remove(project);
    save_projects_map(proj_map_path, &map)?;
    Ok(())
}

/// Query Crossref's `/works` endpoint by title (+ optional author) and return
/// the first matching DOI. Used by `refetch_metadata` for entries without a
/// stored DOI and by `add_reference_by_metadata` for ad-hoc lookups.
pub fn lookup_doi_by_metadata(title: &str, author: &str) -> Result<String> {
    let query = if author.trim().is_empty() {
        title.to_string()
    } else {
        format!("{} {}", title, author)
    };

    let url = format!(
        "https://api.crossref.org/works?query={}&rows=1&select=DOI",
        urlencoding::encode(&query)
    );

    let resp: serde_json::Value = blocking::get(&url)?.json()?;

    resp["message"]["items"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item["DOI"].as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("No Crossref result for: {}", title))
}

pub fn refetch_metadata(all_bib_path: &str, key: &str) -> Result<()> {
    let content = fs::read_to_string(all_bib_path)?;
    let mut bib = Bibliography::parse(&content)?;

    let entry = bib.get(key)
        .ok_or_else(|| anyhow::anyhow!("Key '{}' not found", key))?
        .clone();

    let stored_doi = entry.get("doi")
        .map(|c| chunks_to_string(c))
        .filter(|s| !s.trim().is_empty());

    let (doi, doi_was_missing) = if let Some(stored) = stored_doi {
        let cleaned = stored
            .strip_prefix("https://doi.org/")
            .or_else(|| stored.strip_prefix("http://doi.org/"))
            .unwrap_or(&stored)
            .to_string();
        (cleaned, false)
    } else {
        let title = entry.get("title")
            .map(|c| chunks_to_string(c))
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("No DOI or title found for '{}'", key))?;

        let author = entry.author().ok()
            .and_then(|a| a.into_iter().next())
            .map(|a| a.name.clone())
            .unwrap_or_default();

        (lookup_doi_by_metadata(&title, &author)?, true)
    };

    let client = Crossref::builder()
        .build()
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let work = client
        .work(&doi)
        .map_err(|e| anyhow::anyhow!("Crossref error: {e:?}"))?;

    // Refetch only missing or empty fields
    let entry = bib.get_mut(key).unwrap();

    let is_empty = |e: &Entry, field: &str| {
        e.get(field)
            .map(|c| chunks_to_string(c).trim().is_empty())
            .unwrap_or(true)
    };

    if doi_was_missing {
        entry.set("doi", field(&doi));
    }

    if is_empty(entry, "abstract") {
        if let Some(abs) = &work.abstract_ {
            entry.set("abstract", field(&clean_jats(abs)));
        }
    }

    if is_empty(entry, "journal") {
        if let Some(journal) = work.container_title.as_ref().and_then(|v| v.get(0)) {
            entry.set("journal", field(journal));
        }
    }

    if is_empty(entry, "volume") {
        if let Some(vol) = &work.volume {
            entry.set("volume", field(vol));
        }
    }

    if is_empty(entry, "issue") {
        if let Some(issue) = &work.issue {
            entry.set("issue", field(issue));
        }
    }

    if is_empty(entry, "pages") {
        if let Some(pages) = &work.page {
            entry.set("pages", field(pages));
        }
    }

    if is_empty(entry, "issn") {
        if let Some(issn) = &work.issn {
            entry.set("issn", field(issn.join(", ")));
        }
    }

    if is_empty(entry, "publisher") && !work.publisher.is_empty() {
        entry.set("publisher", field(&work.publisher));
    }

    if is_empty(entry, "url") && !work.url.is_empty() {
        entry.set("url", field(&work.url));
    }

    fs::write(all_bib_path, bib.to_biblatex_string())?;
    Ok(())
}

pub fn extract_doi_from_pdf(pdf_path: &str) -> Option<String> {
    // Shell out to pdftotext, read first 3 pages only
    let output = std::process::Command::new("pdftotext")
        .args(["-l", "3", pdf_path, "-"])
        .output()
        .ok()?;

    let text = String::from_utf8_lossy(&output.stdout).to_string();

    // Match common DOI patterns
    let doi_re = Regex::new(
        r"10\.\d{4,}/\S+"
    ).ok()?;
        // r"(?i)(?:doi[:\s]*|https?://doi\.org/|doi\.org/)(10\.\d{4,}/[^\s\]\)\}\>\,';]+)"

    doi_re.captures(&text)
        .and_then(|c| c.get(0))
        .map(|m| m.as_str().trim_end_matches('.').to_string())
}

pub fn link_pdf_to_entry(all_bib_path: &str, pdfs_dir: &str, key: &str, pdf_path: &str) -> Result<()> {
    let content = fs::read_to_string(all_bib_path)?;
    let mut bib = Bibliography::parse(&content)?;

    let entry = bib.get_mut(key)
        .ok_or_else(|| anyhow::anyhow!("Key '{}' not found", key))?;

    // Get DOI to use as filename
    let doi = entry.get("doi")
        .map(|c| chunks_to_string(c))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("No DOI found for '{}'", key))?;

    // Sanitize DOI for use as filename
    let filename = doi
        .strip_prefix("https://doi.org/")
        .or_else(|| doi.strip_prefix("http://doi.org/"))
        .unwrap_or(&doi)
        .replace('/', "-");

    fs::create_dir_all(pdfs_dir)?;
    let dest = std::path::PathBuf::from(pdfs_dir).join(format!("{filename}.pdf"));

    // Only copy if not already there
    if !dest.exists() {
        fs::copy(pdf_path, &dest)?;
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub projects_file: PathBuf,
    pub pdfs_dir: PathBuf,
    pub all_bib: PathBuf,
}

// pub fn load_config() -> Config {
//     let config_dir = dirs::config_dir()
//         .expect("Could not find config directory")
//         .join("bshelf");
// 
//     let config_path = config_dir.join("config.toml");
// 
//     let contents = fs::read_to_string(&config_path)
//         .expect("Could not read config file");
// 
//     toml::from_str(&contents)
//         .expect("Invalid config file")
// }

pub fn load_config() -> Config {
    let config_dir = dirs::config_dir()
        .expect("Could not find config directory")
        .join("bshelf");

    let config_path = config_dir.join("config.toml");

    if !config_path.exists() {
        println!("No config found at {:?}\n", config_path);
        println!("Let's set up bshelf.\n");

        let all_bib   = prompt("Path to your all.bib file",      "~/.local/share/bshelf/all.bib");
        let pdfs_dir  = prompt("Path to your PDFs directory",     "~/.local/share/bshelf/pdfs");
        let proj_file = prompt("Path to your projects.json file", "~/.local/share/bshelf/projects.json");

        let contents = format!(
            "all_bib = \"{}\"\npdfs_dir = \"{}\"\nprojects_file = \"{}\"\n",
            all_bib, pdfs_dir, proj_file
        );

        fs::create_dir_all(&config_dir)
            .expect("Could not create config directory");
        fs::write(&config_path, &contents)
            .expect("Could not write config file");

        println!("\nConfig saved to {:?}", config_path);

        // Create the files/dirs if they don't exist yet
        let all_bib_expanded   = expand_tilde(&all_bib);
        let pdfs_dir_expanded  = expand_tilde(&pdfs_dir);
        let proj_file_expanded = expand_tilde(&proj_file);

        if let Some(parent) = std::path::Path::new(&all_bib_expanded).parent() {
            fs::create_dir_all(parent).ok();
        }
        if !std::path::Path::new(&all_bib_expanded).exists() {
            fs::write(&all_bib_expanded, "").ok();
            println!("Created empty all.bib at {}", all_bib_expanded);
        }

        fs::create_dir_all(&pdfs_dir_expanded).ok();
        println!("Created PDFs directory at {}", pdfs_dir_expanded);

        if let Some(parent) = std::path::Path::new(&proj_file_expanded).parent() {
            fs::create_dir_all(parent).ok();
        }
        if !std::path::Path::new(&proj_file_expanded).exists() {
            fs::write(&proj_file_expanded, "{}").ok();
            println!("Created empty projects.json at {}", proj_file_expanded);
        }

        println!("\nAll set! Launching bshelf...\n");
    }

    let contents = fs::read_to_string(&config_path)
        .expect("Could not read config file");

    toml::from_str(&contents)
        .expect("Invalid config file")
}

fn prompt(label: &str, default: &str) -> String {
    use std::io::Write;
    print!("{} [{}]: ", label, default);
    std::io::stdout().flush().unwrap();

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    let trimmed = input.trim();

    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        dirs::home_dir()
            .map(|h| h.join(&path[2..]).to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string())
    } else {
        path.to_string()
    }
}

pub fn find_existing_by_doi(all_bib_path: &str, doi: &str) -> Option<String> {
    let content = std::fs::read_to_string(all_bib_path).ok()?;
    let bib = biblatex::Bibliography::parse(&content).ok()?;

    // Normalise to bare DOI for comparison
    // let bare = doi
    //     .strip_prefix("https://doi.org/")
    //     .or_else(|| doi.strip_prefix("http://doi.org/"))
    //     .unwrap_or(doi)
    //     .to_lowercase();

    bib.iter().find(|e| {
        e.get("doi")
            .map(|chunks| {
                let stored = chunks_to_string(chunks).to_lowercase();
                // Match whether stored as bare DOI or full URL
                let stored_bare = stored
                    .strip_prefix("https://doi.org/")
                    .or_else(|| stored.strip_prefix("http://doi.org/"))
                    .unwrap_or(&stored);
                stored_bare == doi.to_lowercase()
            })
            .unwrap_or(false)
    })
    .map(|e| e.key.clone())
}

pub fn add_reference_by_metadata(all_bib: &str, title: &str, author: &str) -> Result<String> {
    let content = fs::read_to_string(all_bib)?;
    let bib = Bibliography::parse(&content)?;

    // Duplicate check by title (rough but avoids re-adding the same thing)
    let title_lower = title.to_lowercase();
    if let Some(existing) = bib.iter().find(|e| {
        e.get("title")
            .map(|c| chunks_to_string(c).to_lowercase())
            .map(|t| t == title_lower)
            .unwrap_or(false)
    }) {
        return Ok(existing.key.clone());
    }

    let doi = lookup_doi_by_metadata(title, author)?;
    add_reference(all_bib, &doi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::path::Path;

    fn write_file(dir: &Path, name: &str, content: &str) -> String {
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        path.to_string_lossy().to_string()
    }

    fn parse_one(s: &str) -> Entry {
        Bibliography::parse(s).unwrap().iter().next().unwrap().clone()
    }

    // ── entry_matches ────────────────────────────────────────────────────────

    #[test]
    fn entry_matches_title_is_case_insensitive_substring() {
        let e = parse_one("@article{a, title = {The CRISPR System}, author = {Jane Doe}}");
        assert!(entry_matches(&e, "crispr"));
        assert!(entry_matches(&e, "CRISPR"));
        assert!(entry_matches(&e, "system"));
        assert!(!entry_matches(&e, "ribosome"));
    }

    #[test]
    fn entry_matches_author_surname_or_given() {
        let e = parse_one("@article{a, title = {X}, author = {Jane Doe and John Smith}}");
        assert!(entry_matches(&e, "doe"));
        assert!(entry_matches(&e, "smith"));
        assert!(entry_matches(&e, "jane"));
        assert!(!entry_matches(&e, "wong"));
    }

    // ── project map CRUD ─────────────────────────────────────────────────────

    #[test]
    fn projects_save_and_load_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("p.json").to_string_lossy().to_string();
        let mut map = ProjectsMap::new();
        map.insert("alpha".into(), vec!["k1".into(), "k2".into()]);
        save_projects_map(&path, &map).unwrap();
        assert_eq!(load_projects_map(&path).unwrap(), map);
    }

    #[test]
    fn projects_load_missing_file_returns_empty_map() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nope.json").to_string_lossy().to_string();
        assert!(load_projects_map(&path).unwrap().is_empty());
    }

    #[test]
    fn projects_load_empty_file_returns_empty_map() {
        let dir = tempdir().unwrap();
        let path = write_file(dir.path(), "p.json", "");
        assert!(load_projects_map(&path).unwrap().is_empty());
    }

    #[test]
    fn add_to_project_creates_and_dedupes() {
        let dir = tempdir().unwrap();
        let path = write_file(dir.path(), "p.json", "{}");
        add_to_project(&path, "alpha", "k1").unwrap();
        add_to_project(&path, "alpha", "k1").unwrap(); // duplicate ignored
        add_to_project(&path, "alpha", "k2").unwrap();
        let map = load_projects_map(&path).unwrap();
        assert_eq!(map.get("alpha"), Some(&vec!["k1".into(), "k2".into()]));
    }

    #[test]
    fn remove_from_project_drops_key() {
        let dir = tempdir().unwrap();
        let path = write_file(dir.path(), "p.json", "{}");
        add_to_project(&path, "alpha", "k1").unwrap();
        add_to_project(&path, "alpha", "k2").unwrap();
        remove_from_project(&path, "alpha", "k1").unwrap();
        let map = load_projects_map(&path).unwrap();
        assert_eq!(map.get("alpha"), Some(&vec!["k2".into()]));
    }

    #[test]
    fn rename_project_moves_keys() {
        let dir = tempdir().unwrap();
        let path = write_file(dir.path(), "p.json", "{}");
        add_to_project(&path, "old", "k").unwrap();
        rename_project(&path, "old", "new").unwrap();
        let map = load_projects_map(&path).unwrap();
        assert!(!map.contains_key("old"));
        assert_eq!(map.get("new"), Some(&vec!["k".into()]));
    }

    #[test]
    fn rename_project_rejects_existing_target() {
        let dir = tempdir().unwrap();
        let path = write_file(dir.path(), "p.json", "{}");
        add_to_project(&path, "a", "k").unwrap();
        add_to_project(&path, "b", "k").unwrap();
        assert!(rename_project(&path, "a", "b").is_err());
    }

    #[test]
    fn delete_project_removes_entry() {
        let dir = tempdir().unwrap();
        let path = write_file(dir.path(), "p.json", "{}");
        add_to_project(&path, "doomed", "k").unwrap();
        delete_project(&path, "doomed").unwrap();
        assert!(!load_projects_map(&path).unwrap().contains_key("doomed"));
    }

    // ── find_existing_by_doi ─────────────────────────────────────────────────

    #[test]
    fn find_existing_by_doi_matches_url_and_bare() {
        let dir = tempdir().unwrap();
        let bib = "@article{smith_2020, title = {T}, author = {Smith}, doi = {https://doi.org/10.1000/A}}\n\
                   @article{doe_2021, title = {U}, author = {Doe}, doi = {10.1000/B}}";
        let path = write_file(dir.path(), "all.bib", bib);
        assert_eq!(find_existing_by_doi(&path, "10.1000/a"), Some("smith_2020".into()));
        assert_eq!(find_existing_by_doi(&path, "10.1000/b"), Some("doe_2021".into()));
        assert_eq!(find_existing_by_doi(&path, "10.9999/missing"), None);
    }

    // ── import_bib_file ──────────────────────────────────────────────────────

    #[test]
    fn import_into_empty_all_bib_inserts_entry() {
        let dir = tempdir().unwrap();
        let all = write_file(dir.path(), "all.bib", "");
        let imp = write_file(
            dir.path(),
            "in.bib",
            "@article{smith_2020, title = {Hello}, author = {Smith}, doi = {10.1000/a}}",
        );
        let keys = import_bib_file(&all, &imp).unwrap();
        assert_eq!(keys, vec!["smith_2020".to_string()]);
        let after = Bibliography::parse(&std::fs::read_to_string(&all).unwrap()).unwrap();
        assert_eq!(after.iter().count(), 1);
        assert!(after.get("smith_2020").is_some());
    }

    #[test]
    fn import_dedupes_by_doi_across_url_and_bare_form() {
        let dir = tempdir().unwrap();
        let all = write_file(
            dir.path(),
            "all.bib",
            "@article{smith_2020, title = {Hello}, author = {Smith}, doi = {https://doi.org/10.1000/A}}",
        );
        let imp = write_file(
            dir.path(),
            "in.bib",
            "@article{some_other_key, title = {Hello again}, author = {Smith}, doi = {10.1000/a}}",
        );
        let keys = import_bib_file(&all, &imp).unwrap();
        assert_eq!(keys, vec!["smith_2020".to_string()]);
        let after = Bibliography::parse(&std::fs::read_to_string(&all).unwrap()).unwrap();
        assert_eq!(after.iter().count(), 1);
    }

    #[test]
    fn import_dedupes_by_title_and_first_author_when_no_doi() {
        let dir = tempdir().unwrap();
        let all = write_file(
            dir.path(),
            "all.bib",
            "@article{smith_2020, title = {The Big Discovery}, author = {Jane Smith}}",
        );
        let imp = write_file(
            dir.path(),
            "in.bib",
            "@article{different_key, title = {The Big Discovery}, author = {Jane Smith}}",
        );
        let keys = import_bib_file(&all, &imp).unwrap();
        assert_eq!(keys, vec!["smith_2020".to_string()]);
        let after = Bibliography::parse(&std::fs::read_to_string(&all).unwrap()).unwrap();
        assert_eq!(after.iter().count(), 1);
    }

    #[test]
    fn import_inserts_when_no_match_found() {
        let dir = tempdir().unwrap();
        let all = write_file(
            dir.path(),
            "all.bib",
            "@article{smith_2020, title = {A}, author = {Smith}, doi = {10.1000/a}}",
        );
        let imp = write_file(
            dir.path(),
            "in.bib",
            "@article{doe_2021, title = {B}, author = {Doe}, doi = {10.1000/b}}",
        );
        let keys = import_bib_file(&all, &imp).unwrap();
        assert_eq!(keys, vec!["doe_2021".to_string()]);
        let after = Bibliography::parse(&std::fs::read_to_string(&all).unwrap()).unwrap();
        assert_eq!(after.iter().count(), 2);
        assert!(after.get("smith_2020").is_some());
        assert!(after.get("doe_2021").is_some());
    }

    #[test]
    fn import_suffixes_key_when_collision_but_distinct_entry() {
        let dir = tempdir().unwrap();
        let all = write_file(
            dir.path(),
            "all.bib",
            "@article{smith_2020, title = {A paper}, author = {Smith}, doi = {10.1000/a}}",
        );
        // Same key but distinct DOI and title — should not overwrite.
        let imp = write_file(
            dir.path(),
            "in.bib",
            "@article{smith_2020, title = {Another paper}, author = {Jones}, doi = {10.1000/b}}",
        );
        let keys = import_bib_file(&all, &imp).unwrap();
        assert_eq!(keys, vec!["smith_2020a".to_string()]);
        let after = Bibliography::parse(&std::fs::read_to_string(&all).unwrap()).unwrap();
        assert_eq!(after.iter().count(), 2);
        let original = after.get("smith_2020").unwrap();
        assert_eq!(
            chunks_to_string(original.get("doi").unwrap()).to_lowercase(),
            "10.1000/a"
        );
    }

    #[test]
    fn import_returns_canonical_keys_in_input_order() {
        let dir = tempdir().unwrap();
        let all = write_file(
            dir.path(),
            "all.bib",
            "@article{smith_2020, title = {Hello}, author = {Smith}, doi = {10.1000/a}}",
        );
        let imp = write_file(
            dir.path(),
            "in.bib",
            "@article{x, title = {Hello}, author = {Smith}, doi = {10.1000/A}}\n\
             @article{doe_2021, title = {New}, author = {Doe}, doi = {10.1000/b}}",
        );
        let keys = import_bib_file(&all, &imp).unwrap();
        assert_eq!(keys, vec!["smith_2020".to_string(), "doe_2021".to_string()]);
    }
}
