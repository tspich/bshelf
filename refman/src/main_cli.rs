use clap::{Parser, Subcommand};
use anyhow::Result;
use refman::{new_project, add_reference, export_bibtex}; // <-- crate name = [package].name in Cargo.toml

#[derive(Parser)]
#[command(name = "refman", version, about = "Minimal reference manager")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new project (empty .json)
    New { project: String },
    /// Add a reference by DOI
    Add { project: String, doi: String },
    Bibtex { 
        project: String,
        #[arg(short, long)]
        doi: Option<String>,
        #[arg(short, long)]
        output: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { project } => {
            new_project(&project)?;
        }
        Commands::Add { project, doi } => {
            add_reference(&project, &doi)?;
        }
        //Commands::List => {
        // refman::list_references(&project_file)?;
        //}
        Commands::Bibtex { project, doi, output } => {
            export_bibtex(&project, doi, output)?;
        }
    }

    Ok(())
}
