const express = require('express');
const fs      = require('fs');
const db      = require('../database');
const tools   = require('../tools');
const { BACKUP_DIR } = require('../backup');

const router = express.Router();

router.get('/stats', (_req, res) => {
  const state = db.read();
  res.json({
    totalConns:   state.connections.length,
    totalBackups: state.backups.length,
    successCount: state.backups.filter(b => b.status === 'success').length,
    totalSize:    state.backups.reduce((s, b) => s + (b.file_size || 0), 0),
    lastBackup:   state.backups[0]?.created_at ?? null,
    activeScheds: state.schedules.filter(s => s.enabled).length,
  });
});

router.get('/tools', async (_req, res) => {
  res.json(await tools.status());
});

router.get('/groups', (_req, res) => {
  const groups = [...new Set(db.read().connections.map(c => c.group_name))];
  res.json(groups.sort());
});

router.delete('/all-data', (req, res) => {
  if (req.body?.confirm !== 'DELETE EVERYTHING') {
    return res.status(400).json({ error: 'Confirmation phrase does not match' });
  }
  if (fs.existsSync(BACKUP_DIR)) {
    fs.rmSync(BACKUP_DIR, { recursive: true, force: true });
    fs.mkdirSync(BACKUP_DIR, { recursive: true });
  }
  db.write({ connections: [], backups: [], schedules: [], _seq: { c: 1, b: 1, s: 1 } });
  res.json({ ok: true });
});

router.delete('/all-backups', (req, res) => {
  if (req.body?.confirm !== 'DELETE BACKUPS') {
    return res.status(400).json({ error: 'Confirmation phrase does not match' });
  }
  if (fs.existsSync(BACKUP_DIR)) {
    fs.rmSync(BACKUP_DIR, { recursive: true, force: true });
    fs.mkdirSync(BACKUP_DIR, { recursive: true });
  }
  const state   = db.read();
  state.backups = [];
  db.write(state);
  res.json({ ok: true });
});

module.exports = router;
