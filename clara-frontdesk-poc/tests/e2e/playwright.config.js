// @ts-check
const { defineConfig } = require('@playwright/test');

module.exports = defineConfig({
  testDir: '.',
  timeout: 30_000,
  fullyParallel: false,
  reporter: [['list']],
  use: {
    baseURL: process.env.FRONTDESK_URL || 'http://localhost:8088',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
});
