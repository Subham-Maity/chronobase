const fs   = require('fs');
const path = require('path');

const DATA_FILE = path.join(process.cwd(), 'chronobase-data.json');

const DEFAULT_STATE = {
  connections: [],
  backups:     [],
  schedules:   [],
  _seq:        { c: 1, b: 1, s: 1 },
};

function read() {
  try {
    if (fs.existsSync(DATA_FILE)) {
      return JSON.parse(fs.readFileSync(DATA_FILE, 'utf8'));
    }
  } catch {
    // corrupted file — start fresh
  }
  return structuredClone(DEFAULT_STATE);
}

function write(data) {
  fs.writeFileSync(DATA_FILE, JSON.stringify(data, null, 2));
}

function nextId(db, key) {
  const id = db._seq[key]++;
  write(db);
  return id;
}

function timestamp() {
  return new Date().toLocaleString('sv').replace('T', ' ');
}

module.exports = { read, write, nextId, timestamp };
