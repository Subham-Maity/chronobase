import { api }                                                            from '../api.js';
import { maskUrl, typeBadge, groupBadge, toast, openModal, closeModal, spinnerBtn, emptyState, showConfirm } from '../utils.js';

let connections = [];
let editingId   = null;

export async function render() {
  connections = await api.get('/api/connections');
  document.getElementById('conn-badge').textContent = connections.length;
  updateGroupFilter();
  renderTable();
}

function updateGroupFilter() {
  const groups = [...new Set(connections.map(c => c.group_name))];
  const sel    = document.getElementById('conn-group-filter');
  const val    = sel.value;
  sel.innerHTML = '<option value="">All Groups</option>' +
    groups.map(g => `<option value="${g}">${g}</option>`).join('');
  sel.value = val;
}

export function renderTable() {
  const gf   = document.getElementById('conn-group-filter').value;
  const tf   = document.getElementById('conn-type-filter').value;
  const list = connections.filter(c => (!gf || c.group_name === gf) && (!tf || c.type === tf));
  const wrap = document.getElementById('conn-table-wrap');

  if (!list.length) {
    wrap.innerHTML = emptyState('⬡', 'No connections', 'Click "+ Add Connection" to get started.');
    return;
  }

  const rows = list.map(c => `
    <tr>
      <td class="cell-name">${c.name}</td>
      <td>${groupBadge(c.group_name)}</td>
      <td>${typeBadge(c.type)}</td>
      <td class="cell-url" title="${maskUrl(c.url)}">${maskUrl(c.url)}</td>
      <td>
        <div class="actions">
          <button class="btn btn-primary btn-sm"  data-backup="${c.id}">⚡ Backup</button>
          <button class="btn btn-ghost btn-sm"    data-edit="${c.id}">Edit</button>
          <button class="btn btn-wipe btn-sm"     data-wipe="${c.id}" title="Delete ALL data inside this database">🗑 Wipe DB</button>
          <button class="btn btn-danger btn-sm"   data-delete="${c.id}">✕</button>
        </div>
      </td>
    </tr>`).join('');

  wrap.innerHTML = `
    <table>
      <thead><tr><th>Name</th><th>Group</th><th>Type</th><th>URL</th><th>Actions</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>`;

  wrap.querySelectorAll('[data-backup]').forEach(btn  => btn.addEventListener('click', () => backupOne(parseInt(btn.dataset.backup), btn)));
  wrap.querySelectorAll('[data-edit]').forEach(btn    => btn.addEventListener('click', () => openEdit(parseInt(btn.dataset.edit))));
  wrap.querySelectorAll('[data-wipe]').forEach(btn    => btn.addEventListener('click', () => wipeDb(parseInt(btn.dataset.wipe), btn)));
  wrap.querySelectorAll('[data-delete]').forEach(btn  => btn.addEventListener('click', () => deleteConn(parseInt(btn.dataset.delete))));
}

async function backupOne(id, btn) {
  const restore = spinnerBtn(btn, '…');
  try {
    await api.post(`/api/connections/${id}/backup`, {});
    toast('Backup saved ✓', 'success');
  } catch (e) {
    toast(e.message, 'error');
  } finally {
    restore();
  }
}

async function wipeDb(id, btn) {
  const conn = connections.find(c => c.id === id);
  if (!conn) return;

  const confirmed = await showConfirm({
    icon:         '💀',
    iconType:     'red',
    title:        'Wipe entire database?',
    body:         `This will permanently delete <strong>every table, row, and collection</strong> inside <strong>${conn.name}</strong>.<br><br>
                  <span style="color:var(--text-lo);font-size:12px">Your ChronoBase connection entry is kept. Only the actual database contents are deleted.<br>
                  Consider running a backup first.</span>`,
    confirmLabel: '💀 Yes, wipe it',
    confirmClass: 'btn-confirm-danger',
  });

  if (!confirmed) return;

  const restore = spinnerBtn(btn, 'Wiping…');
  try {
    await api.post(`/api/connections/${id}/wipe`, {});
    toast(`${conn.name} — database wiped`, 'success');
  } catch (e) {
    toast(e.message, 'error');
  } finally {
    restore();
  }
}

async function deleteConn(id) {
  const conn = connections.find(c => c.id === id);
  const confirmed = await showConfirm({
    icon:         '⬡',
    iconType:     'amber',
    title:        'Remove connection?',
    body:         `<strong>${conn?.name || 'This connection'}</strong> will be removed from ChronoBase.<br><br>
                  <span style="color:var(--text-lo);font-size:12px">Existing backup files on disk are kept. The actual database is not affected.</span>`,
    confirmLabel: 'Remove',
    confirmClass: 'btn-danger',
  });
  if (!confirmed) return;
  await api.delete(`/api/connections/${id}`);
  toast('Connection removed', 'info');
  render();
}

export function openAdd() {
  editingId = null;
  document.getElementById('conn-modal-title').textContent = 'Add Connection';
  document.getElementById('conn-form').reset();
  document.getElementById('f-group').value = 'Default';
  updateUrlHint();
  openModal('conn-modal');
}

function openEdit(id) {
  const c = connections.find(x => x.id === id);
  if (!c) return;
  editingId = id;
  document.getElementById('conn-modal-title').textContent = 'Edit Connection';
  document.getElementById('f-name').value  = c.name;
  document.getElementById('f-group').value = c.group_name;
  document.getElementById('f-type').value  = c.type;
  document.getElementById('f-url').value   = c.url;
  document.getElementById('f-notes').value = c.notes || '';
  updateUrlHint();
  openModal('conn-modal');
}

export async function saveConnection() {
  const body = {
    name:       document.getElementById('f-name').value.trim(),
    group_name: document.getElementById('f-group').value.trim() || 'Default',
    type:       document.getElementById('f-type').value,
    url:        document.getElementById('f-url').value.trim(),
    notes:      document.getElementById('f-notes').value.trim(),
  };
  if (!body.name || !body.url) { toast('Name and URL are required', 'error'); return; }
  try {
    if (editingId) await api.put(`/api/connections/${editingId}`, body);
    else           await api.post('/api/connections', body);
    toast(editingId ? 'Connection updated ✓' : 'Connection added ✓', 'success');
    closeModal('conn-modal');
    render();
  } catch (e) {
    toast(e.message, 'error');
  }
}

export async function backupGroup() {
  const group   = document.getElementById('conn-group-filter').value || 'All';
  const btn     = document.getElementById('backup-group-btn');
  const restore = spinnerBtn(btn, 'Backing up…');
  try {
    const results = await api.post('/api/connections/backup-group', { group_name: group });
    const ok = results.filter(r => r.ok).length;
    toast(`${ok}/${results.length} backed up`, ok === results.length ? 'success' : 'error');
  } catch (e) {
    toast(e.message, 'error');
  } finally {
    restore();
  }
}

export function updateUrlHint() {
  const type = document.getElementById('f-type').value;
  document.getElementById('f-url-hint').textContent = type === 'postgres'
    ? 'postgresql://username:password@host:5432/database'
    : 'mongodb://username:password@host:27017/database  or  mongodb+srv://…';
  document.getElementById('f-url').placeholder = type === 'postgres'
    ? 'postgresql://user:pass@localhost:5432/mydb'
    : 'mongodb://user:pass@localhost:27017/mydb';
}
