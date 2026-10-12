use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, version, about = "Veevi Sentinel - Zero-Knowledge Database Backup CLI", long_about = None)]
pub struct Cli {
    /// Path to configuration file (default: config.yaml or config.yml)
    #[arg(short, long, global = true, value_name = "FILE")]
    pub config: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Starts the background scheduler based on config.yaml cron expressions
    Daemon,
    
    /// Generates a default config.yaml template in the current directory
    Init,
    
    /// Installs a system service to run the daemon automatically on boot
    InstallService,
    
    /// On-demand backup. Leave target blank for interactive prompt, or specify "all" or a db-id.
    Backup {
        #[arg(value_name = "TARGET")]
        target: Option<String>,
    },
    
    /// Lists all available backups in the cloud bucket for a given database ID
    List {
        #[arg(value_name = "DB_ID")]
        db_id: Option<String>,
    },
    
    /// Restores a specific backup for a database.
    Restore {
        #[arg(value_name = "DB_ID")]
        db_id: String,
        
        #[arg(value_name = "BACKUP_ID")]
        backup_id: String,
        
        /// Output the decrypted backup to a file instead of piping directly to the DB
        #[arg(long)]
        to_file: Option<String>,
        
        /// Pipe the decrypted backup directly into the database engine
        #[arg(long, default_value_t = false)]
        to_db: bool,
    },
    
    /// Performs a live connectivity check on storage and databases.
    DryRun {
        #[arg(value_name = "TARGET")]
        target: Option<String>,
    },
}
