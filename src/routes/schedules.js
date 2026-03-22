const express   = require('express');
const cron      = require('node-cron');
const db        = require('../database');
const scheduler = require('../scheduler');

const router = express.Router();

router.get('/', (_req, res) => {
  res.json(db.read().schedules);
});

router.post('/', (req, res) => {
  const { name, group_name, cron_expr, enabled } = req.body;
  if (!name?.trim()) return res.status(400).json({ error: 'name is required' });
  if (!cron.validate(cron_expr)) return res.status(400).json({ error: 'Invalid cron expression' });

  const state    = db.read();
  const schedule = {
    id:         state._seq.s++,
    name:       name.trim(),
    group_name: group_name || 'All',
    cron_expr,
    enabled:    enabled !== false,
    last_run:   null,
    created_at: db.timestamp(),
  };
  state.schedules.unshift(schedule);
  db.write(state);
  scheduler.start(schedule);
  res.status(201).json(schedule);
});

router.put('/:id/toggle', (req, res) => {
  const state = db.read();
  const idx   = state.schedules.findIndex(s => s.id === parseInt(req.params.id));
  if (idx < 0) return res.status(404).json({ error: 'Schedule not found' });

  state.schedules[idx].enabled = !state.schedules[idx].enabled;
  db.write(state);
  scheduler.start(state.schedules[idx]);
  res.json(state.schedules[idx]);
});

router.delete('/:id', (req, res) => {
  const id    = parseInt(req.params.id);
  const state = db.read();
  scheduler.stop(id);
  state.schedules = state.schedules.filter(s => s.id !== id);
  db.write(state);
  res.json({ ok: true });
});

module.exports = router;
