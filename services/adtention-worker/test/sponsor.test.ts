import assert from 'node:assert/strict';
import test from 'node:test';
import { normalizeSponsor, validateSponsorRequest } from '../src/sponsor.ts';

const validRequest = {
  category: 'gaming-mod-manager',
  placement: 'home-support',
  subject: '6f7c41f4-a42d-4cb5-bd44-123456789abc',
};

test('accepts only the minimal expected request', () => {
  assert.deepEqual(validateSponsorRequest(validRequest), validRequest);
  assert.equal(validateSponsorRequest({ ...validRequest, profile: 'private' }), undefined);
  assert.equal(validateSponsorRequest({ ...validRequest, subject: 'username' }), undefined);
  assert.equal(validateSponsorRequest({ ...validRequest, placement: 'unknown' }), undefined);
});

test('normalizes only bounded sponsor copy and HTTPS links', () => {
  const base = {
    impressionId: 'imp_test_123',
    text: 'A short sponsor message',
    clickUrl: 'https://example.com/',
    adId: 'ad_1',
    category: 'general' as const,
    billable: true,
    credit: 0.01,
    fromCache: false,
  };
  assert.deepEqual(normalizeSponsor(base), {
    id: 'imp_test_123',
    message: 'A short sponsor message',
    url: 'https://example.com/',
  });
  assert.equal(normalizeSponsor({ ...base, clickUrl: 'http://example.com/' }), undefined);
  assert.equal(normalizeSponsor({ ...base, text: 'x'.repeat(281) }), undefined);
});
