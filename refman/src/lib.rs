use serde::{Serialize, Deserialize};
// use serde_json;
use anyhow::Result;
// use std::fmt::format;
use std::fs;
use std::path::Path;
use std::ops::Range;
use reqwest::blocking;
use serde_json::Value;
use biblatex::{Bibliography, Chunks, DateValue};
use biblatex::{Entry, EntryType, PermissiveType};
use biblatex::{Chunk, Spanned};
use crossref::Crossref;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Reference {
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<u32>,
    pub doi: Option<String>,
    pub abstract_text: Option<String>,
    pub journal: Option<String>,
    pub volume: Option<String>,
    pub number: Option<String>,
    pub pages: Option<String>,
    pub issn: Option<String>,
    //pub isbn: Option<String>,
    pub publisher: Option<String>,
    //pub pdf: Option<String>,  // relative path to pdf if available
}

/// Create a new project JSON file (empty list).
pub fn new_project(project: &str) -> Result<()> {
    fs::create_dir_all("projects")?;
    let proj_file = format!("projects/{}.bib", project);
    if !Path::new(&proj_file).exists() {
        fs::write(&proj_file, "[]")?;
        println!("Created new project: {}", project);
    } else {
        println!("Project {} already exists", project);
    }
    Ok(())
}

fn field<S: AsRef<str>>(s: S) -> Vec<Spanned<Chunk>> {
    vec![Spanned::detached(Chunk::Verbatim(s.as_ref().to_string()))]
}

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
                (g, f) => format!("{g} {f}"),
                // (String::new(), f) => f.clone(),
                // (g, "") => g.clone(),
                // ("", "") => "<unknown>".to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
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

pub fn add_reference(project: &str, doi: &str) -> Result<()> {
    let proj_file = Path::new("projects").join(format!("{project}.bib"));
    fs::create_dir_all("projects")?;


    // 1. Load bibliography
    // let mut bib = load_bib(&proj_file)?;
    let content = fs::read_to_string(&proj_file)?;
    let mut bib = Bibliography::parse(&content)?;

    let doi_url = format!("https://doi.org/{doi}");

    // 2. Duplicate check
    if bib.iter().any(|e| {
        e.get("doi")
            .map(|chunks|{
                chunks.iter().any(|c| {
                   c.v.get() == doi_url 
                })
            }) 
            .unwrap_or(false)
    }){
        println!("⚠️ DOI already exists: {doi_url}");
        return Ok(());
    }

    // 3. Fetch Crossref
    let client = Crossref::builder()
        .build()
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let work = client
        .work("10.1093/bioinformatics/btad696")
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
        .map(|s| s.clone())
        .unwrap_or_default();

    // 4. Citation key
    let key = format!(
        "{}{}{}",
        authors
            .get(0)
            .and_then(|a| a.split_whitespace().last())
            .unwrap_or("ref")
            .to_lowercase(),
        year,
        title
            .split_whitespace()
            .next()
            .unwrap_or("ref")
            .to_lowercase(),
    );


    // 5. Build BibTeX entry
    let mut entry = Entry::new(key, EntryType::Article);

    entry.set("title", field(title));
    entry.set("author", field(authors.join(" and ")));
    // entry.set("year", field(&year));
    entry.set("date", field(&year));
    entry.set("journal", field(journal));
    entry.set("volume", field(&volume));
    entry.set("issue", field(&issue));
    entry.set("pages", field(&pages));
    entry.set("issn", field(&issn));
    entry.set("publisher", field(&publisher));
    entry.set("doi", field(&doi));
    entry.set("url", field(&doi_url));
    entry.set("abstract", field(&abstract_text));


    // 6. Unpaywall PDF
    let unpaywall_url = format!(
        "https://api.unpaywall.org/v2/{doi}?email=your@email.com"
    );
    let up: Value = blocking::get(&unpaywall_url)?.json()?;

    if let Some(url) = up["best_oa_location"]["url_for_pdf"].as_str() {
        fs::create_dir_all("pdfs")?;
        let filename = doi.replace("/", "_") + ".pdf";
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
    //save_bib(&proj_file, &bib)?;
    fs::write(&proj_file, bib.to_bibtex_string())?;

    println!("✅ Added {doi} to project {project}");
    Ok(())
}


// pub fn export_bibtex(project: &str, doi: Option<String>, output: Option<String>,
// ) -> Result<()> {
// 
//     let proj_file = format!("projects/{}.json", project);
// 
//     let data = fs::read_to_string(proj_file)?;
//     let refs: Vec<Reference> = serde_json::from_str(&data)?;
// 
//     let entries: Vec<String> = if let Some(doi_filter) = doi {
//         refs.iter()
//             .filter(|r| r.doi.as_deref() == Some(&doi_filter))
//             .map(to_bibtex)
//             .collect()
//     } else {
//         refs.iter().map(to_bibtex).collect()
//     };
// 
//     if entries.is_empty() {
//         println!("No matching reference found.");
//         return Ok(());
//     }
// 
//     let content = entries.join("\n\n");
// 
//     if let Some(outfile) = output {
//         fs::write(outfile, content)?;
//         println!("✅ BibTeX written.");
//     } else {
//         println!("{}", content);
//     }
// 
//     Ok(())
// }
// 
// fn to_bibtex(r: &Reference) -> String {
//     let key = r.authors
//         .get(0)
//         .map(|a| a.split_whitespace().last().unwrap_or("ref").to_lowercase())
//         .unwrap_or("ref".to_string());
// 
//     let year = r.year.unwrap_or(0);
// 
// 
//     format!(
//         "@article{{{}{},\n  title = {{{}}},\n  author = {{{}}},\
//             \n  year = {{{}}},\n  doi = {{{}}},\
//             \n  journal = {{{}}},\n  volume = {{{}}},\
//             \n  number = {{{}}},\n  pages = {{{}}},\
//             \n  issn = {{{}}},\n  publisher = {{{}}}\n}}",
//         key,
//         year,
//         r.title,
//         r.authors.join(" and "),
//         year,
//         r.doi.as_deref().unwrap_or(""),
//         r.journal.as_deref().unwrap_or(""),
//         r.volume.as_deref().unwrap_or(""),
//         r.number.as_deref().unwrap_or(""),
//         r.pages.as_deref().unwrap_or(""),
//         r.issn.as_deref().unwrap_or(""),
//         r.publisher.as_deref().unwrap_or(""),
//     )
// }
