import { api }                                                                   from '../api.js';
import { fmtSize, fmtDate, fmtRelative, typeBadge, statusBadge, groupBadge, toast, openModal, closeModal, spinnerBtn, emptyState, showConfirm } from '../utils.js';

let backups     = [];
let connections = [];
let restoreEntry = null;
let restoreMode  = 'same';

export async function render() {
  [backups, connections] = await Promise.all([
    api.get('/api/backups'),
    api.get('/api/connections'),
  ]);
  renderTable();
}

export function renderTable() {
  const gf   = document.getElementById('bk-group-filter').value;
  const tf   = document.getElementById('bk-type-filter').value;
  const sf   = document.getElementById('bk-status-filter').value;
  const list = backups.filter(b =>
    (!gf || b.group_name === gf) &&
    (!tf || b.type === tf) &&
    (!sf || b.status === sf)
  );
  const wrap = document.getElementById('bk-table-wrap');

  if (!list.length) {
    wrap.innerHTML = emptyState('◎', 'No backups match your filter');
    return;
  }

  const rows = list.map(b => `
    <tr>
      <td class="cell-name">${b.connection_name}</td>
      <td>${groupBadge(b.group_name)}</td>
      <td>${typeBadge(b.type)}</td>
      <td>${statusBadge(b.status)}</td>
      <td class="cell-size">${fmtSize(b.file_size)}</td>
      <td class="cell-time" title="${fmtDate(b.created_at)}">${fmtRelative(b.created_at)}</td>
      <td>
        <div class="actions">
          ${b.status === 'success' ? `<button class="btn btn-ghost btn-sm" data-restore="${b.id}">↩ Restore</button>` : ''}
          <button class="btn btn-danger btn-sm" data-delete="${b.id}">✕</button>
        </div>
      </td>
    </tr>`).join('');

  wrap.innerHTML = `
    <table>
      <thead><tr><th>Database</th><th>Group</th><th>Type</th><th>Status</th><th>Size</th><th>When</th><th>Actions</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>`;

  wrap.querySelectorAll('[data-restore]').forEach(btn => btn.addEventListener('click', () => openRestore(parseInt(btn.dataset.restore))));
  wrap.querySelectorAll('[data-delete]').forEach(btn  => btn.addEventListener('click', () => deleteBackup(parseInt(btn.dataset.delete))));
}

async function deleteBackup(id) {
  const b = backups.find(x => x.id === id);
  const confirmed = await showConfirm({
    icon:         '◎',
    iconType:     'red',
    title:        'Delete this backup?',
    body:         `The snapshot for <strong>${b?.connection_name || 'this database'}</strong> taken <strong>${fmtRelative(b?.created_at)}</strong> will be permanently removed from disk.<br><br>
                  <span style="color:var(--text-lo);font-size:12px">This cannot be undone. The connection itself is not affected.</span>`,
    confirmLabel: 'Delete Backup',
    confirmClass: 'btn-danger',
  });
  if (!confirmed) return;
  await api.delete(`/api/backups/${id}`);
  toast('Backup deleted', 'info');
  render();
}

function openRestore(id) {
  restoreEntry = backups.find(b => b.id === id);
  if (!restoreEntry) return;
  const conn = connections.find(c => c.id === restoreEntry.connection_id);
  document.getElementById('r-info').innerHTML = `
    <div class="info-row"><span class="info-label">Database</span><span class="info-val">${restoreEntry.connection_name}</span></div>
    <div class="info-row"><span class="info-label">Type</span><span class="info-val">${restoreEntry.type}</span></div>
    <div class="info-row"><span class="info-label">Size</span><span class="info-val">${fmtSize(restoreEntry.file_size)}</span></div>
    <div class="info-row"><span class="info-label">Snapshot taken</span><span class="info-val">${fmtDate(restoreEntry.created_at)}</span></div>`;
  document.getElementById('r-original-url').value      = conn?.url || '';
  document.getElementById('r-same-url-display').value  = conn ? conn.url.replace(/:([^@]+)@/, ':***@') : '(original connection not found — use Push to Different DB)';
  document.getElementById('r-other-url').value         = '';
  document.getElementById('r-other-url').placeholder   = restoreEntry.type === 'postgres'
    ? 'postgresql://user:pass@newhost:5432/targetdb'
    : 'mongodb://user:pass@newhost:27017/targetdb';
  setMode('same');
  openModal('restore-modal');
}

export function setMode(mode) {
  restoreMode = mode;
  document.getElementById('r-btn-same').className  = 'mode-btn' + (mode === 'same'  ? ' active-same'  : '');
  document.getElementById('r-btn-other').className = 'mode-btn' + (mode === 'other' ? ' active-other' : '');
  document.getElementById('r-sec-same').style.display  = mode === 'same'  ? 'block' : 'none';
  document.getElementById('r-sec-other').style.display = mode === 'other' ? 'block' : 'none';
  const btn = document.getElementById('r-confirm-btn');
  if (mode === 'same') { btn.className = 'btn btn-danger';   btn.textContent = '⚠ Overwrite & Restore'; }
  else                 { btn.className = 'btn btn-success';  btn.textContent = '→ Push to Target'; }
}

export async function doRestore() {
  const origUrl    = document.getElementById('r-original-url').value;
  const otherUrl   = document.getElementById('r-other-url').value.trim();
  const target_url = restoreMode === 'same' ? origUrl : otherUrl;
  if (!target_url) { toast('Enter a target database URL', 'error'); return; }
  const btn     = document.getElementById('r-confirm-btn');
  const restore = spinnerBtn(btn, 'Restoring…');
  try {
    await api.post(`/api/backups/${restoreEntry.id}/restore`, { target_url });
    toast('Restore complete ✓', 'success');
    closeModal('restore-modal');
  } catch (e) {
    toast(e.message, 'error');
  } finally {
    restore();
  }
}
