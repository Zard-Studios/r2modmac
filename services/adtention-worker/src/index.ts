import { SponsorSlot, type AdtentionError, type Category } from '@adtention/sdk';
import {
  isAdtentionConnectionEnabled,
  normalizeSponsor,
  text,
  validateSponsorRequest,
} from './sponsor';

const SPONSOR_PATH = '/api/sponsor';
const MAX_REQUEST_BYTES = 2_048;

const ALL_CATEGORIES: readonly Category[] = [
  'general',
  'web',
  'devops',
  'data',
  'systems',
  'web3',
];

function categoriesToTry(requested: Category): Category[] {
  return [requested, ...ALL_CATEGORIES.filter((category) => category !== requested)];
}

const RESPONSE_HEADERS = {
  'cache-control': 'no-store',
  'x-content-type-options': 'nosniff',
  'referrer-policy': 'no-referrer',
};

function noContent(reason?: string, staging = false): Response {
  const headers = new Headers(RESPONSE_HEADERS);
  if (staging && reason) headers.set('x-r2modmac-sponsor-status', reason);
  return new Response(null, { status: 204, headers });
}

async function readBoundedJson(request: Request): Promise<unknown | undefined> {
  const declaredLength = Number(request.headers.get('content-length') ?? 0);
  if (Number.isFinite(declaredLength) && declaredLength > MAX_REQUEST_BYTES) return undefined;
  if (!request.body) return undefined;

  const reader = request.body.getReader();
  const decoder = new TextDecoder();
  let decoded = '';
  let received = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      received += value.byteLength;
      if (received > MAX_REQUEST_BYTES) return undefined;
      decoded += decoder.decode(value, { stream: true });
    }
    decoded += decoder.decode();
    return JSON.parse(decoded);
  } catch {
    return undefined;
  } finally {
    await reader.cancel().catch(() => undefined);
  }
}

export default {
  async fetch(request, env): Promise<Response> {
    const url = new URL(request.url);
    const staging = env.DEPLOYMENT_ENV === 'staging';
    if (
      request.method !== 'POST'
      || url.pathname !== SPONSOR_PATH
      || !request.headers.get('content-type')?.toLowerCase().startsWith('application/json')
    ) {
      return noContent('route_rejected', staging);
    }

    // Keep this gate ahead of request parsing and SDK setup so disabling the connection
    // guarantees that no request reaches ADtention. It can be restored at the Worker
    // configuration layer without rebuilding or redistributing the desktop app.
    if (!isAdtentionConnectionEnabled(env.ADTENTION_CONNECTION_ENABLED)) {
      return noContent('connection_disabled', staging);
    }

    const publisherId = text(env.ADTENTION_PUBLISHER_ID, 128);
    const body = validateSponsorRequest(await readBoundedJson(request));
    if (!publisherId) return noContent('publisher_missing', staging);
    if (!body) return noContent('request_rejected', staging);
    const clientIp = request.headers.get('cf-connecting-ip') ?? request.headers.get('x-forwarded-for');
    const userAgent = request.headers.get('user-agent');
    const customFetch: typeof fetch = (input, init) => {
      const headers = new Headers(init?.headers);
      if (clientIp) {
        headers.set('x-forwarded-for', clientIp);
      }
      if (userAgent) {
        headers.set('user-agent', userAgent);
      }
      return fetch(input, { ...init, headers });
    };

    try {
      let sdkError: AdtentionError | undefined;

      for (const category of categoriesToTry(body.category)) {
        const slot = new SponsorSlot({
          publisherId,
          serveOnly: true,
          category,
          timeoutMs: 4_500,
          fetch: customFetch,
          onError: (error) => {
            sdkError = error;
          },
        });
        const sponsor = normalizeSponsor(await slot.next({ subject: body.subject }));
        if (sponsor) {
          return Response.json(sponsor, { headers: RESPONSE_HEADERS });
        }
      }


      return noContent(sdkError?.code ?? 'no_inventory', staging);
    } catch {
      return noContent('unexpected_error', staging);
    }
  },
} satisfies ExportedHandler<Env>;
