// Local workerd/D1 only. Never imports a snapshot into a hosted database.
import {readFileSync, mkdtempSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
import {Miniflare, convertV4MiniflareOptions} from 'miniflare';

const [input, output] = process.argv.slice(2);
if (!input || !output) throw Error('Usage: node rehearse-restore.mjs PRIVATE_SNAPSHOT PRIVATE_REPORT');
if (resolve(input) === resolve(output)) throw Error('Report must not replace the snapshot');
const bytes = readFileSync(input);
const snapshot = JSON.parse(bytes);
const tables = ['br_users','br_sessions','br_auth_requests','br_legacy_address_books','br_personal_peers','br_personal_tags','br_devices'];
assert.deepEqual(Object.keys(snapshot.tables), tables);
const timestamp = value => value.replace(/([+-]\d{2})$/, '$1:00');
const normalize = (value, column) => typeof value === 'boolean' ? Number(value) : typeof value === 'string' && column.data_type.startsWith('timestamp') ? timestamp(value) : value;
const snapshotTime = timestamp(snapshot.snapshot_utc);
const canonical = rows => rows.map(row => JSON.stringify(row)).sort();
const dir = mkdtempSync(join(tmpdir(), 'br-restore-'));
const mf = new Miniflare({...convertV4MiniflareOptions({modules:true,script:'export default {fetch(){return new Response("isolated restore")}}',compatibilityDate:'2026-11-10',d1Databases:{DB:'restore'}}),resourcePersistencePath:dir});
try {
  const db = await mf.getD1Database('DB');
  for (const sql of readFileSync(new URL('./schema.sql',import.meta.url),'utf8').split(';').filter(x=>x.trim())) await db.prepare(sql).run();
  const restore = async () => {
    const statements = [];
    for (const table of tables) {
      const source = snapshot.tables[table];
      assert.equal(source.rows.length, source.count);
      const columns = source.columns.map(c=>c.column_name);
      assert(columns.every(c=>/^[a-z_]+$/.test(c)));
      for (const row of source.rows) statements.push(db.prepare(`INSERT INTO ${table} (${columns.join(',')}) VALUES (${columns.map(()=>'?').join(',')})`).bind(...columns.map(c=>normalize(row[c],source.columns.find(col=>col.column_name===c)))));
    }
    if (statements.length) await db.batch(statements);
  };
  const verify = async () => {
    for (const table of tables) {
      const source = snapshot.tables[table], columns = source.columns.map(c=>c.column_name);
      const actual = (await db.prepare(`SELECT ${columns.join(',')} FROM ${table}`).all()).results;
      assert.deepEqual(canonical(actual.map(r=>columns.map(c=>r[c]))),canonical(source.rows.map(r=>columns.map(c=>normalize(r[c],source.columns.find(col=>col.column_name===c))))), `${table} parity`);
    }
    assert.equal((await db.prepare('PRAGMA foreign_key_check').all()).results.length,0);
  };
  await restore(); await verify();
  // Production-only rehearsal of explicit legacy-session promotion. No tokens
  // are printed, invented, or accepted by the staging destination.
  const live = await db.prepare('SELECT COUNT(*) AS n FROM br_sessions s JOIN br_users u ON u.id=s.user_id WHERE julianday(s.expires_at)>julianday(?) AND u.status=1').bind(snapshotTime).first('n');
  assert.equal(live, snapshot.unexpired_sessions, 'Timestamp conversion must preserve active sessions');
  await db.prepare('UPDATE br_sessions SET native=1 WHERE julianday(expires_at)>julianday(?) AND user_id IN(SELECT id FROM br_users WHERE status=1)').bind(snapshotTime).run();
  assert.equal(await db.prepare('SELECT COUNT(*) AS n FROM br_sessions WHERE native=1').first('n'),live);
  assert.equal(await db.prepare('SELECT COUNT(*) AS n FROM br_auth_requests WHERE native=1').first('n'),0);
  // Recover a deliberately changed local copy; compare every original column.
  await db.prepare("UPDATE br_users SET display_name='local rollback fixture'").run();
  await db.batch([...tables].reverse().map(table=>db.prepare(`DELETE FROM ${table}`)));
  await restore(); await verify();
  const report = {snapshotSha256:createHash('sha256').update(bytes).digest('hex'),snapshotUTC:snapshot.snapshot_utc,counts:Object.fromEntries(tables.map(t=>[t,snapshot.tables[t].count])),originalColumnParity:true,foreignKeyCheck:true,activeSessionPromotion:live,legacyOAuthNotRedeemable:true,recoveryParity:true,hostedReads:0,hostedWrites:0,completedUTC:new Date().toISOString()};
  writeFileSync(output,JSON.stringify(report,null,2)+'\n',{mode:0o600});
  console.log(JSON.stringify(report));
} finally { await mf.dispose(); rmSync(dir,{recursive:true,force:true}); }
