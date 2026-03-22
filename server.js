const express   = require('express');
const path      = require('path');
const db        = require('./src/database');
const scheduler = require('./src/scheduler');

const app  = express();
const PORT = process.env.PORT || 3420;

app.use(express.json());
app.use(express.static(path.join(__dirname, 'public')));

app.use('/api/connections', require('./src/routes/connections'));
app.use('/api/backups',     require('./src/routes/backups'));
app.use('/api/schedules',   require('./src/routes/schedules'));
app.use('/api/system',      require('./src/routes/system'));

app.get('*', (_req, res) => {
  res.sendFile(path.join(__dirname, 'public', 'index.html'));
});

if (!require('fs').existsSync(path.join(process.cwd(), 'chronobase-data.json'))) {
  db.write({ connections: [], backups: [], schedules: [], _seq: { c: 1, b: 1, s: 1 } });
}

scheduler.reload();

app.listen(PORT, () => {
  console.log('\n  ╭─────────────────────────────────╮');
  console.log('  │  ChronoBase  ·  Backup Manager  │');
  console.log('  ╰─────────────────────────────────╯');
  console.log(`\n  Local:  http://localhost:${PORT}`);
  console.log('  Stop:   Ctrl+C\n');
});
