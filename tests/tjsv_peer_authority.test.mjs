import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const DRAFT = 'https://json-schema.org/draft/2020-12/schema';
const root = resolve('contracts/tjsv-canary');
const tsp = join(root, 'main.tsp');
const schema = join(root, 'authored.schema.json');
const instances = join(root, 'instances');
const tool = resolve('tmp/tjsv/bin/typespec-json-schema-validator.mjs');
const out = resolve('tmp/tjsv-canary-evidence');
mkdirSync(out, { recursive: true });
const digest = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));

function execute(label, schemaPath, expected) {
  const dir = join(out, label);
  mkdirSync(dir, { recursive: true });
  const generatedDir = join(dir, 'generated');
  const reportPath = join(dir, 'report.json');
  const irPath = join(dir, 'contract-ir.json');
  const generated = join(generatedDir, 'typespec.generated.schema.json');
  const result = spawnSync(process.execPath, [
    tool, 'check',
    `--typespec=${tsp}`,
    `--schema=${schemaPath}`,
    `--instances=${instances}`,
    `--report=${reportPath}`,
    `--contract-ir=${irPath}`,
    `--output-dir=${generatedDir}`,
    '--max-probes=64',
    '--seal-object-schemas=true',
    '--polymorphic-models-strategy=oneOf',
  ], { encoding: 'utf8', timeout: 90000, maxBuffer: 8 * 1024 * 1024 });
  assert.ifError(result.error);
  assert.equal(result.signal, null, label);
  assert.equal(result.status, expected, `${label}: ${result.stdout}\n${result.stderr}`);
  return {
    result,
    report: existsSync(reportPath) ? readJson(reportPath) : null,
    ir: existsSync(irPath) ? readJson(irPath) : null,
    generated,
  };
}

function assertPeerReceipt(report) {
  assert.ok(report, 'TJSV report is required for semantic parity verdicts');
  assert.equal(report.authorities.typespec.authority, 'independently-authored');
  assert.equal(report.authorities.typespec.generatedJsonSchemaRole, 'comparison-evidence-only');
  assert.equal(report.authorities.jsonSchema.authority, 'independently-authored');
  assert.equal(report.authorities.jsonSchema.dialect, DRAFT);
  assert.equal(report.authorities.precedence, 'none');
  assert.equal(report.authorities.onUnexplainedMismatch, 'STOPPED_FOR_EVALUATION');
  assert.equal(report.coverage.directDeclarationInventory, true);
  assert.equal(report.coverage.typespecGeneratedJsonSchemaComparison, true);
  assert.equal(report.coverage.differentialInstanceValidation, true);
}

test('TypeSpec and authored JSON Schema remain independent while generated Schema B is compared', () => {
  const before = { tsp: digest(tsp), schema: digest(schema) };
  const accepted = execute('accepted', schema, 0);
  assert.equal(accepted.report.status, 'passed');
  assert.equal(accepted.report.zeroUnexplainedFindings, true);
  assert.equal(accepted.ir.admissible, true);
  assertPeerReceipt(accepted.report);
  assert.equal(existsSync(accepted.generated), true);
  assert.equal(readJson(accepted.generated).$schema, DRAFT);
  assert.deepEqual({ tsp: digest(tsp), schema: digest(schema) }, before);

  const compare = spawnSync(process.execPath, [
    tool, 'compare',
    `--typespec=${tsp}`,
    `--generated-schema=${accepted.generated}`,
    `--schema=${schema}`,
    `--instances=${instances}`,
    `--report=${join(out, 'compare.report.json')}`,
    '--max-probes=64',
  ], { encoding: 'utf8', timeout: 90000, maxBuffer: 8 * 1024 * 1024 });
  assert.ifError(compare.error);
  assert.equal(compare.status, 0, `${compare.stdout}\n${compare.stderr}`);
  assertPeerReceipt(readJson(join(out, 'compare.report.json')));
  assert.deepEqual({ tsp: digest(tsp), schema: digest(schema) }, before);
});

test('drift in authored Schema A blocks even when TypeSpec and generated Schema B stay stable', () => {
  const mutatedPath = join(out, 'drifted.authored.schema.json');
  const drift = readJson(schema);
  drift.$defs.LeaseProbe.properties.active.type = 'string';
  writeFileSync(mutatedPath, `${JSON.stringify(drift, null, 2)}\n`);
  const rejected = execute('drifted', mutatedPath, 2);
  assert.equal(rejected.report.status, 'stopped_for_evaluation');
  assert.equal(rejected.ir.admissible, false);
  assertPeerReceipt(rejected.report);
  assert(rejected.report.findings.some(finding => finding.ruleId === 'generated-authored-semantic-mismatch'));
});

test('missing authored peer fails closed instead of promoting generated evidence', () => {
  const missing = execute('missing-peer', join(out, 'missing.schema.json'), 3);
  assert.equal(existsSync(missing.generated), false, 'missing authored authority must not promote generated Schema B');
  if (missing.report) assert.equal(missing.report.status, 'failed');
  if (missing.ir) assert.equal(missing.ir.admissible, false);
  assert.match(`${missing.result.stdout}\n${missing.result.stderr}`, /missing|ENOENT|not exist|read/i);
});
