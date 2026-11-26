import type { E2EConfig } from 'e2e';
import { web } from '@e2e-dev/web';
import { nixChromium } from './nix-chromium.ts';
import { SERVER } from './server.ts';

const page = (target: string) => ({
  url: 'http://127.0.0.1:0',
  command: {
    executable: SERVER,
    args: ['--port', '{port}', '--token', 'e2e'],
    log: `.e2e/logs/${target}.log`,
    startupTimeout: 10_000,
  },
  readyUrl: 'http://127.0.0.1:{port}/health',
});

const browser = process.env.E2E_CHROMIUM ? nixChromium(process.env.E2E_CHROMIUM) : 'chromium';

export default {
  targets: [
    { name: 'desktop', engine: web({ browser }), app: page('desktop') },
  ],
  retries: 0,
  trace: 'retain-on-failure',
  workers: 2,
  timeout: 30_000,
} satisfies E2EConfig;
