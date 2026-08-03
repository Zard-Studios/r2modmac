import test from 'node:test';
import assert from 'node:assert/strict';

// Kept dependency-free so it can run in the Vercel service without SDK credentials.
const valid = (candidate) => {
  if (!candidate || typeof candidate !== 'object') return false;
  if (!candidate.id || !candidate.message) return false;
  if (candidate.url && !candidate.url.startsWith('https://')) return false;
  return true;
};

test('only accepts minimal, HTTPS sponsor payloads', () => {
  assert.equal(valid({ id: 's1', message: 'A short message', url: 'https://example.com' }), true);
  assert.equal(valid({ id: 's1', message: 'A short message', url: 'http://example.com' }), false);
  assert.equal(valid({ id: '', message: 'A short message' }), false);
});
