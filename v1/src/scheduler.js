const cron   = require('node-cron');
const db     = require('./database');
const backup = require('./backup');

const activeTasks = new Map();

function start(schedule) {
  const { id, enabled, cron_expr, name, group_name } = schedule;

  if (activeTasks.has(id)) {
    activeTasks.get(id).stop();
    activeTasks.delete(id);
  }

  if (!enabled || !cron.validate(cron_expr)) return;

  const task = cron.schedule(cron_expr, async () => {
    console.log(`[ChronoBase] Schedule "${name}" triggered`);

    const state = db.read();
    const conns = group_name === 'All'
      ? state.connections
      : state.connections.filter(c => c.group_name === group_name);

    for (const conn of conns) {
      const result = await backup.backupWithErrorHandling(conn);
      const label  = result.ok ? 'OK' : 'FAILED';
      console.log(`[ChronoBase]   ${label}: ${conn.name}`);
    }

    const fresh = db.read();
    const idx   = fresh.schedules.findIndex(s => s.id === id);
    if (idx >= 0) {
      fresh.schedules[idx].last_run = db.timestamp();
      db.write(fresh);
    }
  });

  activeTasks.set(id, task);
}

function stop(id) {
  if (activeTasks.has(id)) {
    activeTasks.get(id).stop();
    activeTasks.delete(id);
  }
}

function reload() {
  activeTasks.forEach((task, id) => { task.stop(); activeTasks.delete(id); });
  db.read().schedules.filter(s => s.enabled).forEach(start);
}

module.exports = { start, stop, reload };
