const { exec } = require('child_process');

const PG_DUMP_CANDIDATES = [
  'pg_dump',
  '/usr/bin/pg_dump',
  '/usr/local/bin/pg_dump',
  '/opt/homebrew/bin/pg_dump',
  'C:\\Program Files\\PostgreSQL\\18\\bin\\pg_dump.exe',
  'C:\\Program Files\\PostgreSQL\\17\\bin\\pg_dump.exe',
  'C:\\Program Files\\PostgreSQL\\16\\bin\\pg_dump.exe',
  'C:\\Program Files\\PostgreSQL\\15\\bin\\pg_dump.exe',
  'C:\\Program Files\\PostgreSQL\\14\\bin\\pg_dump.exe',
];

const PSQL_CANDIDATES = PG_DUMP_CANDIDATES.map(p => p.replace('pg_dump', 'psql'));

const MONGODUMP_CANDIDATES = [
  'mongodump',
  '/usr/bin/mongodump',
  '/usr/local/bin/mongodump',
  '/opt/homebrew/bin/mongodump',
  'C:\\Program Files\\MongoDB\\Tools\\100\\bin\\mongodump.exe',
  'C:\\Program Files\\MongoDB\\Tools\\101\\bin\\mongodump.exe',
];

const MONGORESTORE_CANDIDATES = MONGODUMP_CANDIDATES.map(p => p.replace('mongodump', 'mongorestore'));

function probe(bin) {
  return new Promise(resolve =>
    exec(`"${bin}" --version 2>&1`, { timeout: 5000 }, err => resolve(!err))
  );
}

async function find(candidates) {
  for (const bin of candidates) {
    if (await probe(bin)) return bin;
  }
  return null;
}

async function status() {
  const [pgPath, mongoPath] = await Promise.all([
    find(PG_DUMP_CANDIDATES),
    find(MONGODUMP_CANDIDATES),
  ]);
  return { pg: !!pgPath, mongo: !!mongoPath, pgPath, mongoPath };
}

module.exports = {
  PG_DUMP_CANDIDATES,
  PSQL_CANDIDATES,
  MONGODUMP_CANDIDATES,
  MONGORESTORE_CANDIDATES,
  find,
  status,
};
