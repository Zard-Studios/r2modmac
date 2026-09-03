import assert from 'node:assert/strict';
import test from 'node:test';
import {
  isAdtentionConnectionEnabled,
  normalizeSponsor,
  validateSponsorRequest,
} from '../src/sponsor.ts';

const validRequest = {
  category: 'general' as const,
  placement: 'home-support',
  subject: '6f7c41f4-a42d-4cb5-bd44-123456789abc',
};

test('enables the ADtention connection only through an explicit true switch', () => {
  assert.equal(isAdtentionConnectionEnabled('true'), true);
  assert.equal(isAdtentionConnectionEnabled(' TRUE '), true);
  assert.equal(isAdtentionConnectionEnabled('false'), false);
  assert.equal(isAdtentionConnectionEnabled(undefined), false);
});

test('accepts only the minimal expected request', () => {
  assert.deepEqual(validateSponsorRequest(validRequest), validRequest);
  assert.equal(validateSponsorRequest({ ...validRequest, profile: 'private' }), undefined);
  assert.equal(validateSponsorRequest({ ...validRequest, subject: 'username' }), undefined);
  assert.equal(validateSponsorRequest({ ...validRequest, placement: 'unknown' }), undefined);
});

test('takes any of the network\'s six categories and refuses invented ones', () => {
  for (const category of ['web3', 'web', 'devops', 'data', 'systems', 'general']) {
    assert.deepEqual(
      validateSponsorRequest({ ...validRequest, category }),
      { ...validRequest, category },
      category,
    );
  }
  assert.equal(validateSponsorRequest({ ...validRequest, category: 'gaming' }), undefined);
  assert.equal(validateSponsorRequest({ ...validRequest, category: 42 }), undefined);
});

test('the category every released build sends is read as general, not rejected', () => {
  assert.deepEqual(
    validateSponsorRequest({ ...validRequest, category: 'gaming-mod-manager' }),
    { ...validRequest, category: 'general' },
  );
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
    billable: true,
    category: 'general',
    url: 'https://example.com/',
  });
  assert.equal(normalizeSponsor({ ...base, clickUrl: 'http://example.com/' }), undefined);
  assert.equal(normalizeSponsor({ ...base, text: 'x'.repeat(281) }), undefined);
});

test('carries the billable flag through, so a filled impression that earned nothing is visible', () => {
  const base = {
    impressionId: 'imp_test_456',
    text: 'A short sponsor message',
    clickUrl: null,
    adId: 'ad_1',
    category: 'general' as const,
    credit: 0,
    fromCache: false,
  };

  assert.equal(normalizeSponsor({ ...base, billable: false })?.billable, false);
  assert.equal(normalizeSponsor({ ...base, billable: true })?.billable, true);
});
