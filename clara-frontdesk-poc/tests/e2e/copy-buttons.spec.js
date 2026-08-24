// @ts-check
const { test, expect } = require('@playwright/test');

async function login(page, username) {
  await page.goto('/');
  await page.fill('#login-username', username);
  await page.click('#login-submit');
  // input is enabled once the websocket connects (ws.onopen)
  await expect(page.locator('#input')).toBeEnabled({ timeout: 10_000 });
}

test.describe('copy buttons', () => {
  test.beforeEach(async ({ context }) => {
    // clipboard-read lets us assert on what actually landed on the clipboard;
    // only supported on Chromium.
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  });

  test('copying a user prompt bubble puts its exact text on the clipboard', async ({ page }) => {
    await login(page, `pw-copy-user-${Date.now()}`);

    const text = 'Hello, this is a test prompt with "quotes" & <angle> brackets.';
    await page.fill('#input', text);
    await page.keyboard.press('Control+Enter');

    const bubble = page.locator('.bubble.user').last();
    await expect(bubble).toBeVisible();
    await expect(bubble).toContainText(text);

    // the button only becomes visible on hover, but it's clickable regardless
    await bubble.hover();
    await bubble.locator('.copy-btn').click();

    await expect(bubble.locator('.copy-btn')).toHaveClass(/copied/);
    const clipboardText = await page.evaluate(() => navigator.clipboard.readText());
    expect(clipboardText).toBe(text);
  });

  test('copying a fenced code block puts only the code on the clipboard', async ({ page }) => {
    await login(page, `pw-copy-code-${Date.now()}`);

    const snippet = 'def greet(name):\n    print(f"hello {name}")';
    const reply = 'Sure, here you go:\n\n```python\n' + snippet + '\n```\n\nLet me know if that helps.';

    // Inject an agent bubble directly instead of depending on a live LLM
    // response — appendBubble is a global top-level function declared in
    // the page's inline <script>.
    await page.evaluate((text) => {
      // @ts-ignore
      window.appendBubble('agent', text, 'TestBot');
    }, reply);

    const codeBlock = page.locator('pre.code-block').last();
    await expect(codeBlock).toBeVisible();
    await expect(codeBlock.locator('code')).toContainText('def greet');

    await codeBlock.locator('.copy-code-btn').click();
    await expect(codeBlock.locator('.copy-code-btn')).toHaveClass(/copied/);

    const clipboardText = await page.evaluate(() => navigator.clipboard.readText());
    expect(clipboardText).toBe(snippet);
  });

  test('falls back to execCommand copy in a non-secure context', async ({ page }) => {
    // Simulate the reported bug: navigator.clipboard is unavailable
    // (undefined) outside a secure context (plain HTTP on a non-localhost
    // host). We can't easily make Playwright itself load over a non-secure
    // origin against this same server, so instead we directly stub out
    // navigator.clipboard to reproduce that environment and confirm the
    // legacyCopy() fallback still succeeds and reports success in the UI.
    await login(page, `pw-copy-fallback-${Date.now()}`);

    await page.evaluate(() => {
      // @ts-ignore
      delete window.navigator.clipboard;
      Object.defineProperty(window.navigator, 'clipboard', { value: undefined, configurable: true });
    });

    const text = 'fallback path test';
    await page.fill('#input', text);
    await page.keyboard.press('Control+Enter');

    const bubble = page.locator('.bubble.user').last();
    await bubble.hover();
    await bubble.locator('.copy-btn').click();

    // Must NOT silently fail: expect the visible "copied" state, not "failed".
    await expect(bubble.locator('.copy-btn')).toHaveClass(/copied/);
    await expect(bubble.locator('.copy-btn')).not.toHaveClass(/copy-failed/);
  });

  test('plain Enter inserts a newline instead of sending', async ({ page }) => {
    await login(page, `pw-enter-newline-${Date.now()}`);

    await page.fill('#input', 'line one');
    await page.keyboard.press('Enter');
    await page.keyboard.type('line two');

    await expect(page.locator('#input')).toHaveValue('line one\nline two');
    // nothing should have been sent yet
    await expect(page.locator('.bubble.user')).toHaveCount(0);
  });
});
