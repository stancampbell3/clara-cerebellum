// @ts-check
const { test, expect } = require('@playwright/test');

async function login(page, username) {
  await page.goto('/');
  await page.fill('#login-username', username);
  await page.click('#login-submit');
  await expect(page.locator('#input')).toBeEnabled({ timeout: 10_000 });
}

// Assistant replies arrive as markdown; renderMessageHtml (backed by the
// vendored marked.js) is responsible for turning that into real HTML
// instead of showing literal asterisks/pound signs. These inject a bubble
// directly (same technique as copy-buttons.spec.js) rather than depending
// on a live LLM reply.
test.describe('markdown rendering', () => {
  test('headings, bold, italic, and lists render as real elements, not literal markdown', async ({ page }) => {
    await login(page, `pw-md-basic-${Date.now()}`);

    const reply = [
      '# Heading',
      '',
      'Some **bold** and *italic* text.',
      '',
      '- item one',
      '- item two',
    ].join('\n');

    await page.evaluate((text) => {
      // @ts-ignore
      window.appendBubble('agent', text, 'TestBot');
    }, reply);

    const bubble = page.locator('.bubble.agent').last();
    await expect(bubble.locator('h1')).toHaveText('Heading');
    await expect(bubble.locator('strong')).toHaveText('bold');
    await expect(bubble.locator('em')).toHaveText('italic');
    await expect(bubble.locator('ul li')).toHaveCount(2);
    await expect(bubble.locator('ul li').first()).toHaveText('item one');

    // The raw markdown syntax must not appear as literal visible text.
    const visibleText = await bubble.innerText();
    expect(visibleText).not.toContain('**');
    expect(visibleText).not.toContain('# Heading');
  });

  test('fenced code blocks still get the copy-button/hljs treatment after markdown rendering', async ({ page }) => {
    await login(page, `pw-md-code-${Date.now()}`);

    const snippet = 'console.log("hi")';
    const reply = 'Here:\n\n```js\n' + snippet + '\n```\n';

    await page.evaluate((text) => {
      // @ts-ignore
      window.appendBubble('agent', text, 'TestBot');
    }, reply);

    const codeBlock = page.locator('pre.code-block').last();
    await expect(codeBlock).toBeVisible();
    await expect(codeBlock.locator('code')).toContainText(snippet);
    await expect(codeBlock.locator('.code-lang')).toHaveText('js');
    await expect(codeBlock.locator('.copy-code-btn')).toBeVisible();
  });

  test('markdown links render as real anchors, but a javascript: URL is neutralized', async ({ page }) => {
    await login(page, `pw-md-links-${Date.now()}`);

    const reply = 'See [the docs](https://example.com/docs) or a [bad link](javascript:alert(1)).';

    await page.evaluate((text) => {
      // @ts-ignore
      window.appendBubble('agent', text, 'TestBot');
    }, reply);

    const bubble = page.locator('.bubble.agent').last();
    const link = bubble.locator('a', { hasText: 'the docs' });
    await expect(link).toHaveAttribute('href', 'https://example.com/docs');
    await expect(link).toHaveAttribute('target', '_blank');
    await expect(link).toHaveAttribute('rel', /noopener/);

    // The unsafe link must not become a real anchor at all.
    await expect(bubble.locator('a', { hasText: 'bad link' })).toHaveCount(0);
    await expect(bubble).toContainText('bad link');
  });

  test('copying a rendered markdown bubble puts the original raw markdown on the clipboard, not HTML', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    await login(page, `pw-md-copy-${Date.now()}`);

    const reply = '# Title\n\nSome **bold** text.';

    await page.evaluate((text) => {
      // @ts-ignore
      window.appendBubble('agent', text, 'TestBot');
    }, reply);

    const bubble = page.locator('.bubble.agent').last();
    await bubble.hover();
    await bubble.locator('.copy-btn').click();
    await expect(bubble.locator('.copy-btn')).toHaveClass(/copied/);

    const clipboardText = await page.evaluate(() => navigator.clipboard.readText());
    expect(clipboardText).toBe(reply);
  });

  test('literal HTML embedded in a reply is displayed as text, never executed', async ({ page }) => {
    await login(page, `pw-md-xss-${Date.now()}`);

    let dialogFired = false;
    page.on('dialog', async (d) => { dialogFired = true; await d.dismiss(); });

    const reply = 'Before <img src=x onerror="window.__xss=true"> after.';

    await page.evaluate((text) => {
      // @ts-ignore
      window.appendBubble('agent', text, 'TestBot');
    }, reply);

    const bubble = page.locator('.bubble.agent').last();
    await expect(bubble.locator('img')).toHaveCount(0);
    await expect(bubble).toContainText('<img src=x onerror="window.__xss=true">');
    const xssRan = await page.evaluate(() => /** @type {any} */ (window).__xss === true);
    expect(xssRan).toBe(false);
    expect(dialogFired).toBe(false);
  });
});
