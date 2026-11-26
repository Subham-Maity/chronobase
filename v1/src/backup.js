const { exec } = require('child_process');
const path      = require('path');
const fs        = require('fs');
const db        = require('./database');
const tools     = require('./tools');

const BACKUP_DIR = path.join(process.cwd(), 'backups');

function ensureDir(dir) {
  if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });
}

function slugify(str) {
  return str.replace(/[^a-z0-9]/gi, '_');
}

function makeTimestamp() {
  const d = new Date();
  return [
    d.getFullYear(),
    String(d.getMonth() + 1).padStart(2, '0'),
    String(d.getDate()).padStart(2, '0'),
    '_',
    String(d.getHours()).padStart(2, '0'),
    String(d.getMinutes()).padStart(2, '0'),
    String(d.getSeconds()).padStart(2, '0'),
  ].join('');
}

function sizeOf(target) {
  try {
    if (!fs.existsSync(target)) return 0;
    const stat = fs.statSync(target);
    if (!stat.isDirectory()) return stat.size;
    return fs.readdirSync(target, { withFileTypes: true }).reduce((total, entry) => {
      return total + sizeOf(path.join(target, entry.name));
    }, 0);
  } catch {
    return 0;
  }
}

function runCommand(command) {
  return new Promise((resolve, reject) => {
    exec(command, { timeout: 600_000, windowsHide: true }, (err, _stdout, stderr) => {
      if (err) reject(new Error(stderr || err.message));
      else resolve();
    });
  });
}

async function backup(connection) {
  const { id, name, group_name, type, url } = connection;

  ensureDir(BACKUP_DIR);
  const groupDir = path.join(BACKUP_DIR, slugify(group_name));
  ensureDir(groupDir);

  const stamp    = makeTimestamp();
  const baseName = `${slugify(name)}_${stamp}`;

  let filePath, command;

  if (type === 'postgres') {
    const tool = await tools.find(tools.PG_DUMP_CANDIDATES);
    if (!tool) throw new Error('pg_dump not found. See the Setup tab for installation instructions.');
    filePath = path.join(groupDir, `${baseName}.sql`);
    command  = `"${tool}" "${url}" -f "${filePath}"`;
  } else {
    const tool = await tools.find(tools.MONGODUMP_CANDIDATES);
    if (!tool) throw new Error('mongodump not found. See the Setup tab for installation instructions.');
    filePath = path.join(groupDir, baseName);
    command  = `"${tool}" --uri="${url}" --out="${filePath}"`;
  }

  await runCommand(command);

  const state = db.read();
  const entry = {
    id:              state._seq.b++,
    connection_id:   id,
    connection_name: name,
    group_name,
    type,
    file_path:  filePath,
    file_size:  sizeOf(filePath),
    status:     'success',
    error_msg:  null,
    created_at: db.timestamp(),
  };
  state.backups.unshift(entry);
  if (state.backups.length > 500) state.backups = state.backups.slice(0, 500);
  db.write(state);
  return entry;
}

async function backupWithErrorHandling(connection) {
  try {
    return { ok: true, entry: await backup(connection) };
  } catch (err) {
    const state = db.read();
    const entry = {
      id:              state._seq.b++,
      connection_id:   connection.id,
      connection_name: connection.name,
      group_name:      connection.group_name,
      type:            connection.type,
      file_path:       '',
      file_size:       0,
      status:          'failed',
      error_msg:       err.message,
      created_at:      db.timestamp(),
    };
    state.backups.unshift(entry);
    if (state.backups.length > 500) state.backups = state.backups.slice(0, 500);
    db.write(state);
    return { ok: false, error: err.message };
  }
}

async function restore(backupEntry, targetUrl) {
  if (!fs.existsSync(backupEntry.file_path)) {
    throw new Error('Backup file not found on disk. It may have been moved or deleted.');
  }

  let command;

  if (backupEntry.type === 'postgres') {
    const tool = await tools.find(tools.PSQL_CANDIDATES);
    if (!tool) throw new Error('psql not found. Install PostgreSQL tools to enable restore.');
    command = `"${tool}" "${targetUrl}" -f "${backupEntry.file_path}"`;
  } else {
    const tool = await tools.find(tools.MONGORESTORE_CANDIDATES);
    if (!tool) throw new Error('mongorestore not found. Install MongoDB Database Tools to enable restore.');
    command = `"${tool}" --uri="${targetUrl}" "${backupEntry.file_path}"`;
  }

  await runCommand(command);
}

module.exports = { backup, backupWithErrorHandling, restore, BACKUP_DIR };
