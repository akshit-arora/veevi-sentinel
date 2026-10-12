# Veevi Sentinel

**Veevi Sentinel** is a secure, open-source, Zero-Knowledge Database Backup CLI written in Rust. It securely streams backups from your databases (PostgreSQL, MySQL, MariaDB, SQLite), compresses them using Gzip, encrypts them flawlessly using `XChaCha20Poly1305`, and uploads the ciphertexts directly to your preferred cloud storage provider (AWS S3, Cloudflare R2, or Google Drive) without ever writing plaintext to disk.

This architecture ensures total immunity from out-of-memory (OOM) errors and protects your backups entirely on the client-side.

---

## Configuration (`config.yaml`)

Veevi Sentinel is driven entirely by a single `config.yaml` file that must be placed in the same directory as the executable.

Below is an extensive guide to every available setting in the `config.yaml`.

### Complete Example

```yaml
databases:
  # Example: PostgreSQL Database
  - type: postgres
    id: "client-a-postgres"
    connection_string: "postgres://user:password@localhost:5432/client_a_db"
    cron: "0 0 2 * * * *"       # Run every day at 2:00 AM
    keep_last_n: 7              # Keep only the latest 7 backups

  # Example: MySQL or MariaDB Database
  - type: mysql
    id: "client-b-mysql"
    connection_string: "mysql://user:password@192.168.1.100:3306/client_b_db"
    # Or "mariadb://user:password@192.168.1.100:3306/client_b_db"
    cron: "0 30 3 * * * *"      # Run every day at 3:30 AM
    keep_last_n: 14

  # Example: SQLite Database
  - type: sqlite
    id: "internal-sqlite"
    path: "/var/lib/app/data.db"
    cron: "0 0 4 * * 0 *"       # Run every Sunday at 4:00 AM
    keep_last_n: 4

security:
  encryption_algo: "xchacha20poly1305"
  # MUST be exactly 32 bytes! Keep this extremely secure. If you lose this key, you lose your backups.
  encryption_key: "SuperSecretEncryptionKey32Bytes!" 

# Choose ONE storage provider (S3, R2, or Gdrive). Examples below:
storage:
  type: r2
  bucket: "sentinel-backups"
  account_id: "your_cloudflare_account_id"
  access_key: "your_r2_access_key"
  secret_key: "your_r2_secret_key"

logging:
  level: "info"                 # trace, debug, info, warn, error
  type: "daily"                 # single or daily
  file_path: "/var/log/veevi-sentinel"
  file_prefix: "sentinel"
  purge_after_days: 30

veevi:
  license_key: "your_veevi_premium_license_key"
```

---

## Configuration Reference

### 1. `databases`
An array of databases to backup. You can specify as many databases as you need across different engines.
- `type`: Must be `postgres`, `mysql`, or `sqlite`.
- `id`: A unique string identifier for this database. This ID is used to manage and restore your backups. Do not change this after backups have started.
- `connection_string` (Postgres/MySQL): The full URI. For MySQL, it must start with `mysql://` or `mariadb://`.
- `path` (SQLite only): The absolute file path to the SQLite `.db` file.
- `cron` (Optional): A 7-field Cron expression (Seconds Minutes Hours Day-of-month Month Day-of-week Year). If omitted, the database can only be backed up manually via the CLI.
- `keep_last_n` (Optional): Retention policy. The daemon will automatically prune older backups in the cloud bucket keeping only the latest `N` backups.

### 2. `security`
- `encryption_algo`: `xchacha20poly1305` (XChaCha20-Poly1305 authenticated encryption; legacy alias `chacha20poly1305` is also accepted).
- `encryption_key`: Your Master Encryption Key. **This must be exactly 32 bytes long (32 characters if standard ASCII).** Veevi Sentinel uses zero-knowledge encryption; if you lose this key, your cloud backups are completely unrecoverable.

### 3. `storage`
The storage engine to upload the backups to. Only **one** storage block can be defined.

**For Cloudflare R2:**
```yaml
storage:
  type: r2
  bucket: "my-bucket"
  account_id: "YOUR_CLOUDFLARE_ACCOUNT_ID"
  access_key: "YOUR_ACCESS_KEY"
  secret_key: "YOUR_SECRET_KEY"
```

**For AWS S3 (or MinIO):**
```yaml
storage:
  type: s3
  bucket: "my-aws-bucket"
  region: "us-east-1"
  endpoint: null # Optional. Set to "http://localhost:9000" for MinIO
  access_key: "YOUR_ACCESS_KEY"
  secret_key: "YOUR_SECRET_KEY"
```

**For Google Drive:**
```yaml
storage:
  type: gdrive
  client_secret_file: "/path/to/client_secret.json"
  token_cache_file: "/path/to/token_cache.json"
```

