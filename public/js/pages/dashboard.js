import { api }                             from '../api.js';
import { fmtSize, fmtRelative, typeBadge, statusBadge, groupBadge, toast, spinnerBtn } from '../utils.js';

export async function render() {
  const [stats, recent] = await Promise.all([
    api.get('/api/system/stats'),
    api.get('/api/backups'),
  ]);

  document.getElementById('s-conns').textContent = stats.totalConns;
  document.getElementById('s-backups').textContent = stats.totalBackups;
  document.getElementById('s-rate').textContent = stats.totalBackups
    ? `${Math.round(stats.successCount / stats.totalBackups * 100)}% success`
    : 'no backups yet';
  document.getElementById('s-size').textContent = fmtSize(stats.totalSize);
  document.getElementById('s-last').textContent = fmtRelative(stats.lastBackup);
  document.getElementById('s-scheds').textContent = `${stats.activeScheds} active schedule${stats.activeScheds !== 1 ? 's' : ''}`;

  const wrap = document.getElementById('recent-wrap');
  if (!recent.length) {
    wrap.innerHTML = `<div class="empty"><div class="empty-icon">◎</div><div class="empty-title">No backups yet</div><div class="empty-sub">Add a connection and click Backup All</div></div>`;
    return;
  }

  const rows = recent.slice(0, 8).map(b => `
    <tr>
      <td class="cell-name">${b.connection_name}</td>
      <td>${groupBadge(b.group_name)}</td>
      <td>${typeBadge(b.type)}</td>
      <td>${statusBadge(b.status)}</td>
      <td class="cell-size">${fmtSize(b.file_size)}</td>
      <td class="cell-time">${fmtRelative(b.created_at)}</td>
    </tr>`).join('');

  wrap.innerHTML = `
    <table>
      <thead><tr><th>Database</th><th>Group</th><th>Type</th><th>Status</th><th>Size</th><th>When</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>`;
}

export async function backupAll(btn) {
  const restore = spinnerBtn(btn, 'Backing up…');
  try {
    const results = await api.post('/api/connections/backup-group', { group_name: 'All' });
    const ok   = results.filter(r => r.ok).length;
    const fail = results.filter(r => !r.ok).length;
    toast(`Done: ${ok} success${fail ? `, ${fail} failed` : ''}`, fail ? 'error' : 'success');
    render();
  } catch (e) {
    toast(e.message, 'error');
  } finally {
    restore();
  }
}
