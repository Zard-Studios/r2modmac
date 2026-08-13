import { isCategory, type Category, type Sponsor } from '@adtention/sdk';

const LEGACY_CATEGORY = 'gaming-mod-manager';

const PLACEMENTS = new Set([
  'preferences-support',
  'home-support',
  'profile-selector-support',
  'catalog-support',
]);
const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

export type SponsorRequest = {
  category: Category;
  placement: string;
  subject: string;
};

export type SponsorPayload = {
  id: string;
  message: string;
  url?: string;
  /** Which bucket actually served the ad. Absent on a deduped replay. */
  category?: string;
  /** `false` when the serve was throttled, daily-capped, or a deduped replay. */
  billable: boolean;
};

export function text(value: unknown, maximum: number): string | undefined {
  return typeof value === 'string' && value.trim().length > 0 && value.length <= maximum
    ? value.trim()
    : undefined;
}

function httpsUrl(value: unknown): string | undefined {
  const raw = text(value, 2_048);
  if (!raw) return undefined;
  try {
    const url = new URL(raw);
    return url.protocol === 'https:' && url.hostname ? url.toString() : undefined;
  } catch {
    return undefined;
  }
}

export function normalizeSponsor(candidate: Sponsor | null): SponsorPayload | undefined {
  if (!candidate) return undefined;
  const id = text(candidate.impressionId, 128);
  const message = text(candidate.text, 280);
  const rawUrl = candidate.clickUrl;
  const url = rawUrl === null ? undefined : httpsUrl(rawUrl);
  if (!id || !message || (rawUrl !== null && !url)) return undefined;
  return {
    id,
    message,
    billable: candidate.billable === true,
    ...(candidate.category ? { category: candidate.category } : {}),
    ...(url ? { url } : {}),
  };
}

export function validateSponsorRequest(candidate: unknown): SponsorRequest | undefined {
  if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) return undefined;
  const body = candidate as Record<string, unknown>;
  const category = body.category === LEGACY_CATEGORY ? 'general' : body.category;

  if (
    !isCategory(category)
    || typeof body.placement !== 'string'
    || !PLACEMENTS.has(body.placement)
    || typeof body.subject !== 'string'
    || !UUID_PATTERN.test(body.subject)
    || Object.keys(body).some((key) => !['category', 'placement', 'subject'].includes(key))
  ) {
    return undefined;
  }
  return {
    category,
    placement: body.placement,
    subject: body.subject,
  };
}