**How to Setup Google Drive Authentication (OAuth2 Device Flow):**

Veevi Sentinel utilizes the **OAuth2 Device Authorization Flow** for Google Drive to completely bypass the notorious storage quota limitations of standard Service Accounts. This allows you to securely back up to your personal consumer Google Drive directly from a headless server!

1. Go to the [Google Cloud Console](https://console.cloud.google.com/) and create a new Project.
2. Enable the **Google Drive API** in your project's API Library.
3. Navigate to **APIs & Services > OAuth consent screen**. 
   - *Note: If you are using a personal `@gmail.com` account, Google will skip the "User Type" selection. If you do see it, select **External**.*
   - Fill out the basic required fields. Ensure you add your personal email address under the **Test Users** section (this is crucial for unverified apps).
4. Navigate to **APIs & Services > Credentials** and click **Create Credentials > OAuth client ID**. 
   - **CRITICAL**: You MUST choose **TVs and Limited Input devices** as the application type! If you choose "Desktop app" or "Web application", Google will reject the headless Device Flow with an `Invalid client type` error.
5. Download the JSON file for this OAuth client and save it securely on your server (e.g., `/etc/veevi-sentinel/client_secret.json`).
6. Update your `config.yaml` to point to this `client_secret_file` and define a path for `token_cache_file` where the app will save its permanent refresh token.
7. **First Run Authorization**: Run `veevi-sentinel dry-run`. The CLI will print a URL and a secure code (e.g., `https://www.google.com/device`). Open that URL on your laptop/phone, enter the code, and click "Allow". The headless server will automatically detect the authorization, save the token to `token_cache_file`, and run seamlessly forever!
   - *Note: Because Veevi Sentinel uses the highly secure `drive.file` scope, it cannot see folders you created manually. It will autonomously create and manage a `VeeviSentinelBackups` folder in the root of your Google Drive.*

### 4. `logging`
- `level`: The verbosity. (`trace`, `debug`, `info`, `warn`, `error`).
- `type`: `single` (one monolithic file) or `daily` (rolling logs appended with dates).
- `file_path`: Directory to store the logs.
- `file_prefix`: The base name of the log files.
- `purge_after_days`: (Optional) Auto-delete log files older than this.

### 5. `veevi` (Optional)
Configure the Veevi premium feature to securely receive detailed webhook notifications (e.g., Slack, Discord, PagerDuty) containing backup metrics (time taken, storage size, status) on backup success or failure.
- `license_key`: Your unique Veevi License Key, obtained from the Veevi dashboard after subscribing.

---

## Command Line Interface (CLI)

Once `config.yaml` is setup, Veevi Sentinel is incredibly easy to operate.

#### Initialize Configuration
Generates a default `config.yaml` template in your current directory.
```bash
veevi-sentinel init
```

#### Run Connectivity & Health Check
Performs a live connectivity test verifying your cloud bucket credentials and checking if the database dump utilities (`pg_dump`, `mysqldump`, `sqlite3`) are securely accessible on your system PATH.
```bash
# Check all databases in the config
veevi-sentinel dry-run

# Or test a single database connection flow
veevi-sentinel dry-run client-a-postgres
```

#### Start the Daemon
Spins up the daemon in the foreground, registering all crons.
```bash
veevi-sentinel daemon
```

#### Manual On-Demand Backup
Forces an immediate backup execution outside of the cron schedule. If no target is provided, an interactive prompt will allow you to select a database (or it will auto-select if only one database is configured).
```bash
veevi-sentinel backup
veevi-sentinel backup client-a-postgres
# Or to manually backup everything in config.yaml:
veevi-sentinel backup all
```

#### List Backups
Queries your cloud storage and returns all available backup files for a database. If no database ID is provided, an interactive prompt will allow you to select a database (or it will auto-select if only one database is configured).
```bash
veevi-sentinel list
veevi-sentinel list client-a-postgres
```

#### Restore a Backup
Pulls a specific backup file from the cloud and streams it locally through the decryption and decompression layers. By default, it will write the output to a local file (`[DB_ID]_restored.sql`). You can use the `--to-file` flag to specify a custom file path, or the `--to-db` flag to securely inject it directly into your database engine!
```bash
# Writes to `client-a-postgres_restored.sql` by default
veevi-sentinel restore client-a-postgres client-a-postgres/20260724_020000.sql.gz.enc

# Writes to a specific file
veevi-sentinel restore client-a-postgres client-a-postgres/20260724_020000.sql.gz.enc --to-file /path/to/my_backup.sql

# Restores directly into the database engine
veevi-sentinel restore client-a-postgres client-a-postgres/20260724_020000.sql.gz.enc --to-db
```
