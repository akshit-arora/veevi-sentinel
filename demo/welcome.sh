#!/bin/bash
set -e

# Initialize sample database if not already present
if [ ! -f /demo/demo.db ]; then
    sqlite3 /demo/demo.db < /demo/init-db.sql
fi

mkdir -p /demo/backups /demo/logs

cat << 'EOF'
================================================================================
  🛡️  VEEVI SENTINEL DEMO SANDBOX — Zero-Risk Workstation Trial
================================================================================

  You are in an isolated, pre-configured demo sandbox.
  Pre-loaded:
    • Database:  /demo/demo.db (SQLite with sample customers & orders)
    • Storage:   /demo/backups (Local directory encrypted storage)
    • Key:       XChaCha20-Poly1305 (32-byte demo encryption key)

  Try running these commands right now:

    1. veevi-sentinel dry-run
       -> Tests database access, dump tool, and storage permissions.

    2. veevi-sentinel backup all
       -> Dumps, gzips, and encrypts /demo/demo.db into /demo/backups.

    3. veevi-sentinel list demo-sqlite
       -> Lists your encrypted .sql.gz.enc backup artifacts.

    4. veevi-sentinel restore demo-sqlite <BACKUP_FILE> --to-file /demo/restored.sql
       -> Decrypts and decompresses the backup on the fly to inspect plaintext.

================================================================================
EOF

exec /bin/bash "$@"
