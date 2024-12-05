mod start;

use anyhow::{Context, Result};
use clap::Parser;
use start::StartCommand;
use std::{
    fs::{create_dir_all, exists},
    path::PathBuf,
};

#[derive(Debug)]
pub struct GlobalArguments {
    data_path: PathBuf,
    database_url: String,
}

pub trait ExecutableCommand {
    /// Consume the instance of and run this command.
    async fn run(self, global_args: GlobalArguments) -> Result<()>;
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about)]
pub struct CommandRoot {
    #[clap(subcommand)]
    command: Commands,

    /// The base directory to store things like configuration files and other persistent data.
    #[arg(
        long = "data-path",
        env = "WHIMSKY_DATA_PATH",
        default_value = dirs::config_local_dir().unwrap().join("whimsky").into_os_string(),
        global = true
    )]
    data_path: PathBuf,

    /// The connection string to use when connecting to the sqlite database.
    /// Supports some connection parameters.
    #[arg(
        long = "database-url",
        env = "DATABASE_URL",
        default_value = format!("sqlite://{}?mode=rwc", dirs::config_local_dir().unwrap().join("whimsky").join("db.sqlite3").to_str().unwrap()),
        global = true
    )]
    database_url: String,
}

#[derive(Debug, Parser)]
enum Commands {
    Start(Box<StartCommand>),
}

impl CommandRoot {
    pub async fn run(self) -> Result<()> {
        if !exists(&self.data_path)? {
            create_dir_all(&self.data_path)
                .context("failed to create directory at provided --data-path")?;
        }
        let global_args = GlobalArguments {
            data_path: self.data_path,
            database_url: self.database_url,
        };
        match self.command {
            Commands::Start(cmd) => cmd.run(global_args).await,
        }
    }
}
