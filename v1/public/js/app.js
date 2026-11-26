import { api }                             from './api.js';
import { toast, openModal, closeModal }    from './utils.js';
import { render as renderDashboard, backupAll }                from './pages/dashboard.js';
import { render as renderConnections, renderTable as renderConnTable, openAdd as openAddConn, saveConnection, backupGroup, updateUrlHint } from './pages/connections.js';
import { render as renderBackups, renderTable as renderBkTable, setMode, doRestore }   from './pages/backups.js';
import { render as renderSchedules, openAdd as openAddSched, saveSchedule, setCron, populateGroupSelect } from './pages/schedules.js';
import { render as renderSetup }           from './pages/setup.js';

const PAGES = ['dashboard','connections','backups','schedules','setup','danger'];

async function navigate(page) {
  PAGES.forEach(p => {
    document.getElementById(`page-${p}`).classList.toggle('active', p === page);
    document.querySelector(`[data-nav="${p}"]`)?.classList.toggle('active', p === page);
  });
  if (page === 'dashboard')   await renderDashboard();
  if (page === 'connections') await renderConnections();
  if (page === 'backups')     await renderBackups();
  if (page === 'schedules')   await renderSchedules();
  if (page === 'setup')       await renderSetup();
}

async function checkTools() {
  try {
    const s = await api.get('/api/system/tools');
    document.getElementById('sidebar-pg-dot').className        = 'pill-dot ' + (s.pg    ? 'ok' : 'err');
    document.getElementById('sidebar-pg-label').textContent    = s.pg    ? 'pg_dump ready'    : 'pg_dump missing';
    document.getElementById('sidebar-mongo-dot').className     = 'pill-dot ' + (s.mongo ? 'ok' : 'err');
    document.getElementById('sidebar-mongo-label').textContent = s.mongo ? 'mongodump ready' : 'mongodump missing';
  } catch {}
}

function bindConfirmInput(inputId, phrase, btnId) {
  const input = document.getElementById(inputId);
  const btn   = document.getElementById(btnId);
  input.addEventListener('input', () => {
    const match   = input.value === phrase;
    btn.disabled  = !match;
    input.classList.toggle('valid', match);
  });
}

function bindDangerZone() {
  document.getElementById('open-delete-backups-btn').addEventListener('click', () => {
    document.getElementById('confirm-delete-backups').value = '';
    document.getElementById('confirm-delete-backups-btn').disabled = true;
    openModal('delete-backups-modal');
  });

  document.getElementById('open-delete-all-btn').addEventListener('click', () => {
    document.getElementById('confirm-delete-all').value = '';
    document.getElementById('confirm-delete-all-btn').disabled = true;
    openModal('delete-all-modal');
  });

  document.getElementById('cancel-delete-backups-btn').addEventListener('click', () => closeModal('delete-backups-modal'));
  document.getElementById('cancel-delete-all-btn').addEventListener('click',     () => closeModal('delete-all-modal'));

  document.getElementById('confirm-delete-backups-btn').addEventListener('click', async () => {
    try {
      await api.delete('/api/system/all-backups', { confirm: 'DELETE BACKUPS' });
      toast('All backup files deleted', 'success');
      closeModal('delete-backups-modal');
      navigate('dashboard');
    } catch (e) { toast(e.message, 'error'); }
  });

  document.getElementById('confirm-delete-all-btn').addEventListener('click', async () => {
    try {
      await api.delete('/api/system/all-data', { confirm: 'DELETE EVERYTHING' });
      toast('ChronoBase has been reset', 'info');
      closeModal('delete-all-modal');
      navigate('dashboard');
    } catch (e) { toast(e.message, 'error'); }
  });

  bindConfirmInput('confirm-delete-backups', 'DELETE BACKUPS',    'confirm-delete-backups-btn');
  bindConfirmInput('confirm-delete-all',     'DELETE EVERYTHING', 'confirm-delete-all-btn');
}

async function init() {
  document.querySelectorAll('[data-nav]').forEach(btn =>
    btn.addEventListener('click', () => navigate(btn.dataset.nav))
  );
  document.querySelectorAll('[data-goto]').forEach(el =>
    el.addEventListener('click', () => navigate(el.dataset.goto))
  );

  document.getElementById('backup-all-btn').addEventListener('click', e => backupAll(e.currentTarget));
  document.getElementById('add-conn-btn').addEventListener('click', openAddConn);
  document.getElementById('conn-group-filter').addEventListener('change', renderConnTable);
  document.getElementById('conn-type-filter').addEventListener('change', renderConnTable);
  document.getElementById('backup-group-btn').addEventListener('click', backupGroup);
  document.getElementById('save-conn-btn').addEventListener('click', saveConnection);
  document.getElementById('cancel-conn-btn').addEventListener('click', () => closeModal('conn-modal'));
  document.getElementById('f-type').addEventListener('change', updateUrlHint);

  document.getElementById('bk-group-filter').addEventListener('change', renderBkTable);
  document.getElementById('bk-type-filter').addEventListener('change', renderBkTable);
  document.getElementById('bk-status-filter').addEventListener('change', renderBkTable);
  document.getElementById('r-btn-same').addEventListener('click',  () => setMode('same'));
  document.getElementById('r-btn-other').addEventListener('click', () => setMode('other'));
  document.getElementById('r-confirm-btn').addEventListener('click', doRestore);
  document.getElementById('cancel-restore-btn').addEventListener('click', () => closeModal('restore-modal'));

  document.getElementById('add-sched-btn').addEventListener('click', async () => {
    try { populateGroupSelect(await api.get('/api/system/groups')); } catch {}
    openAddSched();
  });
  document.getElementById('save-sched-btn').addEventListener('click', saveSchedule);
  document.getElementById('cancel-sched-btn').addEventListener('click', () => closeModal('sched-modal'));
  document.querySelectorAll('[data-cron]').forEach(c =>
    c.addEventListener('click', () => setCron(c.dataset.cron))
  );

  document.getElementById('recheck-tools-btn').addEventListener('click', renderSetup);

  bindDangerZone();
  await checkTools();
  await navigate('dashboard');
}

init();
