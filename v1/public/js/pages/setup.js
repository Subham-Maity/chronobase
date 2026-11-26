import { api } from '../api.js';

export async function render() {
  const status = await api.get('/api/system/tools');
  updateStatus('pg',    status.pg,    status.pgPath,    'pg_dump');
  updateStatus('mongo', status.mongo, status.mongoPath, 'mongodump');

  const sidebar = {
    pg:    { dot: document.getElementById('sidebar-pg-dot'),    label: document.getElementById('sidebar-pg-label') },
    mongo: { dot: document.getElementById('sidebar-mongo-dot'), label: document.getElementById('sidebar-mongo-label') },
  };

  sidebar.pg.dot.className    = 'pill-dot ' + (status.pg    ? 'ok' : 'err');
  sidebar.pg.label.textContent = status.pg    ? 'pg_dump ready'    : 'pg_dump missing';
  sidebar.mongo.dot.className  = 'pill-dot ' + (status.mongo ? 'ok' : 'err');
  sidebar.mongo.label.textContent = status.mongo ? 'mongodump ready' : 'mongodump missing';
}

function updateStatus(key, ok, toolPath, label) {
  const dot  = document.getElementById(`${key}-setup-dot`);
  const text = document.getElementById(`${key}-setup-text`);
  const wrap = document.getElementById(`${key}-setup-status`);
  dot.className    = 'pill-dot ' + (ok ? 'ok' : 'err');
  wrap.className   = 'setup-status ' + (ok ? 'ok' : 'err');
  text.textContent = ok ? `Found: ${toolPath}` : `Not found — follow the steps below`;
}
