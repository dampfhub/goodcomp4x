#!/usr/bin/env node
//
// size-budget.mjs -- a ratchet on file length, so big files can only shrink.
//
// Every agent that reads a file pays for its length, and one big file is one fence
// that parallel work queues behind. A code file may have up to `budget.code` lines,
// a dedicated test file up to `budget.tests` (`size-budget.json`). A file already
// over its budget is listed in `ceilings` at the length it had when it was listed:
// it may shrink, never grow. A new file over budget fails.
//
// When a listed file shrinks, `--update` lowers its ceiling to match (it never
// raises one) and drops files that are gone or back under budget. The check only
// notes a ceiling that could come down, so two PRs that shrink files don't
// conflict over the JSON.
//
// A file that is too long should be split. When growing it is the better change
// (splitting would scatter one idea, say), raise or add its ceiling by hand in
// `size-budget.json` in the same PR, and give the reason in the PR description,
// so a reviewer sees it.
//
// Counted: files git tracks or would add (not ignored) with an extension in `extensions`.
// Lines are counted like `wc -l`, plus a last line with no newline.
//
// USE
//   node tools/size-budget.mjs              # check; exit 1 if a file is too long
//   node tools/size-budget.mjs --update     # lower ceilings to the current lengths
//   node tools/size-budget.mjs --self-test

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const CONFIG = path.join(ROOT, 'tools', 'size-budget.json');

// Pure: the number of lines in `text`, as `wc -l` counts them plus an unterminated last one.
export function countLines(text) {
  if (text.length === 0) return 0;
  let n = 0;
  for (let i = 0; i < text.length; i++) if (text.charCodeAt(i) === 10) n++;
  return text.endsWith('\n') ? n : n + 1;
}

// Pure: whether `file` (a repo path with forward slashes) is a dedicated test file.
// A pattern ending in `/` matches everything under that directory, or with no other
// `/` (`tests/`), under any directory of that name. Any other pattern with a `/`
// matches that path, and one without matches that file name anywhere.
export function isTestFile(file, patterns) {
  return patterns.some((p) => {
    if (p.endsWith('/')) {
      if (p.indexOf('/') < p.length - 1) return file.startsWith(p);
      return file.startsWith(p) || file.includes(`/${p}`);
    }
    if (p.includes('/')) return file === p;
    return file === p || file.endsWith(`/${p}`);
  });
}

// Pure: whether `file` is counted at all.
export function isCounted(file, config) {
  return config.extensions.includes(path.posix.extname(file));
}

// Pure: the budget `file` gets with no ceiling.
export function budgetFor(file, config) {
  return isTestFile(file, config.tests) ? config.budget.tests : config.budget.code;
}

// Pure: checks `lengths` (path -> lines, counted files only) against `config`.
// Returns { failures, lowerable, stale }: files over their limit, listed files that
// shrank (their ceiling could come down), and listed files that are gone or under budget.
export function check(lengths, config) {
  const failures = [];
  const lowerable = [];
  const stale = [];
  for (const [file, lines] of Object.entries(lengths)) {
    const budget = budgetFor(file, config);
    const ceiling = config.ceilings[file];
    if (ceiling === undefined) {
      if (lines > budget) failures.push({ file, lines, limit: budget, listed: false });
    } else if (lines > ceiling) {
      failures.push({ file, lines, limit: ceiling, listed: true, budget });
    } else if (lines <= budget) {
      stale.push({ file, lines, ceiling });
    } else if (lines < ceiling) {
      lowerable.push({ file, lines, ceiling });
    }
  }
  for (const file of Object.keys(config.ceilings)) {
    if (!(file in lengths)) stale.push({ file, lines: 0, ceiling: config.ceilings[file] });
  }
  const byFile = (a, b) => a.file.localeCompare(b.file);
  return { failures: failures.sort(byFile), lowerable: lowerable.sort(byFile), stale: stale.sort(byFile) };
}

// Pure: the ceilings after `--update`: lowered to current lengths, never raised,
// gone and under-budget files dropped. New files over budget are not added.
export function updatedCeilings(lengths, config) {
  const next = {};
  for (const [file, ceiling] of Object.entries(config.ceilings)) {
    const lines = lengths[file];
    if (lines === undefined || lines <= budgetFor(file, config)) continue;
    next[file] = Math.min(lines, ceiling);
  }
  return Object.fromEntries(Object.entries(next).sort(([a], [b]) => a.localeCompare(b)));
}

function readConfig() {
  return JSON.parse(fs.readFileSync(CONFIG, 'utf8'));
}

function measure(config) {
  // Tracked files, and new ones git doesn't ignore, so a check before committing sees them.
  const args = ['ls-files', '-z', '--cached', '--others', '--exclude-standard'];
  const files = execFileSync('git', args, { cwd: ROOT, encoding: 'utf8' })
    .split('\0')
    .filter((f) => f && isCounted(f, config) && fs.existsSync(path.join(ROOT, f)));
  return Object.fromEntries(files.map((f) => [f, countLines(fs.readFileSync(path.join(ROOT, f), 'utf8'))]));
}

function writeConfig(config) {
  fs.writeFileSync(CONFIG, `${JSON.stringify(config, null, 2)}\n`);
}

