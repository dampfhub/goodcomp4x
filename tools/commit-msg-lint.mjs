#!/usr/bin/env node
//
// commit-msg-lint.mjs -- refuse a commit message that closes an issue its author
// cannot see it closing.
//
// GitHub scans a commit message for `<closing keyword> <issue reference>` and closes
// the referenced issue once the commit reaches the default branch. A newline is
// whitespace to that scan and a line break to the author, so a keyword ending one
// line binds to a reference opening the next:
//
//     ...and together these changes close
//     #534.
//
// closes #534, and nothing in the message looks wrong to the person who wrote it.
// The shape refused is any closure binding whose keyword and reference are on
// different lines. A binding on ONE line (`Closes #12`) is visible, so it is
// reported and allowed; `--declared N` turns its count into an exit code, for a
// merge body that should close exactly one item.
//
// What this cannot catch:
//   1. The keyword and reference tables are transcriptions of GitHub's documented
//      sets. Nothing here observes GitHub, so a keyword GitHub adds later is linted
//      clean.
//   2. It reads commit messages. A pull-request description closes issues by the same
//      rule and is not checked.
//   3. Markup around either half (`**Closes** #1`, `(#5)`) is not modelled and is
//      admitted.
//   4. It over-refuses a few messages GitHub closes nothing through (`fixed` / `#0`,
//      `fixed` / `docs/a.md#3`); the cost is rewording one line.
//   5. `--declared` checks the count of same-line bindings, never which issue they
//      name.
//
// The self-test runs on every invocation, before any message is read, and no verdict
// is printed if it fails. The keyword table is stated three times (CLOSING_KEYWORDS,
// KEYWORD_WITNESSES, KEYWORD_ROSTER) and the reference table twice (REFERENCE_FORMS,
// REFERENCE_SAMPLES) and censused against each other: a case matrix crossed over a
// table cannot see the table shrink, because the cases vanish with the entry.
//
// USE
//   node tools/commit-msg-lint.mjs                  # <upstream default branch>..HEAD
//   node tools/commit-msg-lint.mjs --range A..B
//   node tools/commit-msg-lint.mjs --rev <sha>
//   node tools/commit-msg-lint.mjs --file <path>    # a git commit-msg hook's argument
//   node tools/commit-msg-lint.mjs --file <body> --declared 1   # a merge body
//   node tools/commit-msg-lint.mjs --stdin
//   node tools/commit-msg-lint.mjs --all            # every commit in `git log --all`
//   node tools/commit-msg-lint.mjs --self-test
//
// The verdict is the LAST stdout line:
//   MSG-OK <n> | MSG-REFUSED <n> | MSG-DECLARED-WRONG <n> | SELFTEST-OK <n> |
//   SELFTEST-FAILED <n> | HELP-OK <n> | USAGE <text> | INTERNAL-ERROR <text>
// Exit codes: 0 ok, 3 refused, 5 declared count wrong, 4 self-test failed,
//             2 usage, 1 internal error.

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

// GitHub's documented closing keywords.
const CLOSING_KEYWORDS = [
  'close',
  'closes',
  'closed',
  'fix',
  'fixes',
  'fixed',
  'resolve',
  'resolves',
  'resolved',
];

// Near a keyword but not one; an unanchored matcher accepts these.
const NEAR_MISS_WORDS = [
  'prefix',
  'closing',
  'closer',
  'fixing',
  'fixture',
  'unfix',
  'resolver',
  'resolving',
  'disclose',
  'foreclosed',
];

