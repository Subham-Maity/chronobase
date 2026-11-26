import { api }                                     from '../api.js';
import { toast, openModal, closeModal, emptyState } from '../utils.js';

let schedules = [];

export async function render() {
  schedules = await api.get('/api/schedules');
  renderTable();
}

function renderTable() {
  const wrap = document.getElementById('sched-table-wrap');
  if (!schedules.length) {
    wrap.innerHTML = emptyState('◷', 'No schedules', 'Create a schedule to auto-backup on a timer.');
    return;
  }

  const rows = schedules.map(s => `
    <tr>
      <td class="cell-name">${s.name}</td>
      <td><span class="badge badge-group">${s.group_name}</span></td>
      <td><code style="font-family:var(--mono);font-size:12px;color:var(--accent)">${s.cron_expr}</code></td>
      <td>
        <button class="btn btn-sm ${s.enabled ? 'btn-success' : 'btn-ghost'}" data-toggle="${s.id}">
          ${s.enabled ? '◉ Active' : '○ Paused'}
        </button>
      </td>
      <td class="cell-time">${s.last_run ? new Date(s.last_run).toLocaleString() : 'never'}</td>
      <td><button class="btn btn-danger btn-sm" data-delete="${s.id}">✕</button></td>
    </tr>`).join('');

  wrap.innerHTML = `
    <table>
      <thead><tr><th>Name</th><th>Group</th><th>Cron</th><th>Status</th><th>Last Run</th><th></th></tr></thead>
      <tbody>${rows}</tbody>
    </table>`;

  wrap.querySelectorAll('[data-toggle]').forEach(btn  => btn.addEventListener('click', () => toggleSchedule(parseInt(btn.dataset.toggle))));
  wrap.querySelectorAll('[data-delete]').forEach(btn  => btn.addEventListener('click', () => deleteSchedule(parseInt(btn.dataset.delete))));
}

async function toggleSchedule(id) {
  await api.put(`/api/schedules/${id}/toggle`, {});
  render();
}

async function deleteSchedule(id) {
  if (!confirm('Delete this schedule?')) return;
  await api.delete(`/api/schedules/${id}`);
  toast('Schedule deleted', 'info');
  render();
}

export function openAdd() { openModal('sched-modal'); }

export function setCron(val) { document.getElementById('s-cron').value = val; }

export async function saveSchedule() {
  const body = {
    name:       document.getElementById('s-name').value.trim(),
    group_name: document.getElementById('s-group').value,
    cron_expr:  document.getElementById('s-cron').value.trim(),
  };
  if (!body.name) { toast('Schedule name is required', 'error'); return; }
  try {
    await api.post('/api/schedules', body);
    toast('Schedule created', 'success');
    closeModal('sched-modal');
    document.getElementById('sched-form').reset();
    render();
  } catch (e) {
    toast(e.message, 'error');
  }
}

export function populateGroupSelect(groups) {
  const sel = document.getElementById('s-group');
  sel.innerHTML = '<option value="All">All Connections</option>' + groups.map(g => `<option value="${g}">${g}</option>`).join('');
}
