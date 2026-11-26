const express  = require('express');
const { exec } = require('child_process');
const db       = require('../database');
const backup   = require('../backup');
const tools    = require('../tools');

const router = express.Router();

router.get('/', (_req, res) => {
  const sorted = [...db.read().connections].sort((a, b) =>
    `${a.group_name}${a.name}`.localeCompare(`${b.group_name}${b.name}`)
  );
  res.json(sorted);
});

router.post('/', (req, res) => {
  const { name, group_name, type, url, notes } = req.body;
  if (!name?.trim() || !type || !url?.trim()) {
    return res.status(400).json({ error: 'name, type, and url are required' });
  }
  const state = db.read();
  const conn  = {
    id:         state._seq.c++,
    name:       name.trim(),
    group_name: (group_name || 'Default').trim(),
    type,
    url:        url.trim(),
    notes:      (notes || '').trim(),
    created_at: db.timestamp(),
  };
  state.connections.push(conn);
  db.write(state);
  res.status(201).json(conn);
});

router.put('/:id', (req, res) => {
  const state = db.read();
  const idx   = state.connections.findIndex(c => c.id === parseInt(req.params.id));
  if (idx < 0) return res.status(404).json({ error: 'Connection not found' });
  const { name, group_name, type, url, notes } = req.body;
  state.connections[idx] = {
    ...state.connections[idx],
    name: name.trim(),
    group_name: (group_name || 'Default').trim(),
    type,
    url: url.trim(),
    notes: (notes || '').trim(),
  };
  db.write(state);
  res.json(state.connections[idx]);
});

router.delete('/:id', (req, res) => {
  const state = db.read();
  state.connections = state.connections.filter(c => c.id !== parseInt(req.params.id));
  db.write(state);
  res.json({ ok: true });
});

router.post('/:id/backup', async (req, res) => {
  const conn = db.read().connections.find(c => c.id === parseInt(req.params.id));
  if (!conn) return res.status(404).json({ error: 'Connection not found' });
  const result = await backup.backupWithErrorHandling(conn);
  if (!result.ok) return res.status(500).json({ error: result.error });
  res.json({ ok: true, backup: result.entry });
});

router.post('/backup-group', async (req, res) => {
  const { group_name } = req.body;
  const state = db.read();
  const conns = group_name === 'All'
    ? state.connections
    : state.connections.filter(c => c.group_name === group_name);
  const results = [];
  for (const conn of conns) results.push(await backup.backupWithErrorHandling(conn));
  res.json(results);
});

router.post('/:id/wipe', async (req, res) => {
  const conn = db.read().connections.find(c => c.id === parseInt(req.params.id));
  if (!conn) return res.status(404).json({ error: 'Connection not found' });

  try {
    if (conn.type === 'postgres') {
      const psql = await tools.find(tools.PSQL_CANDIDATES);
      if (!psql) throw new Error('psql not found. Install PostgreSQL tools first.');
      const sql = 'DROP SCHEMA public CASCADE; CREATE SCHEMA public; GRANT ALL ON SCHEMA public TO public;';
      await new Promise((resolve, reject) => {
        exec(`"${psql}" "${conn.url}" -c "${sql}"`, { timeout: 60000, windowsHide: true }, (err, _out, stderr) => {
          if (err) reject(new Error(stderr || err.message));
          else resolve();
        });
      });
    } else {
      const mongosh = await tools.find(['mongosh', 'mongo']);
      if (!mongosh) throw new Error('mongosh not found. Install MongoDB Shell first.');
      const script = 'db.getCollectionNames().forEach(c => db[c].drop())';
      await new Promise((resolve, reject) => {
        exec(`"${mongosh}" "${conn.url}" --eval "${script}" --quiet`, { timeout: 60000, windowsHide: true }, (err, _out, stderr) => {
          if (err) reject(new Error(stderr || err.message));
          else resolve();
        });
      });
    }
    res.json({ ok: true });
  } catch (err) {
    res.status(500).json({ error: err.message });
  }
});

module.exports = router;
