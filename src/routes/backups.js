const express = require('express');
const fs      = require('fs');
const db      = require('../database');
const engine  = require('../backup');

const router = express.Router();

router.get('/', (req, res) => {
  const { group, type, status } = req.query;
  let list = db.read().backups;
  if (group)  list = list.filter(b => b.group_name === group);
  if (type)   list = list.filter(b => b.type === type);
  if (status) list = list.filter(b => b.status === status);
  res.json(list.slice(0, 300));
});

router.delete('/:id', (req, res) => {
  const state = db.read();
  const entry = state.backups.find(b => b.id === parseInt(req.params.id));

  if (entry?.file_path && fs.existsSync(entry.file_path)) {
    try {
      const stat = fs.statSync(entry.file_path);
      if (stat.isDirectory()) {
        fs.rmSync(entry.file_path, { recursive: true, force: true });
      } else {
        fs.unlinkSync(entry.file_path);
      }
    } catch {
      // file deletion best-effort
    }
  }

  state.backups = state.backups.filter(b => b.id !== parseInt(req.params.id));
  db.write(state);
  res.json({ ok: true });
});

router.post('/:id/restore', async (req, res) => {
  const { target_url } = req.body;
  if (!target_url?.trim()) {
    return res.status(400).json({ error: 'target_url is required' });
  }

  const state = db.read();
  const entry = state.backups.find(b => b.id === parseInt(req.params.id));
  if (!entry) return res.status(404).json({ error: 'Backup not found' });

  try {
    await engine.restore(entry, target_url.trim());
    res.json({ ok: true });
  } catch (err) {
    res.status(500).json({ error: err.message });
  }
});

module.exports = router;
