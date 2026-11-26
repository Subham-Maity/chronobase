export function fmtSize(bytes) {
  if (!bytes) return '0 B';
  const units = ['B','KB','MB','GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / 1024 ** i).toFixed(1)} ${units[i]}`;
}

export function fmtDate(str) {
  return str ? new Date(str).toLocaleString() : '—';
}

export function fmtRelative(str) {
  if (!str) return '—';
  const diff = Date.now() - new Date(str).getTime();
  const m = Math.floor(diff / 60_000);
  if (m < 1)  return 'just now';
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  return `${Math.floor(h / 24)}d ago`;
}

export function maskUrl(url = '') {
  return url.replace(/:([^@]+)@/, ':***@');
}

export function typeBadge(type) {
  return type === 'postgres'
    ? '<span class="badge badge-pg">🐘 Postgres</span>'
    : '<span class="badge badge-mongo">🍃 MongoDB</span>';
}

export function statusBadge(status) {
  return `<span class="badge badge-${status}">${status}</span>`;
}

export function groupBadge(name) {
  return `<span class="badge badge-group">${name}</span>`;
}

const toastContainer = document.getElementById('toast-container');

export function toast(message, type = 'info') {
  const el = document.createElement('div');
  el.className = `toast ${type}`;
  const icons = { success: '✓', error: '✗', info: '·' };
  el.innerHTML = `<span style="font-weight:700">${icons[type] || '·'}</span><span>${message}</span>`;
  toastContainer.appendChild(el);
  setTimeout(() => el.remove(), 3800);
}

export function openModal(id)  { document.getElementById(id).classList.add('open'); }
export function closeModal(id) { document.getElementById(id).classList.remove('open'); }

export function emptyState(icon, title, sub = '') {
  return `<div class="empty">
    <div class="empty-icon">${icon}</div>
    <div class="empty-title">${title}</div>
    ${sub ? `<div class="empty-sub">${sub}</div>` : ''}
  </div>`;
}

export function spinnerBtn(btn, label = 'Working…') {
  btn.disabled   = true;
  btn._origLabel = btn.innerHTML;
  btn.innerHTML  = `<span class="spin"></span> ${label}`;
  return () => { btn.disabled = false; btn.innerHTML = btn._origLabel; };
}

// ── Generic confirm modal (replaces native confirm()) ──────────────────────
let _confirmResolve = null;

export function showConfirm({ icon = '⚠', iconType = 'amber', title, body, confirmLabel = 'Confirm', confirmClass = 'btn-danger' }) {
  return new Promise(resolve => {
    _confirmResolve = resolve;

    document.getElementById('generic-confirm-icon').textContent  = icon;
    document.getElementById('generic-confirm-icon').className    = `modal-icon ${iconType}`;
    document.getElementById('generic-confirm-title').textContent = title;
    document.getElementById('generic-confirm-body').innerHTML    = body;
    const confirmBtn = document.getElementById('generic-confirm-ok');
    confirmBtn.textContent = confirmLabel;
    confirmBtn.className   = `btn ${confirmClass}`;

    openModal('generic-confirm-modal');
  });
}

document.addEventListener('DOMContentLoaded', () => {
  document.getElementById('generic-confirm-ok').addEventListener('click', () => {
    closeModal('generic-confirm-modal');
    if (_confirmResolve) { _confirmResolve(true); _confirmResolve = null; }
  });
  document.getElementById('generic-confirm-cancel').addEventListener('click', () => {
    closeModal('generic-confirm-modal');
    if (_confirmResolve) { _confirmResolve(false); _confirmResolve = null; }
  });
  document.getElementById('generic-confirm-modal').addEventListener('click', e => {
    if (e.target === document.getElementById('generic-confirm-modal')) {
      closeModal('generic-confirm-modal');
      if (_confirmResolve) { _confirmResolve(false); _confirmResolve = null; }
    }
  });

  document.querySelectorAll('.modal-overlay').forEach(overlay => {
    overlay.addEventListener('click', e => {
      if (e.target === overlay) overlay.classList.remove('open');
    });
  });
});
