// @ts-check
const { test, expect } = require('@playwright/test');

async function login(page, username) {
  await page.goto('/');
  await page.fill('#login-username', username);
  await page.click('#login-submit');
  await expect(page.locator('#input')).toBeEnabled({ timeout: 10_000 });
}

// Capture what the page sends over its WebSocket (the real socket stays connected to the live backend).
async function captureSends(page) {
  await page.evaluate(() => {
    // @ts-ignore
    window.__sent = [];
    const original = WebSocket.prototype.send;
    WebSocket.prototype.send = function (data) {
      // @ts-ignore
      window.__sent.push(JSON.parse(data));
      // Swallow escalation_resolve so the (fake) request never reaches the real backend.
      if (String(data).includes('escalation_resolve')) return;
      return original.call(this, data);
    };
  });
}

const ITEM = {
  type: 'research_update',
  request_id: 'esc-1',
  kind: 'escalation',
  query: 'publish my plan',
  reply: 'The Ego wants to run **publish_document** with {"name": "plan.md"}.\nApprove or deny?',
  citation_count: 0,
  ref: 'ritual-x/3',
};

test.describe('Ego escalations', () => {
  test('an escalation shows Approve/Deny buttons and is not auto-acked', async ({ page }) => {
    await login(page, `pw-esc-basic-${Date.now()}`);
    await captureSends(page);
    await page.evaluate((item) => { window.addPendingResearch(item); }, ITEM);

    await page.click('#research-bell');
    const row = page.locator('.research-item.escalation');
    await expect(row).toHaveCount(1);
    await expect(row).toContainText('publish_document');
    await expect(row.locator('button.approve')).toBeEnabled();
    await expect(row.locator('button.deny')).toBeEnabled();

    // An ack would retire an undecided request server-side; the page must not send one.
    const sent = await page.evaluate(() => window.__sent);
    expect(sent.filter((m) => m.type === 'research_ack')).toHaveLength(0);
  });

  test('clicking Approve sends one escalation_resolve and disables both buttons', async ({ page }) => {
    await login(page, `pw-esc-approve-${Date.now()}`);
    await captureSends(page);
    await page.evaluate((item) => { window.addPendingResearch(item); }, ITEM);
    await page.click('#research-bell');

    await page.click('.research-item.escalation button.approve');
    const row = page.locator('.research-item.escalation');
    await expect(row.locator('button.approve')).toBeDisabled();
    await expect(row.locator('button.deny')).toBeDisabled();
    const sent = await page.evaluate(() => window.__sent.filter((m) => m.type === 'escalation_resolve'));
    expect(sent).toEqual([{ type: 'escalation_resolve', request_id: 'esc-1', decision: 'approve' }]);
  });

  test('resolving removes the item and shows what was done', async ({ page }) => {
    await login(page, `pw-esc-done-${Date.now()}`);
    await page.evaluate((item) => { window.addPendingResearch(item); }, ITEM);
    await page.evaluate(() => {
      window.dropPendingResearch('esc-1');
      window.appendBubble('agent', "Approved. published 'plan.md'", 'Bot', 'research-update');
    });
    await expect(page.locator('.research-item.escalation')).toHaveCount(0);
    await expect(page.locator('.bubble.agent').last()).toContainText("published 'plan.md'");
  });

  test('an unanswered escalation survives a reload', async ({ page }) => {
    await login(page, `pw-esc-reload-${Date.now()}`);
    await page.evaluate((item) => { window.addPendingResearch(item); }, ITEM);
    await page.reload();
    await expect(page.locator('#input')).toBeEnabled({ timeout: 10_000 });
    await page.click('#research-bell');
    await expect(page.locator('.research-item.escalation')).toHaveCount(1);
  });
});
