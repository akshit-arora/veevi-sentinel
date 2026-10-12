use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct ServiceInstaller;

impl ServiceInstaller {
    pub fn install(config_path: &str) -> Result<()> {
        let exe_path = std::env::current_exe()
            .context("Failed to determine current executable path")?;
        let abs_config = fs::canonicalize(config_path)
            .unwrap_or_else(|_| PathBuf::from(config_path));
        let work_dir = abs_config.parent().unwrap_or_else(|| Path::new("."));

        if cfg!(target_os = "linux") {
            Self::install_systemd(&exe_path, &abs_config, work_dir)
        } else if cfg!(target_os = "macos") {
            Self::install_launchd(&exe_path, &abs_config)
        } else {
            println!("Automatic service installation is supported on Linux (systemd) and macOS (launchd).");
            println!("Executable: {}", exe_path.display());
            println!("Config: {}", abs_config.display());
            Ok(())
        }
    }

    fn install_systemd(exe_path: &Path, abs_config: &Path, work_dir: &Path) -> Result<()> {
        let unit_content = format!(
            r#"[Unit]
Description=Veevi Sentinel Zero-Knowledge Database Backup Daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={} --config {} daemon
Restart=always
RestartSec=10
WorkingDirectory={}

[Install]
WantedBy=multi-user.target
"#,
            exe_path.display(),
            abs_config.display(),
            work_dir.display()
        );

        let systemd_system_path = Path::new("/etc/systemd/system/veevi-sentinel.service");
        
        // Attempt to write to /etc/systemd/system
        match fs::write(systemd_system_path, &unit_content) {
            Ok(_) => {
                println!("✅ Successfully created systemd unit at /etc/systemd/system/veevi-sentinel.service");
                let _ = Command::new("systemctl").arg("daemon-reload").status();
                match Command::new("systemctl").args(["enable", "--now", "veevi-sentinel"]).status() {
                    Ok(status) if status.success() => {
                        println!("🚀 Veevi Sentinel service enabled and started successfully via systemctl!");
                    }
                    _ => {
                        println!("ℹ️  Run the following commands to enable and start the service:");
                        println!("   sudo systemctl daemon-reload");
                        println!("   sudo systemctl enable --now veevi-sentinel");
                    }
                }
            }
            Err(_) => {
                // Non-root or read-only /etc: try user-level systemd service
                let home = std::env::var("HOME").unwrap_or_default();
                if !home.is_empty() {
                    let user_service_dir = PathBuf::from(&home).join(".config/systemd/user");
                    let _ = fs::create_dir_all(&user_service_dir);
                    let user_service_file = user_service_dir.join("veevi-sentinel.service");
                    if fs::write(&user_service_file, &unit_content).is_ok() {
                        println!("✅ Created user systemd unit at: {}", user_service_file.display());
                        println!("ℹ️  Run the following commands to enable and start your user service:");
                        println!("   systemctl --user daemon-reload");
                        println!("   systemctl --user enable --now veevi-sentinel");
                        return Ok(());
                    }
                }

                println!("⚠️  Unable to write to /etc/systemd/system/ (root permission required).");
                println!("\nPlease create '/etc/systemd/system/veevi-sentinel.service' with this content:\n");
                println!("{}", unit_content);
                println!("Then enable and start the service:\n");
                println!("   sudo systemctl daemon-reload");
                println!("   sudo systemctl enable --now veevi-sentinel\n");
            }
        }

        Ok(())
    }

    fn install_launchd(exe_path: &Path, abs_config: &Path) -> Result<()> {
        let plist_content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.veevi.sentinel</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>--config</string>
        <string>{}</string>
        <string>daemon</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>
"#,
            exe_path.display(),
            abs_config.display()
        );

        let home = std::env::var("HOME").unwrap_or_default();
        if !home.is_empty() {
            let launch_agents_dir = PathBuf::from(&home).join("Library/LaunchAgents");
            let _ = fs::create_dir_all(&launch_agents_dir);
            let plist_path = launch_agents_dir.join("com.veevi.sentinel.plist");
            if fs::write(&plist_path, &plist_content).is_ok() {
                println!("✅ Successfully created launchd agent at: {}", plist_path.display());
                match Command::new("launchctl").args(["load", "-w", plist_path.to_str().unwrap()]).status() {
                    Ok(status) if status.success() => {
                        println!("🚀 Veevi Sentinel loaded into launchd!");
                    }
                    _ => {
                        println!("ℹ️  To start the service now, run:");
                        println!("   launchctl load -w {}", plist_path.display());
                    }
                }
                return Ok(());
            }
        }

        println!("Launchd configuration:\n\n{}", plist_content);
        Ok(())
    }
}
