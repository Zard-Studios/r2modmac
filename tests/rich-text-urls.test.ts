import assert from 'node:assert/strict';
import test from 'node:test';

import { attachmentLinesToVideos, isAllowedMediaUrl, isSafeLinkUrl } from '../src/utils/richTextUrls.ts';

test('only GitHub-hosted pictures and videos are allowed', () => {
    assert.ok(isAllowedMediaUrl('https://github.com/user-attachments/assets/0a1b2c3d-0000-4000-8000-000000000000'));
    assert.ok(isAllowedMediaUrl('https://user-images.githubusercontent.com/1/pic.png'));
    assert.ok(isAllowedMediaUrl('https://github.com/Zard-Studios/r2modmac/raw/main/docs/pic.png'));
    assert.ok(!isAllowedMediaUrl('http://github.com/user-attachments/assets/x'));
    assert.ok(!isAllowedMediaUrl('https://example.com/pic.png'));
    assert.ok(!isAllowedMediaUrl('https://githubusercontent.com.example.com/pic.png'));
    assert.ok(!isAllowedMediaUrl('https://github.com/other/repo/raw/main/pic.png'));
    assert.ok(!isAllowedMediaUrl('javascript:alert(1)'));
    assert.ok(!isAllowedMediaUrl('not a url'));
});

test('links open over https or as mail, and nothing else', () => {
    assert.ok(isSafeLinkUrl('https://github.com/Zard-Studios/r2modmac/releases'));
    assert.ok(isSafeLinkUrl('mailto:someone@example.com'));
    assert.ok(!isSafeLinkUrl('http://example.com'));
    assert.ok(!isSafeLinkUrl('javascript:alert(1)'));
    assert.ok(!isSafeLinkUrl('file:///etc/passwd'));
    assert.ok(!isSafeLinkUrl('#section'));
});

test('mod descriptions may also link to plain http sites', () => {
    assert.ok(isSafeLinkUrl('http://example.com/page', true));
    assert.ok(!isSafeLinkUrl('http://example.com/page'), 'announcements stay on https');
    assert.ok(isSafeLinkUrl('https://example.com', true));
    assert.ok(!isSafeLinkUrl('javascript:alert(1)', true));
    assert.ok(!isSafeLinkUrl('file:///etc/passwd', true));
    assert.ok(!isSafeLinkUrl('data:text/html,<script>alert(1)</script>', true));
});

test('a bare upload link on its own line becomes a video player', () => {
    const url = 'https://github.com/user-attachments/assets/0a1b2c3d-0000-4000-8000-000000000000';
    const out = attachmentLinesToVideos(`Watch this:\n\n${url}\n\nand ${url} inside a sentence stays text`);

    assert.match(out, /<video controls preload="metadata" src="https:\/\/github\.com\/user-attachments\/assets\/0a1b/);
    assert.equal(out.match(/<video/g)?.length, 1, 'a link inside a sentence is left alone');
});
