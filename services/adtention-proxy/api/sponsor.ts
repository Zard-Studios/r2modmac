import type { VercelRequest, VercelResponse } from '@vercel/node';
import { SponsorSlot, type Sponsor } from '@adtention/sdk';

const CATEGORY = 'gaming-mod-manager';
const PLACEMENTS = new Set([
  'preferences-support',
  'profile-selector-support',
  'catalog-support',
]);
const ADTENTION_CATEGORY = 'general' as const;

type SponsorPayload = {
  id: string;
  sponsorName?: string;
  message: string;
  url?: string;
};

function text(value: unknown, maximum: number): string | undefined {
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

// The SDK only exposes safe copy, a safe click URL, and opaque ad/impression identifiers. It does
// not expose an advertiser name, so the UI labels the row simply as "Sponsored".
export function normalizeSponsor(candidate: Sponsor | null): SponsorPayload | undefined {
  if (!candidate) return undefined;
  const id = text(candidate.impressionId, 128);
  const message = text(candidate.text, 280);
  const rawUrl = candidate.clickUrl;
  const url = rawUrl === null ? undefined : httpsUrl(rawUrl);

  if (!id || !message || (rawUrl !== null && !url)) {
    return undefined;
  }
  return { id, message, ...(url ? { url } : {}) };
}

function noContent(response: VercelResponse) {
  return response.status(204).setHeader('Cache-Control', 'no-store').end();
}

export default async function handler(request: VercelRequest, response: VercelResponse) {
  if (request.method !== 'POST') return noContent(response);
  const subject = text(request.body?.subject, 128);
  if (
    request.body?.category !== CATEGORY
    || typeof request.body?.placement !== 'string'
    || !PLACEMENTS.has(request.body.placement)
    || !subject
  ) {
    return noContent(response);
  }

  const publisherId = process.env.ADTENTION_PUBLISHER_ID;
  if (!publisherId) return noContent(response);

  try {
    const slot = new SponsorSlot({ publisherId, serveOnly: true, category: ADTENTION_CATEGORY });
    // `subject` is a random UUID generated locally once per opted-in installation. It is not
    // derived from a person, account, device fingerprint, profile, mod, path, or application use.
    const candidate = await slot.next({ subject });
    const sponsor = normalizeSponsor(candidate);
    if (!sponsor) return noContent(response);
    return response.status(200).setHeader('Cache-Control', 'no-store').json(sponsor);
  } catch {
    return noContent(response);
  }
}