// Issue-reference spellings. Only a token's head is matched (`#12's`, `#12.` bind);
// digits running into a letter (`#5abc`) are not a reference.
const REFERENCE_FORMS = [
  { name: 'plain', re: /^#\d+(?![A-Za-z0-9])/, sample: '#1026' },
  { name: 'gh-prefixed', re: /^GH-\d+(?![A-Za-z0-9])/i, sample: 'GH-1026' },
  { name: 'cross-repo', re: /^[A-Za-z0-9._-]+\/[A-Za-z0-9._-]+#\d+(?![A-Za-z0-9])/, sample: 'owner/repo#1026' },
  {
    name: 'issue-url',
    re: /^https?:\/\/(?:www\.)?github\.com\/[A-Za-z0-9._-]+\/[A-Za-z0-9._-]+\/issues\/\d+(?![A-Za-z0-9])/i,
    sample: 'https://github.com/owner/repo/issues/1026',
  },
];

// Written out literally, not derived from REFERENCE_FORMS, so deleting a form fails a
// case here instead of silently deleting its cases.
const REFERENCE_SAMPLES = ['#1026', 'GH-1026', 'owner/repo#1026', 'https://github.com/owner/repo/issues/1026'];

// One literal straddle per keyword, for the same reason.
const KEYWORD_WITNESSES = [
  ['close', 'chore: a subject\n\nprose that ends with close\n#1 opens the next line\n'],
  ['closes', 'chore: a subject that closes\n\n#2 opens the body\n'],
  ['closed', 'chore: a subject now closed\n\n#3 opens the body\n'],
  ['fix', 'chore: a subject to fix\n\n#4 opens the body\n'],
  ['fixes', 'chore: a subject that fixes\n\n#5 opens the body\n'],
  ['fixed', 'chore: a subject now fixed\n\n#6 opens the body\n'],
  ['resolve', 'chore: a subject to resolve\n\n#7 opens the body\n'],
  ['resolves', 'chore: a subject that resolves\n\n#8 opens the body\n'],
  ['resolved', 'chore: a subject now resolved\n\n#9 opens the body\n'],
];

// A third statement, as a string: one `grep -v "'close',"` removes the entry from both
// arrays above and leaves them agreeing with each other.
const KEYWORD_ROSTER = 'close closes closed fix fixes fixed resolve resolves resolved';
const KEYWORD_ROSTER_SIZE = 9;

const KEYWORD_SET = new Set(CLOSING_KEYWORDS.map((k) => k.toLowerCase()));

// ---------------------------------------------------------------- the check

// All three break spellings: a lone CR is a line break to the author and whitespace to
// GitHub, and splitting on /\r?\n/ reads `closes\r#5` as a visible same-line binding.
function messageLines(msg) {
  return String(msg).split(/\r\n|\n|\r/);
}

function tokenize(msg) {
  const out = [];
  messageLines(msg).forEach((line, lineIndex) => {
    for (const m of line.matchAll(/\S+/g)) out.push({ line: lineIndex, text: m[0] });
  });
  return out;
}

// Zero-width and control characters are stripped from both halves before classifying:
// the author cannot see them, which is this tool's whole subject. This refuses more,
// the safe direction for a lint whose cost is rewording a line.
const INVISIBLE = /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f­​-‏⁠-⁤﻿]/g;

function visible(token) {
  return token.replace(INVISIBLE, '');
}

// A keyword position is a closing keyword, bare or with one trailing colon. Any other
// trailing character sits between keyword and reference and GitHub does not bind.
function keywordAt(token0) {
  const token = visible(token0);
  const bare = token.endsWith(':') ? token.slice(0, -1) : token;
  const lower = bare.toLowerCase();
  return KEYWORD_SET.has(lower) ? lower : null;
}

function referenceAt(token0) {
  const token = visible(token0);
  for (const form of REFERENCE_FORMS) {
    const m = token.match(form.re);
    if (m) return { form: form.name, text: m[0] };
  }
  return null;
}

function bindings(msg) {
  const toks = tokenize(msg);
  const found = [];
  for (let i = 0; i + 1 < toks.length; i++) {
    const kw = keywordAt(toks[i].text);
    if (!kw) continue;
    const ref = referenceAt(toks[i + 1].text);
    if (!ref) continue;
    found.push({
      keyword: kw,
      keywordLine: toks[i].line,
      reference: ref.text,
      referenceForm: ref.form,
      referenceLine: toks[i + 1].line,
      sameLine: toks[i].line === toks[i + 1].line,
    });
  }
  return found;
}

function lintMessage(msg) {
  const all = bindings(msg);
  return { refusals: all.filter((b) => !b.sameLine), declared: all.filter((b) => b.sameLine) };
}

// ---------------------------------------------------------------- the self-test

// Two real-world shapes of the defect: a subject ending in a keyword with the body
// opening on a reference, and a wrapped prose line ending in a keyword.
const SUBJECT_BODY_INCIDENT = [
  "probe: the acceptance program is byte-identical base vs fixed",
  "",
  "#1026's band is untouched: the direct array return still records",
  "the owner on both builds.",
  "",
].join('\n');

const WRAPPED_PROSE_INCIDENT = [
  "docs(handoff): rewrite from the board rather than the last one",
  "",
  "#918 and #948 came unblocked yesterday when #533 and #917 merged, and together they close",
  "#534.",
  "",
].join('\n');

// Mentions the same issue with no keyword adjacent; must pass.
const CONTROL_MSG = [
  "test: pin every slot, and record what is not built",
  "",
  "The union fixture carries a different name on each side. This touches nothing",
  "#1026 is about.",
  "",
].join('\n');

const CANARY_CASE = 'planted-control/the-runner-must-report-this';

function capitalisations(word) {
  return [word.toLowerCase(), word[0].toUpperCase() + word.slice(1).toLowerCase(), word.toUpperCase()];
}

function selfTestCases() {
  const cases = [];
  const add = (name, msg, expect, why) => cases.push({ name, msg, expect, why });

  for (const kw of CLOSING_KEYWORDS) {
    for (const cap of capitalisations(kw)) {
      for (const form of REFERENCE_FORMS) {
        const r = form.sample;
        add(`straddle/subject-body/${cap}/${form.name}`, `probe: a subject that ends in ${cap}\n\n${r}'s band is untouched\n`, 'REFUSE', 'keyword ends the subject, reference opens the body');
        add(`straddle/wrapped-prose/${cap}/${form.name}`, `subject line\n\nprose that wraps and ends with ${cap}\n${r}.\n`, 'REFUSE', 'keyword ends a wrapped line, reference opens the next');
        add(`straddle/colon/${cap}/${form.name}`, `subject line\n\ntrailer paragraph ${cap}:\n${r}\n`, 'REFUSE', 'GitHub binds through one trailing colon');
        add(`same-line/${cap}/${form.name}`, `subject line\n\n${cap} ${r}\n`, 'PASS', 'a one-line binding is visible');
        add(`punctuated/${cap}/${form.name}`, `probe: a subject that ends in ${cap}.\n\n${r}'s band is untouched\n`, 'PASS', 'punctuation sits between keyword and reference');
        add(`not-adjacent/${cap}/${form.name}`, `probe: ${cap} the thing that broke\n\n${r}'s band is untouched\n`, 'PASS', 'another word sits between keyword and reference');
      }
    }
  }
  for (const word of NEAR_MISS_WORDS) {
    for (const form of REFERENCE_FORMS) {
      add(`near-miss/${word}/${form.name}`, `probe: a subject that ends in ${word}\n\n${form.sample} opens the body\n`, 'PASS', `${word} is not a closing keyword`);
    }
  }
  add('bare-word-before-reference', 'subject line\n\na sentence ending in the word board\n#1026 opens the next line\n', 'PASS', 'an ordinary word before a line-initial reference binds nothing');
  add('literal/lone-CR-break-is-a-straddle', 'a\rcloses\r#5\r', 'REFUSE', 'a lone CR is a line break to the author and whitespace to GitHub');
  add('literal/CRLF-break-is-a-straddle', 'a\r\n\r\ncloses\r\n#5\r\n', 'REFUSE', 'the same binding spelled with CRLF');
  add('literal/digits-running-into-letters-are-not-a-reference', 'a\n\ncloses\n#5abc\n', 'PASS', 'GitHub links nothing through #5abc');
  add('literal/issue-url-is-a-reference', 'chore: a subject now Fixes\nhttps://github.com/owner/repo/issues/1026\n', 'REFUSE', 'GitHub closes through the full issue URL');
  add('literal/hash-zero-is-refused', 'a\n\nfixed\n#0\n', 'REFUSE', 'pins the declared over-refusal (gap 4); not a claim that refusing #0 is right');
  // Keywords spelled inside prose, never as the quoted bare token the tables use, so an
  // edit removing a keyword from all three tables cannot take these with it.
  add('literal/keyword-straddle/closed', 'chore: a subject that is now closed\n\n#11 opens the body\n', 'REFUSE', 'GitHub closes through `closed`');
  add('literal/keyword-straddle/fix', 'chore: a subject we still have to fix\n\n#12 opens the body\n', 'REFUSE', 'GitHub closes through `fix`');
  add('literal/keyword-straddle/resolve', 'chore: a subject we still have to resolve\n\n#13 opens the body\n', 'REFUSE', 'GitHub closes through `resolve`');
  add('literal/keyword-straddle/resolves', 'chore: a subject that resolves\n\n#14 opens the body\n', 'REFUSE', 'GitHub closes through `resolves`');
  add('literal/keyword-straddle/resolved', 'chore: a subject now resolved\n\n#15 opens the body\n', 'REFUSE', 'GitHub closes through `resolved`');
  add('literal/zero-width-glued-to-the-keyword', 'a\n\nclose​\n#5\n', 'REFUSE', 'the author sees `close`');
  add('literal/zero-width-glued-to-the-reference', 'a\n\ncloses\n​#5\n', 'REFUSE', 'the author sees `#5`');
  add('literal/soft-hyphen-glued-to-the-keyword', 'a\n\nclose­\n#5\n', 'REFUSE', 'U+00AD renders as nothing in most fonts');
  add('literal/a-visible-non-keyword-is-still-inert', 'a\n\nboard\n#5\n', 'PASS', 'stripping invisibles must not make every word a keyword');
  add('incident/subject-body', SUBJECT_BODY_INCIDENT, 'REFUSE', 'subject ends `fixed`, body opens `#1026`');
  add('incident/wrapped-prose', WRAPPED_PROSE_INCIDENT, 'REFUSE', 'a wrapped line ends `close`, the next opens `#534.`');
  add('incident/control', CONTROL_MSG, 'PASS', 'mentions #1026 with no keyword adjacent');
  // Deliberately wrong expectation, last: the runner must report it, or it reports nothing.
  add(CANARY_CASE, 'chore: a subject that is now fixed\n\n#1 opens the body\n', 'PASS', 'deliberately wrong');
  return cases;
}

function runSelfTest() {
  const cases = selfTestCases();
  const failures = [];
  const fail = (name, expect, got, why) => failures.push({ name, expect, got, why });
  let refuseSeen = 0;
  let passSeen = 0;

  for (const c of cases) {
    const got = lintMessage(c.msg).refusals.length > 0 ? 'REFUSE' : 'PASS';
    if (got === 'REFUSE') refuseSeen++;
    else passSeen++;
    if (got !== c.expect) fail(c.name, c.expect, got, c.why);
  }
  if (refuseSeen === 0 || passSeen === 0) {
    fail('matrix/both-verdicts-observed', 'both REFUSE and PASS', `REFUSE=${refuseSeen} PASS=${passSeen}`, 'a checker with one answer would otherwise score clean');
  }

  for (const [kw, witness] of KEYWORD_WITNESSES) {
    if (lintMessage(witness).refusals.length === 0) fail(`census/keyword-acts/${kw}`, 'REFUSE', 'PASS', `a straddle through "${kw}" is admitted`);
  }
  const roster = KEYWORD_ROSTER.split(' ');
  for (const [label, list] of [['CLOSING_KEYWORDS', CLOSING_KEYWORDS], ['KEYWORD_WITNESSES', KEYWORD_WITNESSES.map(([k]) => k)]]) {
    const set = new Set(list.map((k) => k.toLowerCase()));
    if (set.size !== KEYWORD_ROSTER_SIZE) fail(`census/roster-size/${label}`, `${KEYWORD_ROSTER_SIZE}`, `${set.size}`, 'the keyword lists disagree in size');
    for (const kw of roster) if (!set.has(kw)) fail(`census/roster-member/${label}/${kw}`, 'present', 'absent', 'KEYWORD_ROSTER names it');
  }
  for (const [kw] of KEYWORD_WITNESSES) {
    if (!KEYWORD_SET.has(kw)) fail(`census/witness-is-a-keyword/${kw}`, 'in CLOSING_KEYWORDS', 'absent', 'the witness list and the table disagree');
  }
  for (const word of NEAR_MISS_WORDS) {
    if (KEYWORD_SET.has(word)) fail(`census/tables-disjoint/${word}`, 'not a keyword', 'a keyword', 'the two tables contradict each other');
  }
  for (const witness of [
    'merge: the warm pass not the prefix\n\n#604 opens the body\n',
    'chore: a subject that is closing\n\n#605 opens the body\n',
    'chore: a subject with a resolver\n\n#606 opens the body\n',
  ]) {
    if (lintMessage(witness).refusals.length > 0) fail(`census/near-miss-inert/${messageLines(witness)[0]}`, 'PASS', 'REFUSE', 'binds through a non-keyword');
  }
  for (const sample of REFERENCE_SAMPLES) {
    if (!referenceAt(sample)) fail(`census/sample-recognised/${sample}`, 'recognised', 'not recognised', 'GitHub binds through this spelling');
  }
  for (const form of REFERENCE_FORMS) {
    const own = referenceAt(form.sample);
    if (!own || own.form !== form.name) fail(`census/form-matches-own-sample/${form.name}`, form.name, own ? own.form : 'no match', 'form and sample disagree');
    if (!REFERENCE_SAMPLES.some((s) => (referenceAt(s) || {}).form === form.name)) fail(`census/form-is-sampled/${form.name}`, 'sampled', 'not sampled', 'a form nothing samples can vanish unseen');
  }
  for (const kw of CLOSING_KEYWORDS) {
    const caps = capitalisations(kw);
    if (new Set(caps).size !== 3 || !caps.every((c) => keywordAt(c) === kw)) fail(`census/capitalisations/${kw}`, '3 keyword spellings', 'fewer', 'the matrix crosses this axis');
  }

  // The canary is filtered out rather than spliced, so no single edit to a clearing
  // line can empty the whole list, and its presence is returned separately.
  const canarySeen = failures.some((f) => f.name === CANARY_CASE);
  const reported = failures.filter((f) => f.name !== CANARY_CASE);
  if (!canarySeen) reported.push({ name: 'planted-control/ABSENT', expect: `${CANARY_CASE} reported`, got: 'not reported', why: 'the case runner is not reporting, or the planted message changed' });
  return { total: cases.length, failures: reported, canarySeen, refuseSeen, passSeen };
}

// ---------------------------------------------------------------- message sources

// NUL framing: a commit message cannot contain NUL, so the split is unambiguous.
function messagesFromLog(repoDir, logArgs) {
  const raw = execFileSync('git', ['-C', repoDir, 'log', ...logArgs, '--format=%H%x00%B%x00'], {
    encoding: 'utf8',
    maxBuffer: 512 * 1024 * 1024,
  });
  const parts = raw.split('\0');
  const out = [];
  for (let i = 0; i + 1 < parts.length; i += 2) {
    const id = parts[i].trim();
    if (id) out.push({ id, msg: parts[i + 1] });
  }
  return out;
}

// The default branch comes from `.claude/board.config.json` when present, else the
// usual names are tried.
function defaultRange(repoDir) {
  const names = [];
  try {
    const top = execFileSync('git', ['-C', repoDir, 'rev-parse', '--show-toplevel'], { encoding: 'utf8' }).trim();
    const cfg = JSON.parse(fs.readFileSync(path.join(top, '.claude', 'board.config.json'), 'utf8'));
    if (cfg.defaultBranch) names.push(cfg.defaultBranch);
  } catch {
    /* no config; fall through to the usual names */
  }
  names.push('main', 'master');
  for (const name of names) {
    for (const base of [`origin/${name}`, name]) {
      try {
        execFileSync('git', ['-C', repoDir, 'rev-parse', '--verify', '--quiet', base], { stdio: 'ignore' });
        return `${base}..HEAD`;
      } catch {
        /* try the next one */
      }
    }
  }
  return null;
}

// ---------------------------------------------------------------- main

function help() {
  return [
    'commit-msg-lint.mjs -- refuse a commit message that closes an issue its author',
    'cannot see it closing: a closing keyword and an issue reference on different lines.',
    'A one-line binding (`Closes #12`) is visible, and is reported and allowed.',
    '',
    `Closing keywords: ${CLOSING_KEYWORDS.join(' ')}`,
    'Reference forms:',
    ...REFERENCE_FORMS.map((f) => `  ${f.name.padEnd(12)} e.g. ${f.sample}`),
    '',
    '  --range <A..B>   lint each commit message in the range',
    '  --rev <rev>      lint one commit message',
    '  --file <path>    lint a message file (a commit-msg hook argument)',
    '  --stdin          lint a message on stdin',
    '  --all            lint every message in `git log --all`',
    '  --self-test      run the self-test alone and print it',
    '  --declared N     exit 5 unless exactly N same-line bindings are present',
    '  --json           machine-readable report (lint modes only)',
    '  -C <dir>         repository directory (default: cwd)',
    '',
    'With no mode, lints <origin/default-branch>..HEAD.',
    'Exit codes: 0 ok, 3 refused, 5 declared count wrong, 4 self-test failed,',
    '2 usage, 1 internal error. The verdict is always the LAST stdout line.',
  ].join('\n');
}

function parseArgs(argv) {
  const o = { mode: null, value: null, json: false, dir: process.cwd(), flag: null, dirSeen: false, declared: null };
  // Two subjects is a usage error, not last-wins: a wrapper appending a mode to an
  // existing invocation would otherwise lint something else and still exit 0.
  const setMode = (m, flagText) => {
    if (o.flag !== null) throw new Error(`${o.flag} and ${flagText} each name what to lint; give exactly one`);
    o.mode = m;
    o.flag = flagText;
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const value = () => {
      if (i + 1 >= argv.length) throw new Error(`${a} needs a value`);
      return argv[++i];
    };
    if (a === '--range' || a === '--rev' || a === '--file') {
      setMode(a.slice(2), a);
      o.value = value();
    } else if (a === '--stdin') setMode('stdin', a);
    else if (a === '--all') setMode('all', a);
    else if (a === '--self-test') setMode('self-test', a);
    else if (a === '--help' || a === '-h') setMode('help', a);
    else if (a === '--json') o.json = true;
    else if (a === '--declared') {
      const v = value();
      if (!/^\d+$/.test(v)) throw new Error(`--declared needs a non-negative integer, got ${JSON.stringify(v)}`);
      o.declared = Number(v);
    } else if (a === '-C') {
      if (o.dirSeen) throw new Error('-C given twice; give exactly one');
      o.dirSeen = true;
      o.dir = value();
    } else throw new Error(`unknown argument ${JSON.stringify(a)}`);
  }
  return o;
}

function main() {
  // First, before argv is read, so "runs on every invocation" is a property of the
  // control flow.
  const st = runSelfTest();

  let opts;
  try {
    opts = parseArgs(process.argv.slice(2));
  } catch (e) {
    console.log(help());
    if (st.failures.length) console.log(`SELFTEST-FAILED ${st.failures.length}`);
    console.log(`USAGE ${e.message}`);
    return 2;
  }

  const summary = () => console.log(`self-test: ${st.total} cases (REFUSE=${st.refuseSeen} PASS=${st.passSeen})`);
  // The refusal is its own statement, independent of any mode test, so an edit to what
  // gets printed cannot make a failed self-test silent on a lint path.
  if (st.failures.length) {
    summary();
    for (const f of st.failures) {
      console.log(`  FAILED ${f.name}`);
      console.log(`     expected ${f.expect}, got ${f.got}`);
      console.log(`     ${f.why}`);
    }
    console.log('Change an expectation only after showing, on a real commit, what GitHub does.');
    console.log(`SELFTEST-FAILED ${st.failures.length}`);
    return 4;
  }
  if (!st.canarySeen) {
    summary();
    console.log('  FAILED planted-control/NOT-OBSERVED');
    console.log('SELFTEST-FAILED 1');
    return 4;
  }
  if (opts.mode === 'self-test') {
    summary();
    console.log(`SELFTEST-OK ${st.total}`);
    return 0;
  }
  if (opts.mode === 'help') {
    console.log(help());
    console.log(`HELP-OK ${st.total}`);
    return 0;
  }

  let items;
  let source;
  try {
    if (opts.mode === 'file') {
      items = [{ id: opts.value, msg: fs.readFileSync(opts.value, 'utf8') }];
      source = `file ${opts.value}`;
    } else if (opts.mode === 'stdin') {
      items = [{ id: '<stdin>', msg: fs.readFileSync(0, 'utf8') }];
      source = 'stdin';
    } else if (opts.mode === 'rev') {
      items = messagesFromLog(opts.dir, ['-1', opts.value]);
      source = `rev ${opts.value}`;
    } else if (opts.mode === 'all') {
      items = messagesFromLog(opts.dir, ['--all']);
      source = 'git log --all';
    } else {
      const range = opts.mode === 'range' ? opts.value : defaultRange(opts.dir);
      if (!range) {
        console.log('USAGE no --range given and no default branch resolves');
        return 2;
      }
      items = messagesFromLog(opts.dir, [range]);
      source = `range ${range}`;
    }
  } catch (e) {
    console.log(`INTERNAL-ERROR ${e.message.split('\n')[0]}`);
    return 1;
  }

  const refused = [];
  let declaredTotal = 0;
  for (const it of items) {
    const r = lintMessage(it.msg);
    declaredTotal += r.declared.length;
    if (r.refusals.length) refused.push({ ...it, ...r });
  }

  if (opts.json) {
    console.log(JSON.stringify({
      source,
      linted: items.length,
      declaredBindings: declaredTotal,
      refused: refused.map((r) => ({ id: r.id, subject: messageLines(r.msg)[0], bindings: r.refusals })),
      selfTestCases: st.total,
    }));
  } else {
    console.log(`linted ${items.length} message(s) from ${source}`);
    console.log(`same-line (visible, allowed) bindings: ${declaredTotal}`);
    for (const r of refused) {
      const lines = messageLines(r.msg);
      console.log('');
      console.log(`  REFUSED ${r.id}`);
      console.log(`    ${lines[0]}`);
      for (const b of r.refusals) {
        console.log(`      keyword  line ${b.keywordLine + 1}: ${JSON.stringify(lines[b.keywordLine])}`);
        console.log(`      ref      line ${b.referenceLine + 1}: ${JSON.stringify(lines[b.referenceLine])}`);
        console.log(`      GitHub reads this as "${b.keyword} ${b.reference}" and closes ${b.reference}`);
      }
      console.log('    Fix: put keyword and reference on ONE line if the close is intended;');
      console.log('         otherwise rewrap so the line does not end in the keyword.');
    }
  }

  if (opts.declared !== null && declaredTotal !== opts.declared) {
    console.log(`  DECLARED ${declaredTotal} same-line closure binding(s); --declared ${opts.declared} requires exactly that many`);
    console.log(`MSG-DECLARED-WRONG ${declaredTotal}`);
    return 5;
  }
  if (refused.length) {
    console.log(`MSG-REFUSED ${refused.length}`);
    return 3;
  }
  console.log(`MSG-OK ${items.length}`);
  return 0;
}

let code;
try {
  code = main();
} catch (e) {
  console.log(`INTERNAL-ERROR ${(e && e.message) || e}`);
  code = 1;
}
process.exit(code);
