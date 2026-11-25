#!/usr/bin/env node
//
// check.mjs -- runs exactly what CI runs, so "done" means the same here and there.
// Each job in `.github/workflows/ci.yml` calls this script; the steps live only here.
//
// USE
//   node tools/check.mjs              # every job: rust, msrv and tools
//   node tools/check.mjs --fast       # rust without the tests, and tools (the pre-push hook)
//   node tools/check.mjs <job>...     # some of rust, msrv and tools
//   node tools/check.mjs --list       # print the steps without running them
//   node tools/check.mjs --range A..B # the commits whose messages tools lints
//
// rust uses whatever `cargo` resolves to (CI pins its Rust in ci.yml). msrv checks
// with the Rust in Cargo.toml's `rust-version`, and never installs it: when that
// toolchain isn't installed it is skipped with a note (CI installs it first, and
// there a missing toolchain fails). tools lints the commit messages in
// `origin/main..HEAD` unless given `--range`, and skips that with no `origin/main`;
// it also checks file lengths against their budgets (size-budget.mjs).
//
// It stops at the first failing step and exits non-zero.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const NODE = process.execPath;

// The oldest Rust this crate supports, from Cargo.toml.
function msrv() {
  const toml = fs.readFileSync(path.join(ROOT, 'Cargo.toml'), 'utf8');
  const m = toml.match(/^rust-version\s*=\s*"([^"]+)"/m);
  if (!m) throw new Error('no rust-version in Cargo.toml');
  return m[1];
}

// Whether rustup has toolchain `name` installed. Asks `rustup toolchain list`, which
// never downloads anything.
function hasToolchain(name) {
  const r = spawnSync('rustup', ['toolchain', 'list'], { cwd: ROOT, encoding: 'utf8' });
  if (r.status !== 0) return false;
  return r.stdout.split('\n').some((line) => {
    const t = line.trim().split(/\s/)[0];
    return t === name || t.startsWith(`${name}-`);
  });
}

function hasRef(ref) {
  return spawnSync('git', ['rev-parse', '--verify', '--quiet', ref], { cwd: ROOT }).status === 0;
}

// Each job's steps: a command line, or { skip: why } for a step left out.
function jobs({ fast, range }) {
  const oldest = msrv();
  const lintRange = range || (hasRef('origin/main') ? 'origin/main..HEAD' : null);
  const msrvSteps = () => {
    if (hasToolchain(oldest)) return [['cargo', `+${oldest}`, 'check', '--locked', '--all-targets']];
    const why = `Rust ${oldest} isn't installed (rustup toolchain install ${oldest} --profile minimal)`;
    // In CI the job installs it first, so a miss there is a real failure.
    return [process.env.CI ? { fail: why } : { skip: why }];
  };
  return {
    rust: () => [
      ['cargo', 'fmt', '--check'],
      ['cargo', 'clippy', '--locked', '--all-targets', '--', '-D', 'warnings'],
      fast ? { skip: 'cargo test (--fast)' } : ['cargo', 'test', '--locked'],
    ],
    msrv: () => (fast ? [{ skip: `cargo +${oldest} check (--fast)` }] : msrvSteps()),
    tools: () => [
      [NODE, 'tools/board/board.mjs', 'selftest'],
      [NODE, 'tools/commit-msg-lint.mjs', '--self-test'],
      lintRange
        ? [NODE, 'tools/commit-msg-lint.mjs', '--range', lintRange]
        : { skip: 'commit messages (no origin/main to compare with)' },
      [NODE, 'tools/size-budget.mjs', '--self-test'],
      [NODE, 'tools/size-budget.mjs'],
    ],
  };
}

function parseArgs(argv) {
  const opts = { fast: false, list: false, help: false, range: null, jobs: [] };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--fast') opts.fast = true;
    else if (a === '--list') opts.list = true;
    else if (a === '--range') opts.range = argv[++i];
    else if (a === '-h' || a === '--help') opts.help = true;
    else opts.jobs.push(a);
  }
  return opts;
}

function show(step) {
  return step.map((s) => (s === NODE ? 'node' : s)).join(' ');
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) {
    console.log('usage: node tools/check.mjs [--fast] [--list] [--range A..B] [rust|msrv|tools]...');
    return 0;
  }
  const all = jobs(opts);
  const names = opts.jobs.length ? opts.jobs : Object.keys(all);
  for (const name of names) {
    if (!(name in all)) {
      console.error(`unknown job "${name}"; jobs: ${Object.keys(all).join(', ')}`);
      return 2;
    }
  }
  let skipped = 0;
  for (const name of names) {
    for (const step of all[name]()) {
      if (step.skip) {
        console.log(`[${name}] skipped: ${step.skip}`);
        skipped++;
        continue;
      }
      if (step.fail) {
        console.error(`[${name}] FAILED: ${step.fail}`);
        return 1;
      }
      const line = show(step);
      console.log(`[${name}] ${line}`);
      if (opts.list) continue;
      const started = Date.now();
      // shell: false, so arguments pass through unquoted on every platform.
      const r = spawnSync(step[0], step.slice(1), {
        cwd: ROOT,
        stdio: 'inherit',
        // Never let rustup download a toolchain on its own.
        env: { ...process.env, RUSTUP_AUTO_INSTALL: '0' },
      });
      if (r.error) {
        console.error(`[${name}] could not run ${step[0]}: ${r.error.message}`);
        return 1;
      }
      if (r.status !== 0) {
        console.error(`[${name}] FAILED (exit ${r.status}): ${line}`);
        return r.status || 1;
      }
      console.log(`[${name}] ok in ${((Date.now() - started) / 1000).toFixed(1)} s`);
    }
  }
  const note = skipped ? ` (${skipped} skipped)` : '';
  console.log(opts.list ? 'CHECK-LISTED' : `CHECK-OK ${names.join(' ')}${note}`);
  return 0;
}

process.exitCode = main();
