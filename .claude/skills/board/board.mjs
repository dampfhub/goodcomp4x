#!/usr/bin/env node
// Work board: a thin, deterministic wrapper over a GitHub Projects v2 board.
//
// Why this exists rather than raw `gh`:
//   1. `gh project item-add` succeeds exactly once per process: it caches its content
//      lookup, so a loop adds one item and then reports "resource not found" for every
//      other one. Everything here goes through `gh api graphql` + addProjectV2ItemById
//      instead, which works in a loop.
//   2. Setting a single-select needs three opaque ids (project, field, option). Fine for
//      a script, unusable by hand.
//   3. `gh` is often not on Git Bash's PATH on Windows; we locate it.
//
// Everything project-specific (repo, project owner and number, field schema, labels)
// lives in `.claude/board.config.json`, so this file carries no project knowledge.

import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// The script sits at <root>/.claude/skills/board/board.mjs, so the root is fixed by its
// own location: the command works from any cwd, and from a worktree it answers about
// that worktree's copy of the config.
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const CONFIG_PATH = join(ROOT, '.claude', 'board.config.json');

// Exit code 1 is the verdict on any failure: every caller that has already written a
// partial answer to stdout before a call that can fail must gather all its data before
// printing any of it, so a `die()` never leaves a confident-looking partial answer on
// stdout beside a failing exit code.
function die(msg) { console.error(`board: ${msg}`); process.exit(1); }

// ---------------------------------------------------------------- config

// Flags that commands read themselves; a field whose flag spelling collides with one
// would be silently written by an unrelated command.
const RESERVED_FLAGS = new Set([
  'title', 'body', 'body-file', 'label', 'fresh', 'open', 'of', 'reason', 'commit',
  'by', 'add', 'comment', 'apply', 'selftest',
]);
const STATUS = { todo: 'Todo', inProgress: 'In Progress', done: 'Done' };
const FILES_FIELD = 'Files';

function flagOf(fieldName) { return fieldName.toLowerCase().replace(/\s+/g, '-'); }

// Pure: validates a parsed config and returns it normalized, or throws. The selftest
// drives this directly.
function normalizeConfig(raw) {
  const c = { ...raw };
  if (typeof c.repo !== 'string' || !/^[\w.-]+\/[\w.-]+$/.test(c.repo)) throw new Error('"repo" must be "owner/name"');
  if (typeof c.projectOwner !== 'string' || !c.projectOwner) throw new Error('"projectOwner" is required');
  c.projectOwnerType = c.projectOwnerType ?? 'user';
  if (!['user', 'organization'].includes(c.projectOwnerType)) throw new Error('"projectOwnerType" must be "user" or "organization"');
  if (c.projectNumber !== null && c.projectNumber !== undefined && !Number.isInteger(c.projectNumber)) throw new Error('"projectNumber" must be an integer or null');
  c.projectNumber = c.projectNumber ?? null;
  c.defaultBranch = c.defaultBranch ?? 'main';
  c.selectFields = c.selectFields ?? {};
  c.textFields = c.textFields ?? [];
  c.requiredOnFile = c.requiredOnFile ?? [];
  c.labels = c.labels ?? {};
  const status = c.selectFields.Status;
  if (!Array.isArray(status) || !Object.values(STATUS).every(s => status.includes(s))) {
    throw new Error(`"selectFields.Status" must include ${Object.values(STATUS).join(', ')}`);
  }
  const all = [...Object.keys(c.selectFields), ...c.textFields];
  const seen = new Set();
  for (const name of all) {
    const flag = flagOf(name);
    if (RESERVED_FLAGS.has(flag)) throw new Error(`field "${name}" would use the reserved flag --${flag}`);
    if (seen.has(flag)) throw new Error(`two fields share the flag --${flag}`);
    seen.add(flag);
  }
  for (const r of c.requiredOnFile) if (!all.includes(r)) throw new Error(`"requiredOnFile" names unknown field "${r}"`);
  c.listColumns = c.listColumns ?? Object.keys(c.selectFields);
  for (const col of c.listColumns) if (!all.includes(col)) throw new Error(`"listColumns" names unknown field "${col}"`);
  return c;
}

function loadConfig() {
  if (!existsSync(CONFIG_PATH)) die(`no config at ${CONFIG_PATH}`);
  try {
    return normalizeConfig(JSON.parse(readFileSync(CONFIG_PATH, 'utf8')));
  } catch (e) {
    die(`${CONFIG_PATH}: ${e.message}`);
  }
}

const CFG = loadConfig();
const REPO = CFG.repo;
const [REPO_OWNER, REPO_NAME] = REPO.split('/');
const OWNER = CFG.projectOwner;
const OWNER_KIND = CFG.projectOwnerType;   // the GraphQL root field: user(...) or organization(...)
const SELECT_FIELDS = Object.keys(CFG.selectFields);
const TEXT_FIELDS = CFG.textFields;
const ALL_FIELDS = [...SELECT_FIELDS, ...TEXT_FIELDS];

function projectNumber() {
  if (CFG.projectNumber === null) {
    die(`no project configured. Create one and put its number in .claude/board.config.json:
  gh project create --owner ${OWNER} --title "${REPO_NAME}"
then run \`board.mjs setup\` to see what the project is missing.`);
  }
  return CFG.projectNumber;
}

// A trivial `gh api graphql` call costs ~1-2s (process spawn + network round trip); node
// startup is ~250-300ms. That fixed per-call cost, not JSON size, dominates every command
// that pages through the whole project. BOARD_TRACE=1 prints each call's label and wall
// time to stderr, so a slow command can be attributed to a call count.
const TRACE = process.env.BOARD_TRACE === '1';
let traceSeq = 0;
function trace(label, fn) {
  if (!TRACE) return fn();
  const seq = ++traceSeq;
  const t0 = Date.now();
  try {
    return fn();
  } finally {
    console.error(`[board-trace] #${seq} ${label} ${Date.now() - t0}ms`);
  }
}

// ---------------------------------------------------------------- gh discovery

