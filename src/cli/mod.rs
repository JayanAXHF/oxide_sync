use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long, default_value_t = false)]
    pub server: bool,
    #[arg(required_if_eq("server", "false"), required = false)]
    pub from: Option<PathBuf>,
    #[arg(required_if_eq("server", "false"), required = false)]
    pub to: Option<PathBuf>,
    #[arg(short, long, default_value_t = 22)]
    pub port: u16,
    #[arg(long)]
    pub exclude: Option<Vec<PathBuf>>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,
    #[arg(short, long, default_value_t = false)]
    pub delete: bool,
    #[arg(short, long, default_value_t = false)]
    pub recursive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ClientServerOpts {
    pub to: PathBuf,
    pub delete: bool,
    pub recursive: bool,
    pub dry_run: bool,
    pub verbose: bool,
    pub exclude: Vec<PathBuf>,
}

impl From<&Cli> for ClientServerOpts {
    fn from(cli: &Cli) -> Self {
        ClientServerOpts {
            to: cli.to.clone().unwrap_or_default(),
            delete: cli.delete,
            recursive: cli.recursive,
            dry_run: cli.dry_run,
            verbose: cli.verbose,
            exclude: cli.exclude.clone().unwrap_or_default(),
        }
    }
}
