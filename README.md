# Veevi Sentinel

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![Rust Edition](https://img.shields.io/badge/Rust-2021-orange.svg)](https://www.rust-lang.org)
[![Security: XChaCha20-Poly1305](https://img.shields.io/badge/Security-XChaCha20--Poly1305-emerald.svg)](https://en.wikipedia.org/wiki/Poly1305)
[![Zero Knowledge](https://img.shields.io/badge/Zero--Knowledge-Client--Side-cyan.svg)](#security-architecture)

> **Zero-Knowledge, Streaming Database Backup Engine for Self-Hosted Infrastructure.**  
> Stream database dumps directly through in-memory Gzip compression and authenticated encryption into your cloud bucket — **without ever touching local disk in plaintext**.

---

## ⚡ Why Veevi Sentinel?

Most database backup setups rely on brittle bash scripts that run `pg_dump` to `/tmp`, write gigabytes of unencrypted SQL to disk, and upload unencrypted blobs to cloud buckets with broad credentials. If your disk fills up, the server crashes. If storage is compromised, your customer data leaks.

**Veevi Sentinel solves this with client-side Zero-Knowledge architecture:**

* 🔒 **Zero-Knowledge Encryption**: Backups are encrypted locally using **XChaCha20-Poly1305 (256-bit key)** authenticated encryption before leaving your server. Storage providers only ever see high-entropy ciphertext.
* 🚀 **True Stream Pipeline**: Database dumps pipe directly through memory buffers to compression, encryption, and multipart cloud upload. **No plaintext touches disk at any point**, avoiding disk exhaustion and OOM errors.
* 🗄 **Multi-Engine Support**: Native support for **PostgreSQL**, **MySQL**, **MariaDB**, and **SQLite**.
* ☁️ **Bring Your Own Storage**: Direct streaming to **AWS S3** (or S3-compatible like MinIO), **Cloudflare R2**, **Google Drive** (via OAuth2 headless Device Flow), and **Local Directory / NAS**.
* ⏱ **Built-In Scheduler & Retention**: Automated cron schedules and automatic pruning (`keep_last_n`) to enforce compliance and control storage costs.
* 🔔 **Webhook Alerts**: Optional integration with Veevi for delivery notifications and metrics (backup duration, ciphertext size, status).

---

## 🏗️ Architecture Pipeline

```text
┌─────────────────┐       ┌─────────────────┐       ┌────────────────────────┐       ┌──────────────────────┐
│  Database Dump  │ ----> │ Gzip Compression│ ----> │ XChaCha20-Poly1305 AEAD│ ----> │ Cloud Storage Upload │
│ pg_dump/mysqldump│ (RAM) │  (Level 6 Sync) │ (RAM) │  (32-Byte Master Key)  │ (RAM) │ AWS S3 / R2 / GDrive │
└─────────────────┘       └─────────────────┘       └────────────────────────┘       └──────────────────────┘
         │                                                                                       ▲
         └────────────────── Never touches disk as plaintext ──────────────────────────────────┘
```

---

## 📦 Installation

### Option 1: 30-Second Zero-Risk Docker Sandbox (Recommended for Evaluation)

Evaluate Veevi Sentinel on your workstation without touching real infrastructure or live databases:

```bash
docker run --rm -it veevi/sentinel:demo
```

Pre-configured with:
* A seeded SQLite database (`/demo/app.db`) containing realistic user and order records
* A local storage adapter (`/demo/backups`)
* Pre-configured 32-byte XChaCha20-Poly1305 encryption key
* Interactive welcome terminal guiding you through `dry-run`, `backup`, and `restore`

---

### Option 2: Quick Install Script (Linux x86_64 / ARM64)

```bash
curl -fsSL https://sentinel.veevi.dev/install.sh | sh
```

This installs `veevi-sentinel` into `~/.local/bin` (or `/usr/local/bin` if run as root) and verifies the cryptographic checksum against the published `SHA256SUMS` manifest.

### Option 3: Prebuilt Binaries (GitHub Releases)

Download prebuilt archives for your architecture from the [GitHub Releases](https://github.com/akshit-arora/veevi-sentinel/releases) page:

* **Linux x86_64** (Static musl binary): `veevi-sentinel-<version>-linux-x86_64.tar.gz`
* **Linux ARM64** (Static musl binary): `veevi-sentinel-<version>-linux-aarch64.tar.gz`
* **macOS Apple Silicon**: `veevi-sentinel-<version>-darwin-aarch64.tar.gz`
* **macOS Intel**: `veevi-sentinel-<version>-darwin-x86_64.tar.gz`

Extract and move the binary to your PATH:

```bash
tar -xzf veevi-sentinel-*-linux-x86_64.tar.gz
sudo install -m 0755 veevi-sentinel /usr/local/bin/
```

### Option 4: Build from Source

Requires Rust 1.75+ and Cargo:

```bash
git clone https://github.com/akshit-arora/veevi-sentinel.git
cd veevi-sentinel
cargo build --release
sudo cp target/release/veevi-sentinel /usr/local/bin/
```

---

## 📋 Prerequisites

Sentinel leverages database client dump utilities. Ensure the appropriate utility is installed on the host and available in your `PATH`:

* **PostgreSQL**: `pg_dump` and `pg_restore` (e.g. `postgresql-client`)
* **MySQL / MariaDB**: `mysqldump` and `mysql` (e.g. `mysql-client` or `mariadb-client`)
* **SQLite**: `sqlite3` (e.g. `sqlite3`)

---

## 🚀 Quickstart (5 Minutes)

### 1. Initialize Configuration

Generate a clean `config.yaml` template:

```bash
veevi-sentinel init
```

### 2. Configure Your Databases and Storage

Edit `config.yaml` with your database credentials, master 32-byte encryption key, and storage bucket.

```yaml
databases:
  - type: postgres
    id: "production-pg"
    connection_string: "postgres://user:password@127.0.0.1:5432/production_db"
    cron: "0 0 2 * * * *"       # Every night at 02:00:00 UTC
    keep_last_n: 14             # Retain the 14 most recent backups

security:
  encryption_algo: "xchacha20poly1305"
  # MUST be exactly 32 bytes (256 bits). Store this securely!
  encryption_key: "SuperSecretMasterKey32BytesLong!"

storage:
  type: r2
  bucket: "sentinel-backups"
  account_id: "your_cloudflare_account_id"
  access_key: "your_r2_access_key"
  secret_key: "your_r2_secret_key"

logging:
  level: "info"                 # trace, debug, info, warn, error
  type: "daily"                 # daily rolling or single
  file_path: "/var/log/veevi-sentinel"
  file_prefix: "sentinel"
  purge_after_days: 30
```

### 3. Verify Health & Connectivity

Run a dry-run check to validate configuration syntax, test cloud storage authentication, and verify that database utilities exist:

```bash
veevi-sentinel dry-run
```

### 4. Trigger an Immediate Backup

```bash
# Back up a specific database
veevi-sentinel backup production-pg

# Or back up all configured databases
veevi-sentinel backup all
```

### 5. Install Background System Service

Run the daemon as a managed service on system startup:

```bash
# Linux (systemd) or macOS (launchd)
sudo veevi-sentinel install-service
```

---

## ⚙️ Configuration Reference (`config.yaml`)

Veevi Sentinel automatically discovers `config.yaml` or `config.yml` in the current working directory, or accepts a custom configuration file via the `--config <FILE>` flag.

### 1. `databases`
Array of databases to backup. You can configure multiple instances across different database engines.

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | String | Yes | `postgres`, `mysql`, or `sqlite` |
| `id` | String | Yes | Unique identifier for this database. Used in file prefixes and restore targets. |
| `connection_string` | String | Postgres/MySQL | Full URI (e.g. `postgres://user:pass@host:5432/db` or `mysql://user:pass@host:3306/db`). |
| `path` | String | SQLite only | Absolute path to the SQLite `.db` file. |
| `cron` | String | No | 7-field cron expression (`Sec Min Hour Day Month DayOfWeek Year`). If omitted, backup only runs on-demand. |
| `keep_last_n` | Integer | No | Number of latest backups to retain. Older backups in the bucket are automatically pruned. |

### 2. `security`

| Field | Type | Required | Description |
|---|---|---|---|
| `encryption_algo` | String | Yes | `xchacha20poly1305` (legacy alias `chacha20poly1305` also accepted). |
| `encryption_key` | String | Yes | **Exactly 32 bytes (32 ASCII characters).** Zero-knowledge master key. If lost, cloud backups cannot be recovered. |

### 3. `storage` (Choose exactly one provider)

#### Cloudflare R2
```yaml
storage:
  type: r2
  bucket: "sentinel-backups"
  account_id: "your_cloudflare_account_id"
  access_key: "your_r2_access_key"
  secret_key: "your_r2_secret_key"
```

#### AWS S3 / MinIO
```yaml
storage:
  type: s3
  bucket: "my-aws-bucket"
  region: "us-east-1"
  endpoint: null # Optional. Set to "http://localhost:9000" for MinIO or local S3
  access_key: "YOUR_AWS_ACCESS_KEY"
  secret_key: "YOUR_AWS_SECRET_KEY"
```

#### Google Drive (OAuth2 Device Flow)
```yaml
storage:
  type: gdrive
  client_secret_file: "/etc/veevi-sentinel/client_secret.json"
  token_cache_file: "/etc/veevi-sentinel/token_cache.json"
```

> **Headless Google Drive Setup**:  
> Sentinel uses the OAuth2 Device Authorization flow to bypass personal Google Drive service account quotas.  
> 1. In Google Cloud Console, enable **Google Drive API**.  
> 2. Create credentials: select **OAuth Client ID** and choose **TVs and Limited Input devices**.  
> 3. Download the JSON to `client_secret_file`.  
> 4. Run `veevi-sentinel dry-run`. Visit the URL displayed in the terminal (e.g., `https://www.google.com/device`), enter the short code, and authenticate. Permanent tokens are cached at `token_cache_file`.

#### Local Directory / NAS (Staging & Air-Gapped)
```yaml
storage:
  type: local
  path: "/var/backups/sentinel"
```

> **Direct Local Storage**:  
> Perfect for Docker staging sandboxes, developer evaluation, air-gapped systems, or mounted NAS/NFS storage volumes. Backups remain fully encrypted with XChaCha20-Poly1305.

### 4. `logging`

| Field | Type | Default | Description |
|---|---|---|---|
| `level` | String | `info` | Logging verbosity: `trace`, `debug`, `info`, `warn`, `error`. |
| `type` | String | `daily` | `daily` (rolling daily files) or `single` (monolithic file). |
| `file_path` | String | Required | Directory to store logs (e.g. `/var/log/veevi-sentinel`). |
| `file_prefix` | String | `sentinel` | Base filename prefix for log files. |
| `purge_after_days` | Integer | Optional | Automatically delete logs older than N days. |

### 5. `veevi` (Optional)

Configure webhook notifications to receive status alerts on WhatsApp, Telegram or Slack (Coming Soon):

```yaml
veevi:
  license_key: "your_veevi_premium_license_key"
```

---

## 💻 CLI Command Reference

Global flag:
* `-c, --config <FILE>`: Specify custom path to configuration file (defaults to `config.yaml` / `config.yml`).

```bash
# 1. Initialize template configuration in current directory
veevi-sentinel init
veevi-sentinel --config /etc/sentinel.yaml init

# 2. Run diagnostic health and connectivity check
veevi-sentinel dry-run
veevi-sentinel dry-run <database-id>

# 3. Start scheduler daemon in the foreground
veevi-sentinel daemon

# 4. Install background daemon as system service (systemd / launchd)
sudo veevi-sentinel install-service

# 5. On-demand manual backup
veevi-sentinel backup                  # Interactive database selector
veevi-sentinel backup <database-id>    # Back up single database
veevi-sentinel backup all              # Back up all databases in config

# 6. List available backups stored in cloud bucket
veevi-sentinel list                    # Interactive database selector
veevi-sentinel list <database-id>      # List backups for specified database

# 7. Restore a backup
# Decrypts, decompresses, and writes to <database-id>_restored.sql
veevi-sentinel restore <db-id> <backup-key>

# Decrypts and saves to a custom output path
veevi-sentinel restore <db-id> <backup-key> --to-file /var/backups/restored.sql

# Streams decrypted dump directly into the running database engine
veevi-sentinel restore <db-id> <backup-key> --to-db
```

---

## 🔒 Security Architecture

| Security Property | Implementation Details |
|---|---|
| **Cipher** | `XChaCha20-Poly1305` (Authenticated Encryption with Associated Data / AEAD). |
| **Nonce Generation** | Cryptographically secure random 24-byte (192-bit) extended nonce generated per backup object. Extended nonce eliminates nonce-reuse risks under high volumes. |
| **Key Size** | 256-bit (32 bytes) symmetric master key. |
| **Zero Plaintext on Disk** | Backups are processed through an asynchronous memory pipe: stdout stream → in-memory Gzip → ChaCha chunk encryptor → cloud multipart stream. |
| **Integrity Verification** | Poly1305 MAC tag verified during decryption; corrupted or tampered blobs fail immediately before being restored. |

---

## 🛠️ Building Releases & CI/CD

Release assets and manifests are generated automatically by GitHub Actions on every version tag push (`v*`).

To build release packages locally:

```bash
./scripts/build-release.sh
```

This compiles optimized binaries, bundles them into `.tar.gz` archives conforming to `install.sh`, and outputs standard `SHA256SUMS` manifests into the `./dist` directory.

---

## 📄 License

Veevi Sentinel is licensed under the [MIT License](LICENSE).
