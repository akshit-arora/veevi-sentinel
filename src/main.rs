mod cli;
mod config;
mod ports;
mod core;
mod adapters;

use clap::Parser;
use cli::{Cli, Commands};
use config::AppConfig;
use core::health::HealthCheckService;
use core::scheduler::SchedulerService;
use std::path::Path;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Check for config file before proceeding (supports config.yaml and config.yml, or custom --config)
    let config_path = AppConfig::find_config_path(cli.config.as_deref());
    
    // If the user is just trying to initialize the config, we do it immediately
    if let Commands::Init = cli.command {
        let target_init = cli.config.as_deref().unwrap_or("config.yaml");
        if Path::new(target_init).exists() || (cli.config.is_none() && Path::new("config.yml").exists()) {
            eprintln!("Error: Configuration file already exists in the current directory.");
            std::process::exit(1);
        }
        let default_config = include_str!("default_config.yaml");
        std::fs::write(target_init, default_config)?;
        println!("Successfully generated '{}'. Please edit it with your credentials.", target_init);
        return Ok(());
    }

    let config_file = match config_path {
        Some(ref path) if Path::new(path).exists() => path.clone(),
        Some(ref path) => {
            eprintln!("Error: Configuration file '{}' not found.", path);
            std::process::exit(1);
        }
        None => {
            eprintln!("Error: Neither 'config.yaml' nor 'config.yml' was found in the current directory.");
            eprintln!("Run `veevi-sentinel init` to generate a default configuration file.");
            std::process::exit(1);
        }
    };

    let config = AppConfig::load(&config_file)?;
    
    // Initialize Logging
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.logging.level));
    
    // Create file appender based on config
    let file_appender_res = if config.logging.log_type.to_lowercase() == "daily" {
        tracing_appender::rolling::Builder::new()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix(&config.logging.file_prefix)
            .build(&config.logging.file_path)
    } else {
        tracing_appender::rolling::Builder::new()
            .rotation(tracing_appender::rolling::Rotation::NEVER)
            .filename_prefix(format!("{}.log", config.logging.file_prefix))
            .build(&config.logging.file_path)
    };
    
    let stdout_layer = tracing_subscriber::fmt::layer()
        .without_time()
        .with_target(false)
        .with_ansi(true);
        
    let mut _guard = None;
    let file_layer = match file_appender_res {
        Ok(appender) => {
            let (non_blocking, guard) = tracing_appender::non_blocking(appender);
            _guard = Some(guard);
            Some(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(non_blocking)
            )
        }
        Err(e) => {
            eprintln!("Warning: Failed to initialize file logging at '{}': {}. Is this running as root or is the directory missing?", config.logging.file_path, e);
            None
        }
    };
    
    tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer)
        .with(file_layer)
        .init();
    
    // Aggressive startup health check
    HealthCheckService::validate(&config)?;
    tracing::info!("Health check passed. Configuration is valid.");

    let config_arc = Arc::new(config);

    match &cli.command {
        Commands::Init => unreachable!(), // Handled above
        Commands::Daemon => {
            tracing::info!("Starting daemon mode...");
            SchedulerService::start(config_arc).await?;
        }
        Commands::DryRun { target } => {
            crate::core::health::HealthCheckService::live_check(config_arc, target.clone()).await?;
        }
        Commands::InstallService => {
            println!("⚙️  Configuring background system service for Veevi Sentinel...");
            crate::core::service::ServiceInstaller::install(&config_file)?;
        }
        Commands::Backup { target } => {
            if let Some(t) = target {
                if t == "all" {
                    for db in &config_arc.databases {
                        tracing::info!("Running manual on-demand backup for '{}'", db.id());
                        if let Err(e) = SchedulerService::run_backup_for_db(db.id(), &config_arc).await {
                            tracing::error!("Backup failed for {}: {:?}", db.id(), e);
                        }
                    }
                } else {
                    tracing::info!("Running manual on-demand backup for '{}'", t);
                    if let Err(e) = SchedulerService::run_backup_for_db(t, &config_arc).await {
                        tracing::error!("Backup failed: {:?}", e);
                    } else {
                        tracing::info!("Backup completed successfully.");
                    }
                }
            } else {
                if config_arc.databases.len() == 1 {
                    let db = &config_arc.databases[0];
                    tracing::info!("Running manual on-demand backup for '{}'", db.id());
                    if let Err(e) = SchedulerService::run_backup_for_db(db.id(), &config_arc).await {
                        tracing::error!("Backup failed for {}: {:?}", db.id(), e);
                    } else {
                        tracing::info!("Backup completed successfully.");
                    }
                } else {
                    let mut options: Vec<String> = config_arc.databases.iter().map(|db| db.id().to_string()).collect();
                    options.insert(0, "All Databases (all)".to_string());
                    
                    println!();
                    let selection = dialoguer::Select::new()
                        .with_prompt("Select a backup target")
                        .items(&options)
                        .default(0)
                        .interact()?;
                    
                    let chosen_target = if selection == 0 {
                        "all".to_string()
                    } else {
                        options[selection].clone()
                    };
                    
                    if chosen_target == "all" {
                        for db in &config_arc.databases {
                            tracing::info!("Running manual on-demand backup for '{}'", db.id());
                            if let Err(e) = SchedulerService::run_backup_for_db(db.id(), &config_arc).await {
                                tracing::error!("Backup failed for {}: {:?}", db.id(), e);
                            }
                        }
                    } else {
                        tracing::info!("Running manual on-demand backup for '{}'", chosen_target);
                        if let Err(e) = SchedulerService::run_backup_for_db(&chosen_target, &config_arc).await {
                            tracing::error!("Backup failed: {:?}", e);
                        } else {
                            tracing::info!("Backup completed successfully.");
                        }
                    }
                }
            }
        }
        Commands::List { db_id } => {
            let chosen_db_id = if let Some(id) = db_id {
                id.clone()
            } else {
                if config_arc.databases.is_empty() {
                    tracing::error!("No databases configured.");
                    return Ok(());
                } else if config_arc.databases.len() == 1 {
                    config_arc.databases[0].id().to_string()
                } else {
                    let options: Vec<String> = config_arc.databases.iter().map(|db| db.id().to_string()).collect();
                    
                    println!();
                    let selection = dialoguer::Select::new()
                        .with_prompt("Select a database")
                        .items(&options)
                        .default(0)
                        .interact()?;
                    
                    options[selection].clone()
                }
            };
            
            tracing::info!("Fetching backups for '{}'", chosen_db_id);
            match SchedulerService::list_backups_for_db(&chosen_db_id, &config_arc).await {
                Ok(backups) => {
                    println!("Found {} backups for '{}':", backups.len(), chosen_db_id);
                    for b in backups {
                        println!(" - {}", b);
                    }
                }
                Err(e) => tracing::error!("Failed to list backups: {:?}", e),
            }
        }
        Commands::Restore { db_id, backup_id, to_file, to_db } => {
            let target_file = if *to_db {
                None
            } else if let Some(file) = to_file {
                Some(file.clone())
            } else {
                let default_file = format!("{}_restored.sql", db_id);
                Some(default_file)
            };

            tracing::info!("Running restore for '{}' from backup '{}'", db_id, backup_id);
            if let Err(e) = SchedulerService::run_restore_for_db(db_id, backup_id, &config_arc, target_file).await {
                tracing::error!("Restore failed: {:?}", e);
            } else {
                tracing::info!("Restore completed successfully.");
            }
        }
    }

    Ok(())
}