let _gh = null;
function ghPath() {
  if (_gh) return _gh;
  const candidates = [
    'gh',
    'C:\\Program Files\\GitHub CLI\\gh.exe',
    'C:\\Program Files (x86)\\GitHub CLI\\gh.exe',
    `${process.env.LOCALAPPDATA ?? ''}\\Programs\\GitHub CLI\\gh.exe`,
    `${process.env.LOCALAPPDATA ?? ''}\\Microsoft\\WinGet\\Links\\gh.exe`,
    '/opt/homebrew/bin/gh',
    '/usr/local/bin/gh',
  ];
  for (const c of candidates) {
    if (c === 'gh') {
      try { execFileSync('gh', ['--version'], { stdio: 'ignore' }); return (_gh = 'gh'); } catch { continue; }
    } else if (existsSync(c)) return (_gh = c);
  }
  die('gh CLI not found. Install it, then run `gh auth login -s project`.');
}

function gh(args, { json = true, label } = {}) {
  return trace(label ?? args.slice(0, 3).join(' '), () => {
    let out;
    try {
      out = execFileSync(ghPath(), args, { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'] });
    } catch (e) {
      const detail = (e.stderr || e.stdout || e.message || '').toString().trim();
      die(`gh ${args.slice(0, 3).join(' ')} failed: ${detail}`);
    }
    return json ? JSON.parse(out) : out;
  });
}

// A null/undefined variable is omitted, not sent: `gh -f k=null` sends the four-character
// string "null", which a cursor argument rejects outright. An absent nullable variable is
// already null per the GraphQL spec. `-F` type-coerces (ints, bools, @file) and mangles
// arbitrary strings; `-f` is raw, so route by value type.
function graphqlArgs(query, vars) {
  const args = ['api', 'graphql', '-f', `query=${query}`];
  for (const [k, v] of Object.entries(vars)) {
    if (v === null || v === undefined) continue;
    const flag = typeof v === 'number' ? '-F' : '-f';
    args.push(flag, `${k}=${v}`);
  }
  return args;
}

function graphql(query, vars = {}, label = 'graphql') {
  const r = gh(graphqlArgs(query, vars), { label });
  if (r.errors) die(`graphql: ${JSON.stringify(r.errors)}`);
  return r.data;
}

// GitHub raises a GraphQL error, not a null field, when a query resolves a node by number
// and the number does not exist. Only that message shape is read as "absent"; anything
// else still dies, so a transport failure is never mistaken for "not found".
const NODE_NOT_FOUND_RE = /Could not resolve to an? \w+ with the number/i;

function graphqlOrNull(query, vars, label) {
  const args = graphqlArgs(query, vars);
  return trace(label, () => {
    let out;
    try {
      out = execFileSync(ghPath(), args, { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'] });
    } catch (e) {
      const detail = (e.stderr || e.stdout || e.message || '').toString().trim();
      if (NODE_NOT_FOUND_RE.test(detail)) return null;
      die(`gh ${args.slice(0, 3).join(' ')} failed: ${detail}`);
    }
    const r = JSON.parse(out);
    if (r.errors) {
      if (r.errors.every(er => NODE_NOT_FOUND_RE.test(er.message ?? ''))) return null;
      die(`graphql: ${JSON.stringify(r.errors)}`);
    }
    return r.data;
  });
}

// ---------------------------------------------------------------- board schema

let _schema = null;
function schema() {
  if (_schema) return _schema;
  const d = graphql(
    `query($o:String!,$n:Int!){${OWNER_KIND}(login:$o){projectV2(number:$n){
       id title url viewerCanUpdate
       repositories(first:20){nodes{nameWithOwner}}
       fields(first:50){nodes{
         ... on ProjectV2Field{id name dataType}
         ... on ProjectV2SingleSelectField{id name dataType options{id name}}
       }}
     }}}`,
    { o: OWNER, n: projectNumber() },
    'schema',
  );
  const p = d[OWNER_KIND]?.projectV2;
  if (!p) die(`project #${projectNumber()} not found for ${OWNER_KIND} ${OWNER}`);
  const fields = {};
  for (const f of p.fields.nodes) {
    if (!f?.name) continue;
    fields[f.name] = { id: f.id, dataType: f.dataType, options: f.options ?? null };
  }
  _schema = {
    projectId: p.id,
    title: p.title,
    url: p.url,
    canUpdate: !!p.viewerCanUpdate,
    repos: (p.repositories?.nodes ?? []).map(r => r.nameWithOwner),
    fields,
  };
  return _schema;
}

function fieldId(name) {
  const f = schema().fields[name];
  if (!f) die(`the project has no field "${name}" (run \`board.mjs setup\`). Have: ${Object.keys(schema().fields).join(', ')}`);
  return f.id;
}

function optionId(field, value) {
  const f = schema().fields[field];
  if (!f) die(`the project has no field "${field}" (run \`board.mjs setup\`)`);
  if (!f.options) die(`field "${field}" is not a single-select`);
  const hit = f.options.find(o => o.name.toLowerCase() === String(value).toLowerCase());
  if (!hit) die(`"${value}" is not a valid ${field}. Options: ${f.options.map(o => o.name).join(', ')}`);
  return hit.id;
}

// ---------------------------------------------------------------- board-wide cache
//
// A whole-board read (`list`, `fences`) pages through every item at ~1-2s per page
// regardless of what changed. A snapshot cache trades staleness (bounded by CACHE_TTL_MS)
// for that cost: a read within the window uses the snapshot, and `--fresh` always
// re-pages. Every mutating command patches the one entry it touched via a direct
// by-number fetch, so a read right after a write sees the write.
const CACHE_TTL_MS = 10 * 60 * 1000;
function cachePath() { return join(ROOT, '.claude', 'board-cache.json'); }

function readCache() {
  try {
    const parsed = JSON.parse(readFileSync(cachePath(), 'utf8'));
    if (!parsed || typeof parsed.fetchedAt !== 'number' || !Array.isArray(parsed.items)) return null;
    // A cache written for another project would answer confidently about the wrong board.
    if (parsed.project !== cacheKey()) return null;
    return parsed;
  } catch {
    return null;
  }
}

function cacheKey() { return `${OWNER_KIND}:${OWNER}/${CFG.projectNumber}`; }

function writeCacheFile(cache) {
  try { writeFileSync(cachePath(), JSON.stringify({ ...cache, project: cacheKey() })); } catch { /* an optimization, not a correctness requirement */ }
}

// Pure: no disk, no network, no wall clock unless `now` is omitted. The selftest drives
// these, so the fixtures exercise the shipped functions rather than a paraphrase.
function cacheFresh(cache, now = Date.now()) {
  return !!cache && (now - cache.fetchedAt) < CACHE_TTL_MS;
}

function applyCachePatch(cache, item) {
  const items = cache ? cache.items.slice() : [];
  const idx = items.findIndex(x => x.number === item.number);
  if (idx === -1) { items.push(item); items.sort((a, b) => a.number - b.number); }
  else items[idx] = item;
  return { fetchedAt: cache ? cache.fetchedAt : Date.now(), items };
}

function applyCacheRemoval(cache, number) {
  if (!cache) return cache;
  return { fetchedAt: cache.fetchedAt, items: cache.items.filter(x => x.number !== Number(number)) };
}

// A cache miss is a no-op: never create a one-item cache file, which a later whole-board
// read would treat as complete.
function patchCacheEntry(item) {
  if (!item) return;
  const cache = readCache();
  if (!cache) return;
  writeCacheFile(applyCachePatch(cache, item));
}

function refreshCacheEntry(number) {
  const cache = readCache();
  if (!cache) return;
  const fresh = itemDirect(number);
  writeCacheFile(fresh ? applyCachePatch(cache, fresh) : applyCacheRemoval(cache, number));
}

// ---------------------------------------------------------------- items

const FIELD_VALUES = `fieldValues(first:30){nodes{
           ... on ProjectV2ItemFieldTextValue{text field{... on ProjectV2FieldCommon{name}}}
           ... on ProjectV2ItemFieldSingleSelectValue{name field{... on ProjectV2FieldCommon{name}}}
         }}`;

function fieldMap(fieldValues) {
  const f = {};
  for (const v of fieldValues?.nodes ?? []) {
    const n = v.field?.name;
    if (n) f[n] = v.text ?? v.name;
  }
  return f;
}

// Paginated deliberately: a single `items(first:100)` silently truncates once the board
// passes 100 items, and it drops the newest ones, so `list` looks complete and `fences`
// goes blind to exactly the work in flight.
function items({ fresh = false } = {}) {
  if (!fresh) {
    const cache = readCache();
    if (cacheFresh(cache)) {
      if (TRACE) console.error(`[board-trace] items(): cache hit, age=${Date.now() - cache.fetchedAt}ms, ${cache.items.length} item(s)`);
      return cache.items;
    }
  }
  const nodes = [];
  let after = null;
  let pages = 0;
  for (;;) {
    const d = graphql(
      `query($o:String!,$n:Int!,$after:String){${OWNER_KIND}(login:$o){projectV2(number:$n){
         items(first:100,after:$after){
           pageInfo{hasNextPage endCursor}
           nodes{
             id
             content{... on Issue{number title state url repository{nameWithOwner} labels(first:20){nodes{name}}}}
             ${FIELD_VALUES}
           }}
       }}}`,
      { o: OWNER, n: projectNumber(), after },
      'items-page',
    );
    pages++;
    const page = d[OWNER_KIND].projectV2.items;
    nodes.push(...(page.nodes ?? []));
    if (!page.pageInfo?.hasNextPage) break;
    after = page.pageInfo.endCursor;
  }
  const result = nodes
    // Drafts and pull requests carry no issue number; issues from other repos on a shared
    // board are not this repo's work and would collide by number.
    .filter(i => i.content?.number && i.content.repository?.nameWithOwner === REPO)
    .map(i => ({
      itemId: i.id,
      number: i.content.number,
      title: i.content.title,
      state: i.content.state,
      url: i.content.url,
      labels: (i.content.labels?.nodes ?? []).map(l => l.name),
      fields: fieldMap(i.fieldValues),
    }))
    .sort((a, b) => a.number - b.number);
  writeCacheFile({ fetchedAt: Date.now(), items: result });
  if (TRACE) console.error(`[board-trace] items(): ${pages} page(s), ${result.length} item(s), cache written`);
  return result;
}

// Project numbers are per owner, so an issue on two owners' project #1 is told apart by
// the owner's login as well as the number.
function isOurProject(p) {
  return p.project?.number === CFG.projectNumber &&
    String(p.project?.owner?.login ?? '').toLowerCase() === OWNER.toLowerCase();
}

function parseIssueNode(issue) {
  if (!issue) return null;
  const pItem = (issue.projectItems?.nodes ?? []).find(isOurProject);
  if (!pItem) return null;
  return {
    itemId: pItem.id,
    number: issue.number,
    title: issue.title,
    state: issue.state,
    url: issue.url,
    labels: (issue.labels?.nodes ?? []).map(l => l.name),
    fields: fieldMap(pItem.fieldValues),
  };
}

const ISSUE_PROJECT_FIELDS = `
       id number title state url
       labels(first:20){nodes{name}}
       projectItems(first:10){nodes{
         id project{number owner{... on User{login} ... on Organization{login}}}
         ${FIELD_VALUES}
       }}`;

function itemDirect(number) {
  const d = graphqlOrNull(
    `query($o:String!,$r:String!,$n:Int!){repository(owner:$o,name:$r){issue(number:$n){${ISSUE_PROJECT_FIELDS}
     }}}`,
    { o: REPO_OWNER, r: REPO_NAME, n: Number(number) },
    'itemDirect',
  );
  return parseIssueNode(d?.repository?.issue);
}

// `show`'s fields and its sub-issue summary in one call rather than two, so a failure of
// the second half never follows an already-printed first half.
function showFetch(number) {
  const d = graphqlOrNull(
    `query($o:String!,$r:String!,$n:Int!){repository(owner:$o,name:$r){issue(number:$n){${ISSUE_PROJECT_FIELDS}
       subIssues(first:50){nodes{number title state}}
       subIssuesSummary{total completed percentCompleted}
     }}}`,
    { o: REPO_OWNER, r: REPO_NAME, n: Number(number) },
    'showFetch',
  );
  const issue = d?.repository?.issue;
  return {
    item: parseIssueNode(issue),
    kids: {
      nodes: issue?.subIssues?.nodes ?? [],
      total: issue?.subIssuesSummary?.total ?? 0,
      completed: issue?.subIssuesSummary?.completed ?? 0,
      percent: issue?.subIssuesSummary?.percentCompleted ?? 0,
    },
  };
}

function itemFor(issueNumber) {
  const hit = itemDirect(issueNumber);
  if (!hit) die(`issue #${issueNumber} is not on the board (add it with \`file\`, or it may not exist)`);
  return hit;
}

function issueNodeId(number) {
  const d = graphql(
    `query($o:String!,$r:String!,$n:Int!){repository(owner:$o,name:$r){issue(number:$n){id}}}`,
    { o: REPO_OWNER, r: REPO_NAME, n: Number(number) },
    'issueNodeId',
  );
  const id = d.repository?.issue?.id;
  if (!id) die(`issue #${number} not found in ${REPO}`);
  return id;
}

function addToBoard(issueNumber) {
  const d = graphql(
    `mutation($p:ID!,$c:ID!){addProjectV2ItemById(input:{projectId:$p,contentId:$c}){item{id}}}`,
    { p: schema().projectId, c: issueNodeId(issueNumber) },
    'addToBoard',
  );
  return d.addProjectV2ItemById.item.id;
}

function setText(itemId, field, value) {
  graphql(
    `mutation($p:ID!,$i:ID!,$f:ID!,$v:String!){updateProjectV2ItemFieldValue(
       input:{projectId:$p,itemId:$i,fieldId:$f,value:{text:$v}}){projectV2Item{id}}}`,
    { p: schema().projectId, i: itemId, f: fieldId(field), v: value },
    'setText',
  );
}

function setSelect(itemId, field, value) {
  graphql(
    `mutation($p:ID!,$i:ID!,$f:ID!,$v:String!){updateProjectV2ItemFieldValue(
       input:{projectId:$p,itemId:$i,fieldId:$f,value:{singleSelectOptionId:$v}}){projectV2Item{id}}}`,
    { p: schema().projectId, i: itemId, f: fieldId(field), v: optionId(field, value) },
    'setSelect',
  );
}

// GitHub rejects a text field value over 1024 characters ("Column value must be a valid
// value for text column") and accepts exactly 1024, so a footprint built by concatenation
// can end mid-path and still be a valid write. Refusing here names the field and the
// length instead of the opaque error.
const TEXT_CAP = 1024;

// Pure: the (field, value) pairs a flag set asks for, validated against the config only.
function fieldWrites(flags) {
  const out = [];
  for (const name of ALL_FIELDS) {
    const val = flags[flagOf(name)];
    if (val === undefined) continue;
    if (val === true) throw new Error(`--${flagOf(name)} needs a value`);
    if (TEXT_FIELDS.includes(name) && String(val).length > TEXT_CAP) {
      throw new Error(`${name} is ${String(val).length} characters; GitHub's cap is ${TEXT_CAP}`);
    }
    out.push([name, String(val)]);
  }
  return out;
}

function checkedWrites(flags) {
  try { return fieldWrites(flags); } catch (e) { die(e.message); }
}

function applyFields(itemId, writes) {
  for (const [name, val] of writes) {
    if (SELECT_FIELDS.includes(name)) setSelect(itemId, name, val);
    else setText(itemId, name, val);
  }
}

// ---------------------------------------------------------------- arg parsing

function parseArgs(argv) {
  const positional = [];
  const flags = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a.startsWith('--')) {
      const key = a.slice(2);
      const next = argv[i + 1];
      if (next === undefined || next.startsWith('--')) { flags[key] = true; }
      else { flags[key] = next; i++; }
    } else positional.push(a);
  }
  return { positional, flags };
}

function issueArg(s, cmd) {
  const n = String(s ?? '').replace(/^#/, '');
  if (!/^\d+$/.test(n)) die(`${cmd}: need an issue number, got ${JSON.stringify(s ?? '')}`);
  return n;
}

// ---------------------------------------------------------------- footprints

// A `Files` value is `;`- or `,`-separated paths, each optionally followed by a `(note)`.
// Two whole-segment markers are not paths: `tracking only` (a parent whose children
// carry the real footprints) and `UNSCOPED` (a claim nobody has narrowed yet, which would
// otherwise collide with everything).
const NOT_A_CLAIM = /tracking only|UNSCOPED/i;

// Split only at paren depth zero: a note routinely contains `;` and `,`.
function footprintSegments(s) {
  const out = [];
  let cur = '', depth = 0;
  for (const ch of String(s ?? '')) {
    if (ch === '(') depth++;
    else if (ch === ')') depth = Math.max(0, depth - 1);
    if ((ch === ';' || ch === ',') && depth === 0) { out.push(cur); cur = ''; continue; }
    cur += ch;
  }
  out.push(cur);
  return out;
}

function footprintPaths(files) {
  return footprintSegments(files)
    .filter(seg => !NOT_A_CLAIM.test(seg))
    .map(seg => seg.replace(/\([^]*$/, ' ').trim().replace(/\\/g, '/'))
    .filter(Boolean);
}

// Prefix matching covers a directory claim (`src/game/`) against a file beneath it. A
// glob (`src/game/*.rs`) matches nothing here and reports no clash, so name files.
function sharedPaths(a, b) {
  const A = footprintPaths(a), B = footprintPaths(b);
  return A.filter(x => B.some(y => x === y || x.startsWith(y) || y.startsWith(x)));
}

// ---------------------------------------------------------------- commands

function cmdList(flags) {
  let rows = items({ fresh: !!flags.fresh });
  for (const name of SELECT_FIELDS) {
    const want = flags[flagOf(name)];
    if (want === undefined || want === true) continue;
    rows = rows.filter(i => (i.fields[name] ?? '').toLowerCase() === String(want).toLowerCase());
  }
  if (flags.label) rows = rows.filter(i => i.labels.includes(flags.label));
  if (flags.open) rows = rows.filter(i => i.state === 'OPEN');
  if (!rows.length) { console.log('(no items)'); return; }
  const w = (s, n) => String(s ?? '').padEnd(n).slice(0, n);
  const widths = CFG.listColumns.map(c => {
    const opts = CFG.selectFields[c] ?? [];
    return Math.min(24, Math.max(c.length, ...opts.map(o => o.length), ...rows.map(r => (r.fields[c] ?? '').length)) + 2);
  });
  console.log(w('#', 6) + CFG.listColumns.map((c, k) => w(c.toUpperCase(), widths[k])).join('') + 'TITLE');
  console.log('-'.repeat(100));
  for (const i of rows) {
    console.log(w('#' + i.number, 6) + CFG.listColumns.map((c, k) => w(i.fields[c], widths[k])).join('') + i.title);
  }
  console.log(`${rows.length} item(s)`);
}

function printShow(i, kids) {
  console.log(`#${i.number}  ${i.title}`);
  console.log(`  state       ${i.state}`);
  console.log(`  labels      ${i.labels.join(', ') || '(none)'}`);
  for (const f of ALL_FIELDS) {
    if (i.fields[f]) console.log(`  ${f.padEnd(12)}${i.fields[f]}`);
  }
  // A tracking parent with an invisible child list looks exactly like a live item with
  // an empty footprint, so the children are shown here.
  if (kids.nodes.length) {
    console.log(`  children    ${kids.completed}/${kids.total} complete (${kids.percent}%)`);
    for (const k of kids.nodes) console.log(`              #${k.number} [${k.state}] ${k.title}`);
  }
  console.log(`  url         ${i.url}`);
}

function cmdShow(num) {
  const { item, kids } = showFetch(num);
  if (!item) die(`issue #${num} is not on the board (add it with \`file\`, or it may not exist)`);
  printShow(item, kids);
  return item;
}

function cmdFile(flags) {
  if (!flags.title || flags.title === true) die('file: --title is required');
  if (!flags.body && !flags['body-file']) die('file: --body or --body-file is required');
  const writes = checkedWrites(flags);
  for (const r of CFG.requiredOnFile) {
    if (!writes.some(([n]) => n === r)) die(`file: --${flagOf(r)} is required (see requiredOnFile in the config)`);
  }
  const labels = String(flags.label ?? '').split(',').map(s => s.trim()).filter(Boolean);
  const known = Object.keys(CFG.labels);
  for (const l of labels) {
    if (known.length && !known.includes(l)) die(`file: label "${l}" is not in the config. Labels: ${known.join(', ')}`);
  }
  // Validate every single-select value before creating anything: a bad value caught only
  // by applyFields would leave the issue created and orphaned on the board.
  for (const [name, val] of writes) if (SELECT_FIELDS.includes(name)) optionId(name, val);
  const args = ['issue', 'create', '--repo', REPO, '--title', flags.title];
  if (flags['body-file']) args.push('--body-file', flags['body-file']);
  else args.push('--body', flags.body);
  for (const l of labels) args.push('--label', l);
  const out = gh(args, { json: false, label: 'issue create' }).trim();
  const url = out.split(/\s+/).find(t => t.startsWith('https://'));
  if (!url) die(`could not parse issue url from: ${out}`);
  const number = Number(url.split('/').pop());
  const itemId = addToBoard(number);
  if (!writes.some(([n]) => n === 'Status')) writes.push(['Status', STATUS.todo]);
  applyFields(itemId, writes);
  console.log(`#${number}  ${url}`);
  refreshCacheEntry(number);
}

// Close an item that should never have existed. Not `done`: Done means merged and
// verified, and marking a duplicate Done puts a lie in the one field the board is queried
// on. GitHub's "not planned" close reason is the honest terminal state for a duplicate
// or an obsolete item.
function cmdDrop(num, flags) {
  const why = flags.of ? `Duplicate of #${String(flags.of).replace(/^#/, '')}.` : (flags.reason === true ? '' : flags.reason || '');
  if (!why) die('drop: need --of <#N> or --reason "…"');
  const i = itemFor(num);
  applyFields(i.itemId, [['Status', STATUS.todo]]);   // never leave it reading as In Progress
  gh(['issue', 'comment', num, '--repo', REPO, '--body', why], { json: false, label: 'issue comment' });
  gh(['issue', 'close', num, '--repo', REPO, '--reason', 'not planned'], { json: false, label: 'issue close' });
  console.log(`#${num} closed as not planned: ${why}`);
  refreshCacheEntry(num);
}

// GitHub creates the commit<->issue link from the commit side: a commit whose message
// (or a comment on it) names `#N` shows in issue N's timeline. Going forward the merge
// commit or PR carries `Closes #N`; this exists for the retroactive case, because pushed
// history must not be rewritten to add a trailer. The commit must exist on GitHub.
function cmdLink(num, flags) {
  const sha = flags.commit === true ? '' : flags.commit;
  if (!sha) die('link: need --commit <sha>');
  const git = args => execFileSync('git', args, { encoding: 'utf8', cwd: ROOT }).trim();
  const full = git(['rev-parse', sha]);
  const subject = git(['log', '-1', '--format=%s', full]);
  const i = itemFor(num);
  gh(['api', `repos/${REPO}/commits/${full}/comments`, '-f',
      `body=Merged **#${num}**: ${i.title}.\n\nThe board item carries the verification evidence.`],
     { json: true, label: 'commit comment' });
  gh(['issue', 'comment', num, '--repo', REPO, '--body',
      `Merged to \`${CFG.defaultBranch}\` in ${full}: *${subject}*`], { json: false, label: 'issue comment' });
  console.log(`#${num} <-> ${full.slice(0, 12)}  (${subject.slice(0, 60)})`);
}

// Every field write is a replace: `updateProjectV2ItemFieldValue` sets the field to
// exactly the string given, so a widening write that starts from a stale read silently
// drops whatever it does not repeat, and nothing reports a fence that stopped existing.
// So the old value is printed beside the new one on every text write.
function replacedReport(before, writes) {
  const lines = [];
  if (!before) return lines;
  const split = t => footprintSegments(t).map(x => x.trim()).filter(Boolean);
  for (const [field, now] of writes) {
    if (!TEXT_FIELDS.includes(field)) continue;
    const old = String(before.fields?.[field] ?? '').trim();
    if (!old || old === now.trim()) continue;
    const gone = split(old).filter(x => !split(now).includes(x));
    lines.push(`  REPLACED ${field}`, `    was: ${old}`, `    now: ${now}`);
    if (gone.length) lines.push(`    *** ${gone.length} NO LONGER PRESENT: ${gone.join(' | ')}`);
  }
  return lines;
}

function cmdSet(num, flags) {
  const writes = checkedWrites(flags);
  const hasTitle = typeof flags.title === 'string';
  if (!writes.length && !hasTitle) die(`set: nothing to set. Field flags: ${ALL_FIELDS.map(f => '--' + flagOf(f)).join(' ')} --title`);
  const i = itemFor(num);
  for (const l of replacedReport(i, writes)) console.log(l);
  if (hasTitle) gh(['issue', 'edit', num, '--repo', REPO, '--title', flags.title], { json: false, label: 'issue edit' });
  applyFields(i.itemId, writes);
  const updated = cmdShow(num);
  patchCacheEntry(updated);
}

// Fields carry state; the comment thread carries why the state changed, which is what the
// next reader needs when an item's premise turns out to be wrong.
function cmdNote(num, flags) {
  const args = ['issue', 'comment', num, '--repo', REPO];
  if (typeof flags['body-file'] === 'string') args.push('--body-file', flags['body-file']);
  else if (typeof flags.body === 'string') args.push('--body', flags.body);
  else die('note: need --body or --body-file');
  gh(args, { json: false, label: 'issue comment' });
  console.log(`#${num} commented`);
}

function cmdClaim(num) { cmdSet(num, { [flagOf('Status')]: STATUS.inProgress }); }

function cmdDone(num, flags) {
  const i = itemFor(num);
  // The evidence is commented first, as its own call, so it survives a failed close.
  if (typeof flags['body-file'] === 'string') {
    gh(['issue', 'comment', num, '--repo', REPO, '--body-file', flags['body-file']], { json: false, label: 'issue comment' });
  } else if (typeof flags.comment === 'string') {
    gh(['issue', 'comment', num, '--repo', REPO, '--body', flags.comment], { json: false, label: 'issue comment' });
  }
  applyFields(i.itemId, [['Status', STATUS.done]]);
  // A project's "auto-close issue" workflow closes the issue when Status becomes Done, so
  // this close can race it and lose. An already-closed issue is the outcome wanted.
  let after = null;
  try {
    execFileSync(ghPath(), ['issue', 'close', num, '--repo', REPO], { encoding: 'utf8', stdio: 'pipe' });
  } catch {
    after = itemDirect(num);
    if (after?.state !== 'CLOSED') die(`#${num} is marked Done but did not close (state ${after?.state}); close it by hand`);
  }
  console.log(`#${num} closed and marked Done`);
  patchCacheEntry(after ?? itemDirect(num));
}

// GitHub has native issue dependencies. They are readable through GraphQL (`blockedBy` /
// `blocking`) but settable only through REST. Use them rather than a label or prose: a
// real dependency shows in the UI and is queryable.
function restIssueId(num) {
  return gh(['api', `repos/${REPO}/issues/${num}`, '--jq', '.id'], { label: 'restIssueId' });
}

function numberList(s, cmd, flag) {
  if (!s || s === true) die(`${cmd}: ${flag} <#N[,#M]> is required`);
  return String(s).split(',').map(x => issueArg(x.trim(), cmd));
}

function cmdBlock(num, flags) {
  for (const by of numberList(flags.by, 'block', '--by')) {
    gh(['api', '--method', 'POST', `repos/${REPO}/issues/${num}/dependencies/blocked_by`,
        '-F', `issue_id=${restIssueId(by)}`, '--jq', '.number'], { json: false, label: 'blocked_by' });
    console.log(`#${num} blocked by #${by}`);
  }
}

// Sub-issues are hierarchy, distinct from dependencies (which must land first). GitHub
// computes the parent's completion for free.
function cmdSub(parent, flags) {
  const kids = numberList(flags.add, 'sub', '--add');
  const pid = issueNodeId(parent);
  for (const c of kids) {
    graphql(
      `mutation($p:ID!,$c:ID!){addSubIssue(input:{issueId:$p,subIssueId:$c}){issue{number}}}`,
      { p: pid, c: issueNodeId(c) },
      'addSubIssue',
    );
    console.log(`#${c} is now a sub-issue of #${parent}`);
  }
  const d = graphql(
    `query($o:String!,$r:String!,$n:Int!){repository(owner:$o,name:$r){issue(number:$n){
       subIssuesSummary{total completed percentCompleted}
       subIssues(first:50){nodes{number title state}}}}}`,
    { o: REPO_OWNER, r: REPO_NAME, n: Number(parent) },
    'subIssuesSummary',
  );
  const i = d.repository.issue;
  console.log(`  ${i.subIssuesSummary.completed}/${i.subIssuesSummary.total} complete (${i.subIssuesSummary.percentCompleted}%)`);
  for (const n of i.subIssues.nodes) console.log(`  #${n.number} [${n.state}] ${n.title}`);
}

function cmdDeps(num) {
  const d = graphqlOrNull(
    `query($o:String!,$r:String!,$n:Int!){repository(owner:$o,name:$r){issue(number:$n){
       blockedBy(first:20){nodes{number title state}}
       blocking(first:20){nodes{number title state}}
     }}}`,
    { o: REPO_OWNER, r: REPO_NAME, n: Number(num) },
    'deps',
  );
  const i = d?.repository?.issue;
  if (!i) die(`issue #${num} not found in ${REPO}`);
  const show = (label, nodes) => {
    if (!nodes.length) { console.log(`  ${label}: none`); return; }
    for (const n of nodes) console.log(`  ${label}: #${n.number} [${n.state}] ${n.title}`);
  };
  console.log(`#${num} dependencies`);
  show('blocked by', i.blockedBy.nodes);
  show('blocking  ', i.blocking.nodes);
}

// Which queued or in-flight items share a file? The conflict unit is the file, so two
// items whose Files intersect must not be worked in parallel.
function cmdFences(flags) {
  if (!TEXT_FIELDS.includes(FILES_FIELD)) die(`fences: the config has no "${FILES_FIELD}" text field`);
  const live = items({ fresh: !!flags.fresh })
    .filter(i => i.state === 'OPEN' && (i.fields.Status === STATUS.inProgress || i.fields.Status === STATUS.todo));
  let clash = 0;
  for (let a = 0; a < live.length; a++) {
    for (let b = a + 1; b < live.length; b++) {
      const shared = sharedPaths(live[a].fields[FILES_FIELD], live[b].fields[FILES_FIELD]);
      if (shared.length) {
        clash++;
        console.log(`CLASH  #${live[a].number} (${live[a].fields.Status}) vs #${live[b].number} (${live[b].fields.Status})`);
        for (const s of shared) console.log(`         ${s}`);
      }
    }
  }
  const unfenced = live.filter(i => !footprintPaths(i.fields[FILES_FIELD]).length && !NOT_A_CLAIM.test(i.fields[FILES_FIELD] ?? ''));
  if (!clash) console.log(`no file-footprint clashes among ${live.length} open Todo/In Progress item(s)`);
  else console.log(`\n${clash} clash(es): serialise these, or narrow a Files field if the overlap is not real.`);
  if (unfenced.length) console.log(`${unfenced.length} of them have no Files and cannot clash: ${unfenced.map(i => '#' + i.number).join(' ')}`);
}

function cmdFields() {
  for (const [n, f] of Object.entries(schema().fields)) {
    console.log(`${n.padEnd(22)}${f.options ? f.options.map(o => o.name).join(', ') : `(${String(f.dataType ?? 'text').toLowerCase()})`}`);
  }
}

// Runs one gh command for `setup`, returning the failure text instead of exiting, so one
// refused change does not stop the independent ones after it.
function ghTry(args, label) {
  return trace(label, () => {
    try {
      execFileSync(ghPath(), args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
      return null;
    } catch (e) {
      return (e.stderr || e.stdout || e.message || '').toString().trim().split('\n')[0];
    }
  });
}

// Compares the live project and repo with the config. Without --apply it only reports.
// With --apply it links the project to the repo, creates missing fields and labels; it
// never deletes or renames anything, and it cannot add options to an existing
// single-select (gh has no command for that), which it reports for a hand fix.
//
// Project changes need access granted on the project itself: a public project is
// readable by anyone, and write access to a linked repo grants nothing on the board.
function cmdSetup(flags) {
  const apply = !!flags.apply;
  const s = schema();
  const changes = [];   // { what, project: bool, args }
  console.log(`project  ${s.title}  ${s.url}`);
  console.log(`access   ${s.canUpdate ? 'you can edit this project' : 'READ ONLY: the project owner must add you under Settings > Manage access (Write for items, Admin for fields)'}`);
  if (!s.repos.includes(REPO)) {
    changes.push({ what: `link the project to ${REPO}`, project: true,
                   args: ['project', 'link', String(projectNumber()), '--owner', OWNER, '--repo', REPO] });
  }
  for (const [name, options] of Object.entries(CFG.selectFields)) {
    const f = s.fields[name];
    if (!f) {
      changes.push({ what: `create single-select field ${name}: ${options.join(', ')}`, project: true,
                     args: ['project', 'field-create', String(projectNumber()), '--owner', OWNER, '--name', name,
                            '--data-type', 'SINGLE_SELECT', '--single-select-options', options.join(',')] });
    } else if (!f.options) {
      console.log(`  MANUAL  field ${name} exists but is not a single-select`);
    } else {
      const missing = options.filter(o => !f.options.some(x => x.name.toLowerCase() === o.toLowerCase()));
      if (missing.length) console.log(`  MANUAL  field ${name} lacks option(s): ${missing.join(', ')} (add them in the project's field settings)`);
    }
  }
  for (const name of TEXT_FIELDS) {
    const f = s.fields[name];
    if (!f) {
      changes.push({ what: `create text field ${name}`, project: true,
                     args: ['project', 'field-create', String(projectNumber()), '--owner', OWNER, '--name', name, '--data-type', 'TEXT'] });
    } else if (f.options) {
      console.log(`  MANUAL  field ${name} exists but is a single-select, not text`);
    }
  }
  const have = gh(['label', 'list', '--repo', REPO, '--json', 'name', '--limit', '500'], { label: 'label list' }).map(l => l.name);
  for (const [name, description] of Object.entries(CFG.labels)) {
    if (have.includes(name)) continue;
    changes.push({ what: `create label ${name}`, project: false,
                   args: ['label', 'create', name, '--repo', REPO, '--description', description] });
  }
  if (!changes.length) { console.log('  project, fields and labels match the config'); return; }
  if (!apply) {
    for (const c of changes) console.log(`  MISSING ${c.what}`);
    console.log(`\n${changes.length} change(s); rerun with --apply to make them`);
    return;
  }
  let failed = 0;
  for (const c of changes) {
    if (c.project && !s.canUpdate) { failed++; console.log(`  BLOCKED ${c.what} (no project access)`); continue; }
    const err = ghTry(c.args, c.args.slice(0, 2).join(' '));
    if (err) { failed++; console.log(`  FAILED  ${c.what}: ${err}`); }
    else console.log(`  DONE    ${c.what}`);
  }
  if (failed) { console.log(`\n${failed} of ${changes.length} change(s) not made`); process.exitCode = 1; }
}

function cmdCacheInfo() {
  const cache = readCache();
  console.log(`path   ${cachePath()}`);
  if (!cache) { console.log('(no cache for this project)'); return; }
  const age = Date.now() - cache.fetchedAt;
  console.log(`age    ${(age / 1000).toFixed(1)}s`);
  console.log(`items  ${cache.items.length}`);
  console.log(`fresh  ${cacheFresh(cache) ? 'yes' : 'no'} (ttl ${CACHE_TTL_MS / 1000}s)`);
}

// Offline fixtures over the pure functions: the config validator, flag mapping, the text
// cap, the Files parser, the replace report and the cache. No disk, no network.
function cmdSelftest() {
  let bad = 0, n = 0;
  const check = (name, got, want) => {
    n++;
    const pass = JSON.stringify(got) === JSON.stringify(want);
    if (!pass) bad++;
    console.log(`${pass ? 'ok  ' : 'FAIL'}  ${name}`);
    if (!pass) console.log(`          got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`);
  };
  const throws = fn => { try { fn(); return null; } catch (e) { return e.message; } };
  const base = { repo: 'o/r', projectOwner: 'o', selectFields: { Status: ['Todo', 'In Progress', 'Done'] } };

  check('the shipped config is valid', throws(() => normalizeConfig(JSON.parse(readFileSync(CONFIG_PATH, 'utf8')))), null);
  check('a minimal config is valid', throws(() => normalizeConfig(base)), null);
  check('owner type defaults to user', normalizeConfig(base).projectOwnerType, 'user');
  check('a repo without an owner is refused', !!throws(() => normalizeConfig({ ...base, repo: 'r' })), true);
  check('a Status field missing Done is refused',
    !!throws(() => normalizeConfig({ ...base, selectFields: { Status: ['Todo', 'In Progress'] } })), true);
  check('a field spelled like a reserved flag is refused',
    !!throws(() => normalizeConfig({ ...base, textFields: ['Label'] })), true);
  check('two fields sharing a flag are refused',
    !!throws(() => normalizeConfig({ ...base, textFields: ['Due date', 'due  date'] })), true);
  check('requiredOnFile must name a field', !!throws(() => normalizeConfig({ ...base, requiredOnFile: ['Nope'] })), true);
  check('a string project number is refused', !!throws(() => normalizeConfig({ ...base, projectNumber: '3' })), true);
  check('field flags are lowercase and dashed', flagOf('Roadmap IDs'), 'roadmap-ids');

  check('parseArgs: a flag followed by a flag is boolean',
    parseArgs(['list', '--open', '--status', 'Todo']), { positional: ['list'], flags: { open: true, status: 'Todo' } });
  check('fieldWrites keeps only configured fields, as strings',
    fieldWrites({ status: 'Todo', title: 'x', nonsense: 'y' }), [['Status', 'Todo']]);
  check('fieldWrites refuses a field flag with no value', !!throws(() => fieldWrites({ status: true })), true);
  if (TEXT_FIELDS.length) {
    const t = flagOf(TEXT_FIELDS[0]);
    check('a text value at the cap is accepted', throws(() => fieldWrites({ [t]: 'a'.repeat(TEXT_CAP) })), null);
    check('a text value over the cap is refused', !!throws(() => fieldWrites({ [t]: 'a'.repeat(TEXT_CAP + 1) })), true);
  }

  check('footprints split on ; and ,', footprintPaths('a.rs; b.rs, c.rs'), ['a.rs', 'b.rs', 'c.rs']);
  check('a note in parens is stripped, and its ; does not split',
    footprintPaths('src/a.rs (only the tests; not the fn); src/b.rs'), ['src/a.rs', 'src/b.rs']);
  check('tracking-only and UNSCOPED segments are not paths',
    footprintPaths('(tracking only - children carry the footprints); UNSCOPED'), []);
  check('backslashes normalize to slashes', footprintPaths('src\\game\\ai.rs'), ['src/game/ai.rs']);
  check('an empty Files has no paths', footprintPaths(undefined), []);
  check('the same file clashes', sharedPaths('src/game/ai.rs', 'src/game/ai.rs; src/app.rs'), ['src/game/ai.rs']);
  check('a directory claim clashes with a file beneath it', sharedPaths('src/game/', 'src/game/ai.rs'), ['src/game/']);
  check('sibling files do not clash', sharedPaths('src/game/ai.rs', 'src/game/ui.rs'), []);

  check('a replaced Files names what was dropped',
    replacedReport({ fields: { Files: 'a.rs; b.rs' } }, [['Files', 'a.rs']]).at(-1), '    *** 1 NO LONGER PRESENT: b.rs');
  check('an unchanged value reports nothing', replacedReport({ fields: { Files: 'a.rs' } }, [['Files', 'a.rs']]), []);

  const now = 1_000_000_000_000;
  check('a missing cache is never fresh', cacheFresh(null, now), false);
  check('a cache fetched right now is fresh', cacheFresh({ fetchedAt: now }, now), true);
  check('a cache exactly at the TTL boundary is stale', cacheFresh({ fetchedAt: now - CACHE_TTL_MS }, now), false);
  check('a cache one millisecond inside the TTL is fresh', cacheFresh({ fetchedAt: now - (CACHE_TTL_MS - 1) }, now), true);
  const a = { number: 1, title: 'a', fields: {} };
  const b = { number: 2, title: 'b', fields: {} };
  const b2 = { number: 2, title: 'b updated', fields: {} };
  check('patching an existing number replaces it in place, keeping fetchedAt',
    applyCachePatch({ fetchedAt: now, items: [a, b] }, b2), { fetchedAt: now, items: [a, b2] });
  check('patching a new number appends and re-sorts, keeping fetchedAt',
    applyCachePatch({ fetchedAt: now, items: [b] }, a), { fetchedAt: now, items: [a, b] });
  check('removing a number drops only that entry', applyCacheRemoval({ fetchedAt: now, items: [a, b] }, 1), { fetchedAt: now, items: [b] });
  check('removing from no cache is a no-op', applyCacheRemoval(null, 1), null);

  // A planted control: a check that must fail, so a reporter that stopped counting
  // failures is itself caught.
  const before = bad;
  const log = console.log;
  console.log = () => {};
  check('planted control', 1, 2);
  console.log = log;
  const plantedSeen = bad === before + 1;
  bad = before;
  n--;
  if (!plantedSeen) { bad++; console.log('FAIL  the planted control was not counted as a failure'); }

  console.log('');
  console.log(`${n} check(s), ${bad} failure(s)`);
  if (bad) process.exitCode = 1;
}

// ---------------------------------------------------------------- main

const USAGE = `usage: node .claude/skills/board/board.mjs <command>

  Board: ${REPO} -> ${OWNER_KIND} ${OWNER}'s project #${CFG.projectNumber ?? '(unset)'}
  Fields: ${ALL_FIELDS.map(f => '--' + flagOf(f)).join(' ')}

  list [--<select-field> V] [--label L] [--open] [--fresh]
  show <#>                     one issue, fetched directly (never the cache)
  fences [--fresh]             open Todo/In Progress items whose Files overlap
  deps <#>                     what blocks it, and what it blocks
  fields                       the project's fields and their options
  setup [--apply]              compare the project and repo with the config; --apply
                               links the repo and creates missing fields and labels
  cache                        whole-board cache status (10-minute TTL; --fresh bypasses)
  selftest                     offline fixtures over the pure helpers

  file --title T (--body B | --body-file F) [--label a,b] [field flags]
  set <#> [field flags] [--title T]
  note <#> (--body B | --body-file F)      comment without touching Status
  claim <#>                    Status -> In Progress
  done <#> [--comment C | --body-file F]   Status -> Done and close the issue
  drop <#> (--of <#N> | --reason R)        close as NOT PLANNED (duplicate/obsolete)
  block <#> --by <#N[,#M]>     native GitHub issue dependency
  sub <parent#> --add <#N[,#M]>  make issues sub-issues of a parent
  link <#> --commit <sha>      two-way link to a pushed commit (retroactive only)`;

const [, , cmd, ...rest] = process.argv;
const { positional, flags } = parseArgs(rest);
const num = name => issueArg(positional[0], name);

switch (cmd) {
  case 'list': cmdList(flags); break;
  case 'show': cmdShow(num('show')); break;
  case 'file': cmdFile(flags); break;
  case 'set': cmdSet(num('set'), flags); break;
  case 'claim': cmdClaim(num('claim')); break;
  case 'done': cmdDone(num('done'), flags); break;
  case 'block': cmdBlock(num('block'), flags); break;
  case 'sub': cmdSub(num('sub'), flags); break;
  case 'deps': cmdDeps(num('deps')); break;
  case 'fences': cmdFences(flags); break;
  case 'link': cmdLink(num('link'), flags); break;
  case 'note': cmdNote(num('note'), flags); break;
  case 'drop': cmdDrop(num('drop'), flags); break;
  case 'fields': cmdFields(); break;
  case 'setup': cmdSetup(flags); break;
  case 'cache': cmdCacheInfo(); break;
  case 'selftest': cmdSelftest(); break;
  default: console.log(USAGE);
}
