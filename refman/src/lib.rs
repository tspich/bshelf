use serde::{Serialize, Deserialize};
use serde_json;
use anyhow::Result;
// use std::fmt::format;
use std::fs;
use std::path::Path;
use reqwest::blocking;
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug)]
pub struct Reference {
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<u32>,
    pub doi: Option<String>,
    pub journal: Option<String>,
    pub volume: Option<String>,
    pub number: Option<String>,
    pub pages: Option<String>,
    pub issn: Option<String>,
//pub isbn: Option<String>,
    pub publisher: Option<String>,
    pub pdf: Option<String>,  // relative path to pdf if available
}

/// Create a new project JSON file (empty list).
pub fn new_project(project: &str) -> Result<()> {
    fs::create_dir_all("projects")?;
    let proj_file = format!("projects/{}.json", project);
    if !Path::new(&proj_file).exists() {
        fs::write(&proj_file, "[]")?;
        println!("Created new project: {}", project);
    } else {
        println!("Project {} already exists", project);
    }
    Ok(())
}

/// Add a reference from a DOI (fetch Crossref + Unpaywall).
pub fn add_reference(project: &str, doi: &str) -> Result<()> {
    // fs::create_dir_all("projects")?;
    //
    // 1. Load existing references
    let proj_file = format!("projects/{}.json", project);
    let mut refs: Vec<Reference> = if let Ok(data) = fs::read_to_string(&proj_file) {
        serde_json::from_str(&data).unwrap_or_else(|_| vec![])
    } else {
        vec![]
    };

    println!("📚 Existing references:");
    for r in &refs {
        println!(" - {}", r.doi.as_deref().unwrap_or("No DOI"));
    }

    // check duplicate
    let doi_url = format!("https://doi.org/{}", doi);
    if refs.iter().any(|r| r.doi.as_ref() == Some(&doi_url)) {
        println!("⚠️ DOI {} already exists in project {}", doi_url, project);
        return Ok(());
    }

    // Fetch metadata from Crossref
    let crossref_url = format!("https://api.crossref.org/works/{}", doi);
    let crossref_resp: Value = blocking::get(&crossref_url)?.json()?;
    let item = &crossref_resp["message"];

    let title = item["title"][0].as_str().unwrap_or("").to_string();
    let authors: Vec<String> = item["author"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|a| {
            format!(
                "{} {}",
                a["given"].as_str().unwrap_or(""),
                a["family"].as_str().unwrap_or("")
            )
            .trim()
            .to_string()
        })
        .collect();
    let year = item["published"]["date-parts"][0][0].as_u64().map(|y| y as u32);
    let journal = item["container-title"][0].as_str().unwrap_or("").to_string();
    let volume = item["volume"].as_str().unwrap_or("").to_string();
    let number = item["issue"].as_str().unwrap_or("").to_string();
    let pages = item["pages"].as_str().unwrap_or("").to_string();

    let issn = item["issn-type"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter(|n| n.get("type").and_then(|t| t.as_str()) == Some("electronic"))
        .map(|n| {
            n.get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string()
        })
        .collect::<Vec<String>>()
        .join(", ");

    let publisher = item["publisher"].as_str().unwrap_or("").to_string();

    // 2. Unpaywall for PDF
    let unpaywall_url = format!(
        "https://api.unpaywall.org/v2/{}?email={}",
        doi, "your@email.com"
    );
    let unpaywall_resp: Value = blocking::get(&unpaywall_url)?.json()?;
    let pdf_url = unpaywall_resp["best_oa_location"]["url_for_pdf"]
        .as_str()
        .map(|s| s.to_string());

    let pdf_path = if let Some(url) = pdf_url {
        fs::create_dir_all("pdfs")?;
        let filename = doi.replace("/", "_") + ".pdf";
        let full_path = format!("pdfs/{}", filename);
    
        let resp = blocking::get(&url)?;
        if let Some(ct) = resp.headers().get("content-type") {
            if ct.to_str().unwrap_or("").contains("pdf") {
                let mut file = fs::File::create(&full_path)?;
                let content = resp.bytes()?;
                std::io::copy(&mut content.as_ref(), &mut file)?;
                Some(full_path)
            } else {
                println!("⚠️ Not a PDF: {}", url);
                None
            }
        } else {
            None
        }
    } else {
        None
    };


    // 4. Build Reference
    let reference = Reference {
        title,
        authors,
        year,
        doi: Some(doi_url.clone()),
        journal: Some(journal),
        volume: Some(volume),
        number: Some(number),
        pages: Some(pages),
        issn: Some(issn),
        publisher: Some(publisher),
        pdf: pdf_path,
    };

    refs.push(reference);
    let serialized = serde_json::to_string_pretty(&refs)?;
    fs::write(&proj_file, serialized)?;

    println!("Added reference {} to project {}", doi, project);
    Ok(())
}


pub fn export_bibtex(project: &str, doi: Option<String>, output: Option<String>,
) -> Result<()> {

    let proj_file = format!("projects/{}.json", project);

    let data = fs::read_to_string(proj_file)?;
    let refs: Vec<Reference> = serde_json::from_str(&data)?;

    let entries: Vec<String> = if let Some(doi_filter) = doi {
        refs.iter()
            .filter(|r| r.doi.as_deref() == Some(&doi_filter))
            .map(to_bibtex)
            .collect()
    } else {
        refs.iter().map(to_bibtex).collect()
    };

    if entries.is_empty() {
        println!("No matching reference found.");
        return Ok(());
    }

    let content = entries.join("\n\n");

    if let Some(outfile) = output {
        fs::write(outfile, content)?;
        println!("✅ BibTeX written.");
    } else {
        println!("{}", content);
    }

    Ok(())
}

fn to_bibtex(r: &Reference) -> String {
    let key = r.authors
        .get(0)
        .map(|a| a.split_whitespace().last().unwrap_or("ref").to_lowercase())
        .unwrap_or("ref".to_string());

    let year = r.year.unwrap_or(0);


    format!(
        "@article{{{}{},\n  title = {{{}}},\n  author = {{{}}},\
            \n  year = {{{}}},\n  doi = {{{}}},\
            \n  journal = {{{}}},\n  volume = {{{}}},\
            \n  number = {{{}}},\n  pages = {{{}}},\
            \n  issn = {{{}}},\n  publisher = {{{}}}\n}}",
        key,
        year,
        r.title,
        r.authors.join(" and "),
        year,
        r.doi.as_deref().unwrap_or(""),
        r.journal.as_deref().unwrap_or(""),
        r.volume.as_deref().unwrap_or(""),
        r.number.as_deref().unwrap_or(""),
        r.pages.as_deref().unwrap_or(""),
        r.issn.as_deref().unwrap_or(""),
        r.publisher.as_deref().unwrap_or(""),
    )
}
