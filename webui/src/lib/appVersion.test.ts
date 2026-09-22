import assert from 'node:assert/strict';
import { test } from 'node:test';
import { formatAppVersionLabel, normalizeAppVersion } from './appVersion.ts';

test('normalizeAppVersion strips a leading v and trims', () => {
  assert.equal(normalizeAppVersion('v7.0.13'), '7.0.13');
  assert.equal(normalizeAppVersion('7.0.13'), '7.0.13');
  assert.equal(normalizeAppVersion('  V7.0.13  '), '7.0.13');
  assert.equal(normalizeAppVersion(''), '');
  assert.equal(normalizeAppVersion(undefined), '');
});

test('formatAppVersionLabel always shows a single v prefix', () => {
  assert.equal(formatAppVersionLabel('7.0.13'), 'v7.0.13');
  assert.equal(formatAppVersionLabel('v7.0.13'), 'v7.0.13');
  assert.equal(formatAppVersionLabel(''), '');
});