function runCheck() {
  const config = readConfig();
  const { failures, lowerable, stale } = check(measure(config), config);
  for (const f of failures) {
    const why = f.listed
      ? `over its ceiling of ${f.limit} (budget ${f.budget}); it may only shrink`
      : `over the ${f.limit}-line budget`;
    console.log(`TOO LONG  ${f.file}: ${f.lines} lines, ${why}. Split it.`);
  }
  if (failures.length) {
    console.log('If it should grow, set its ceiling in tools/size-budget.json and say why in the PR.');
  }
  for (const f of lowerable) {
    console.log(`note      ${f.file}: ${f.lines} lines, ceiling ${f.ceiling}; --update lowers it`);
  }
  for (const f of stale) {
    console.log(`note      ${f.file}: ${f.lines} lines, ceiling ${f.ceiling}; --update drops it`);
  }
  console.log(failures.length ? `SIZE-FAILED ${failures.length}` : 'SIZE-OK');
  return failures.length ? 1 : 0;
}

function runUpdate() {
  const config = readConfig();
  const lengths = measure(config);
  const before = config.ceilings;
  config.ceilings = updatedCeilings(lengths, config);
  writeConfig(config);
  for (const [file, ceiling] of Object.entries(before)) {
    const now = config.ceilings[file];
    if (now !== ceiling) console.log(`${file}: ${ceiling} -> ${now ?? 'removed'}`);
  }
  const { failures } = check(lengths, config);
  for (const f of failures) console.log(`TOO LONG  ${f.file}: ${f.lines} lines, limit ${f.limit}`);
  console.log(failures.length ? `SIZE-FAILED ${failures.length}` : 'SIZE-OK');
  return failures.length ? 1 : 0;
}

function runSelfTest() {
  const config = {
    budget: { code: 10, tests: 20 },
    extensions: ['.rs', '.mjs'],
    tests: ['tests.rs', 'tests/', 'src/sim.rs', 'src/sim/'],
    ceilings: { 'src/big.rs': 15, 'src/shrunk.rs': 30, 'src/gone.rs': 12, 'src/small.rs': 14 },
  };
  let passed = 0;
  const failed = [];
  const expect = (name, got, want) => {
    const [g, w] = [JSON.stringify(got), JSON.stringify(want)];
    if (g === w) passed++;
    else failed.push(`${name}: got ${g}, want ${w}`);
  };
  expect('empty', countLines(''), 0);
  expect('one terminated line', countLines('a\n'), 1);
  expect('unterminated last line', countLines('a\nb'), 2);
  expect('CRLF counts once', countLines('a\r\nb\r\n'), 2);
  expect('tests.rs anywhere', isTestFile('src/game/ui/tests.rs', config.tests), true);
  expect('not a suffix match', isTestFile('src/game/mytests.rs', config.tests), false);
  expect('exact path', isTestFile('src/sim.rs', config.tests), true);
  expect('exact path elsewhere', isTestFile('src/game/sim.rs', config.tests), false);
  expect('directory', isTestFile('src/sim/economy.rs', config.tests), true);
  expect('directory name anywhere', isTestFile('src/ui/tests/frames.rs', config.tests), true);
  expect('directory name at the root', isTestFile('tests/a.rs', config.tests), true);
  expect('not a directory name suffix', isTestFile('src/mytests/a.rs', config.tests), false);
  expect('counted', isCounted('a/b.rs', config), true);
  expect('not counted', isCounted('docs/a.md', config), false);
  const lengths = {
    'src/big.rs': 16, // over its ceiling
    'src/shrunk.rs': 25, // shrank: lowerable
    'src/small.rs': 9, // back under budget: stale
    'src/new.rs': 11, // new and over budget
    'src/ok.rs': 10, // at budget
    'src/tests.rs': 20, // a test file at its budget
    'src/sim/x.rs': 21, // a test file over its budget
  };
  const result = check(lengths, config);
  expect(
    'failures',
    result.failures.map((f) => [f.file, f.limit]),
    [['src/big.rs', 15], ['src/new.rs', 10], ['src/sim/x.rs', 20]],
  );
  expect('lowerable', result.lowerable.map((f) => f.file), ['src/shrunk.rs']);
  expect('stale', result.stale.map((f) => f.file), ['src/gone.rs', 'src/small.rs']);
  expect('a file at its ceiling passes', check({ 'src/big.rs': 15 }, config).failures, []);
  expect('update lowers, never raises, drops gone and small', updatedCeilings(lengths, config), {
    'src/big.rs': 15,
    'src/shrunk.rs': 25,
  });
  for (const f of failed) console.log(`FAIL ${f}`);
  console.log(failed.length ? `SELFTEST-FAILED ${failed.length}` : `SELFTEST-OK ${passed}`);
  return failed.length ? 1 : 0;
}

// Run only as a script, so the pure functions above can be imported.
const isMain = process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href;
const arg = process.argv[2];
if (!isMain) {
  // Imported.
} else if (arg === undefined) process.exitCode = runCheck();
else if (arg === '--update') process.exitCode = runUpdate();
else if (arg === '--self-test') process.exitCode = runSelfTest();
else {
  console.log('usage: node tools/size-budget.mjs [--update | --self-test]');
  process.exitCode = 2;
}
