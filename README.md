<div align="center">

<br />

# ChronoBase

### The backup tool you'll wish you had *before* that incident.

<p>Local-first database backup manager for PostgreSQL and MongoDB.<br/>
Schedule · Snapshot · Restore — from a clean web UI on your own machine.</p>

<br />

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](./LICENSE)
[![Node](https://img.shields.io/badge/Node.js-18%2B-brightgreen?style=flat-square)](https://nodejs.org)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg?style=flat-square)](./CONTRIBUTING.md)
[![Made with ❤️](https://img.shields.io/badge/made%20with-%E2%9D%A4%EF%B8%8F-red?style=flat-square)](https://github.com/Subham-Maity)

<br />

</div>

---

## 😮 You ever done this?

```bash
$ psql production_db
psql> DROP TABLE users;   # wait no no NO
```

Or shipped a migration that nuked 3 months of data.  
Or restored the wrong dump to the wrong database at 1am.  
Or watched your co-founder's face go pale because staging pointed to prod.

**ChronoBase exists for that exact moment.**  
It runs quietly in the background so that when everything goes sideways —  
and it will — you can breathe out, open a tab, and click *Restore*.

---

## 🎬 Preview

<div align="center">

<!-- Replace this with your demo GIF -->
![ChronoBase Demo](assets/demo.gif)

> *Record a quick demo: add a connection → run a backup → restore it.  
> Tools: [ScreenToGif](https://www.screentogif.com/) (Windows) · [Kap](https://getkap.co/) (macOS)*

</div>

---

## ✨ What it does

| | Feature | Details |
|---|---|---|
| 🗄️ | **Multi-database** | PostgreSQL and MongoDB in one place |
| 📁 | **Group connections** | Organise by project — `Production`, `Staging`, `Dev` |
| ⚡ | **One-click backup** | Single DB, entire group, or all at once |
| ⏰ | **Scheduled backups** | Cron-based — daily at 2am, every 6h, whatever you need |
| 📜 | **Backup history** | Every snapshot with size, status, and timestamp |
| ↩️ | **Restore / Push** | Same DB or push to a completely different target |
| 💾 | **Local storage** | `.sql` files and MongoDB BSON — your files, your disk |
| 🔍 | **Tool detection** | Tells you what's missing and exactly how to fix it |
| 📱 | **Responsive UI** | Works on desktop and mobile |
| 🔒 | **Zero cloud** | Nothing leaves your machine. Ever. |

---

## 🏗️ Architecture

```
chronobase/
├── server.js                   ← Express entry point
├── src/
│   ├── database.js             ← JSON persistence (no native deps)
│   ├── tools.js                ← pg_dump / mongodump auto-discovery
│   ├── backup.js               ← Backup & restore engine
│   ├── scheduler.js            ← Cron schedule manager
│   └── routes/
│       ├── connections.js      ← CRUD + per-connection backup
│       ├── backups.js          ← History, delete, restore
│       ├── schedules.js        ← Schedule CRUD + toggle
│       └── system.js           ← Stats, tool check, groups
└── public/
    ├── index.html
    ├── css/style.css
    └── js/
        ├── app.js              ← Router + event bindings
        ├── api.js              ← Fetch wrapper
        ├── utils.js            ← Toast, modals, formatters
        └── pages/              ← One JS file per UI page
            ├── dashboard.js
            ├── connections.js
            ├── backups.js
            ├── schedules.js
            └── setup.js
```

No ORM. No native add-ons. No compilation step. Data lives in `chronobase-data.json` — readable, portable, yours.

---

## 🔧 Prerequisites

ChronoBase needs **Node.js 18+** to run. The backup engine needs the official CLI tools from Postgres and MongoDB. Both are free and lightweight.

### 1. Node.js

Download the LTS installer → [nodejs.org/en/download](https://nodejs.org/en/download)

```bash
node --version   # must print v18.x or later
```

---

### 2. `pg_dump` — for PostgreSQL backups

<details>
<summary><strong>🪟 Windows</strong></summary>

1. Download from [postgresql.org/download/windows](https://postgresql.org/download/windows)
2. Run installer → select **"Command Line Tools"** only
3. Add to PATH: `C:\Program Files\PostgreSQL\18\bin`
   - Win key → search **"Environment Variables"** → System Path → New → paste → OK
4. Open a **new** terminal and verify:

```bash
pg_dump --version
# pg_dump (PostgreSQL) 18.x
```
</details>

<details>
<summary><strong>🍎 macOS</strong></summary>

```bash
brew install libpq
brew link --force libpq
pg_dump --version
```
</details>

<details>
<summary><strong>🐧 Linux (Ubuntu / Debian)</strong></summary>

```bash
sudo apt update && sudo apt install postgresql-client
pg_dump --version
```
</details>

---

### 3. `mongodump` — for MongoDB backups

<details>
<summary><strong>🪟 Windows</strong></summary>

1. Download the MSI from [mongodb.com/try/download/database-tools](https://mongodb.com/try/download/database-tools)
2. Select: Platform → **Windows**, Package → **MSI**
3. Run installer — it adds tools to PATH automatically

```bash
mongodump --version
```
</details>

<details>
<summary><strong>🍎 macOS</strong></summary>

```bash
brew tap mongodb/brew
brew install mongodb-database-tools
mongodump --version
```
</details>

<details>
<summary><strong>🐧 Linux (Ubuntu / Debian)</strong></summary>

```bash
sudo apt install mongo-tools
# Or download the tarball from mongodb.com/try/download/database-tools
# Extract and add the bin/ folder to your PATH
mongodump --version
```
</details>

> **Atlas users:** `mongodb+srv://` URLs work directly — mongodump handles Atlas natively, no extra config needed.

---

## 🚀 Installation

```bash
git clone https://github.com/Subham-Maity/chronobase.git
cd chronobase
npm install
```

---

## ▶️ Running

### Windows
```bash
# Option 1 — double-click START.bat
# Checks deps, starts server, opens browser automatically

# Option 2 — terminal
node server.js
```

### macOS / Linux
```bash
node server.js
# or
npm start
```

Open **http://localhost:3420** in your browser.

Change the port:
```bash
PORT=8080 node server.js
```

---

## 🤖 Auto-start on boot

Run ChronoBase silently in the background — backups happen whether you think about it or not.

<details>
<summary><strong>🪟 Windows — Startup folder</strong></summary>

1. Press `Win + R` → type `shell:startup` → Enter
2. Right-click `START.bat` → **Create Shortcut**
3. Move the shortcut into the folder that opened

ChronoBase will launch automatically every time Windows boots.
</details>

<details>
<summary><strong>🍎 macOS — launchd</strong></summary>

Create `~/Library/LaunchAgents/com.chronobase.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>              <string>com.chronobase</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/local/bin/node</string>
    <string>/full/path/to/chronobase/server.js</string>
  </array>
  <key>WorkingDirectory</key>   <string>/full/path/to/chronobase</string>
  <key>RunAtLoad</key>          <true/>
  <key>KeepAlive</key>          <true/>
  <key>StandardOutPath</key>    <string>/tmp/chronobase.log</string>
  <key>StandardErrorPath</key>  <string>/tmp/chronobase.err</string>
</dict>
</plist>
```

```bash
launchctl load ~/Library/LaunchAgents/com.chronobase.plist
```

To stop: `launchctl unload ~/Library/LaunchAgents/com.chronobase.plist`
</details>

<details>
<summary><strong>🐧 Linux — systemd</strong></summary>

Create `/etc/systemd/system/chronobase.service`:

```ini
[Unit]
Description=ChronoBase Backup Manager
After=network.target

[Service]
Type=simple
User=YOUR_LINUX_USERNAME
WorkingDirectory=/path/to/chronobase
ExecStart=/usr/bin/node server.js
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable chronobase
sudo systemctl start chronobase

# Check status
sudo systemctl status chronobase

# Live logs
journalctl -u chronobase -f
```
</details>

---

## 🔌 Connection URL formats

```bash
# PostgreSQL
postgresql://username:password@host:5432/database_name

# MongoDB (local / self-hosted)
mongodb://username:password@host:27017/database_name

# MongoDB Atlas (cloud)
mongodb+srv://username:password@cluster0.xxxxx.mongodb.net/database_name
```

---

## 📦 Where data lives

```
chronobase/
├── chronobase-data.json              ← connections, backup history, schedules
└── backups/
    └── Production/
        ├── MyApp_20260322_020001.sql         ← PostgreSQL dump
        └── Analytics_20260322_020015/        ← MongoDB BSON folder
```

Everything stays on your disk. Zero cloud. Zero tracking. Zero surprise bills.

Moving to another machine? Copy the folder. Done.

---

## 📡 API reference

ChronoBase exposes a REST API — build scripts, CI hooks, or your own integrations on top.

```
System
  GET  /api/system/stats                    Dashboard statistics
  GET  /api/system/tools                    Tool detection (pg_dump, mongodump)
  GET  /api/system/groups                   All group names

Connections
  GET    /api/connections                   List all (sorted by group + name)
  POST   /api/connections                   Create
  PUT    /api/connections/:id               Update
  DELETE /api/connections/:id               Delete
  POST   /api/connections/:id/backup        Backup single connection
  POST   /api/connections/backup-group      Backup by group { group_name }

Backups
  GET    /api/backups                       List (query: ?group= &type= &status=)
  DELETE /api/backups/:id                   Delete record + file from disk
  POST   /api/backups/:id/restore           Restore to { target_url }

Schedules
  GET    /api/schedules                     List all
  POST   /api/schedules                     Create { name, group_name, cron_expr }
  PUT    /api/schedules/:id/toggle          Pause / resume
  DELETE /api/schedules/:id                 Delete
```

---

## 🗺️ Roadmap

- [ ] Encryption for stored connection URLs (AES-256)
- [ ] Offsite upload — S3, Cloudflare R2, Backblaze B2
- [ ] Backup compression — gzip
- [ ] Failure notifications — Slack, Discord, email
- [ ] MySQL / MariaDB support
- [ ] Docker image — `docker run -p 3420:3420 chronobase`
- [ ] Multi-user support with basic auth
- [ ] CLI mode — `chronobase backup --group Production`
- [ ] Backup diff — incremental snapshots

Contributions are welcome — see [CONTRIBUTING.md](./CONTRIBUTING.md).

---

## 🤝 Contributing

Bug reports, feature requests, and PRs are all welcome.

Please: no commented-out code, keep modules single-responsibility, test the flow manually before opening a PR. See [CONTRIBUTING.md](./CONTRIBUTING.md) for full guidelines.

---

## 👤 Author

<div align="center">

**Subham Maity**

[github.com/Subham-Maity](https://github.com/Subham-Maity)

<br/>

*Built because juggling `pg_dump` commands across a dozen projects at 2am is not fun.*  
*If this ever saves you from a data disaster — give it a ⭐ and sleep better tonight.*

</div>

---

## 📄 License

[MIT](./LICENSE) — free to use, fork, modify, and ship.

---

<div align="center">
<sub>Made with too much coffee and a healthy fear of data loss.</sub>
</div>
