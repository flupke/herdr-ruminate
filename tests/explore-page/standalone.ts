import { type ChildProcess, spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { type Browser, chromium, type Page } from 'playwright';
import { startChromium, stopChromium } from './nix-chromium.ts';
import { SERVER } from './server.ts';

export const THEMES = ['light', 'dark'] as const;
export type Theme = (typeof THEMES)[number];

export function windowHeight(width: number): number {
  return width < 600 ? 844 : 900;
}

export class StandaloneServer {
  private readonly child: ChildProcess;
  readonly baseUrl: string;

  private constructor(child: ChildProcess, baseUrl: string) {
    this.child = child;
    this.baseUrl = baseUrl;
  }

  static async start(args: string[]): Promise<StandaloneServer> {
    const child = spawn(SERVER, ['--port', '0', ...args], {
      stdio: ['ignore', 'pipe', 'inherit'],
    });
    const lines = createInterface({ input: child.stdout! });
    const baseUrl = await new Promise<string>((resolve, reject) => {
      child.on('error', reject);
      child.on('exit', (code) => reject(new Error(`explore-page-server exited with ${code}`)));
      lines.on('line', (line) => {
        const match = /^Explore page: (http:\/\/[^/]+)\//.exec(line);
        if (match) resolve(match[1]!);
        else if (line.startsWith('csp violation') || line.startsWith('template error')) console.error(line);
      });
    });
    return new StandaloneServer(child, baseUrl);
  }

  stop(): void {
    this.child.kill();
  }
}

export async function launch(): Promise<{ browser: Browser; close: () => Promise<void> }> {
  const executable = process.env.E2E_CHROMIUM;
  if (!executable) {
    const browser = await chromium.launch();
    return { browser, close: () => browser.close() };
  }
  const lease = await startChromium(executable);
  const browser = await chromium.connectOverCDP(lease.cdpEndpoint);
  return {
    browser,
    close: async () => {
      await browser.close();
      await stopChromium(lease);
    },
  };
}

export async function settle(page: Page): Promise<void> {
  await page.waitForFunction(drawn);
}

function drawn(): boolean {
  const sources = document.querySelectorAll('.markdown pre > code.language-mermaid').length;
  const failed = document.querySelectorAll('figure.diagram.failed').length;
  return (
    document.querySelector('main[data-seq]') !== null &&
    !document.querySelector('main[aria-busy="true"]') &&
    document.fonts.status === 'loaded' &&
    sources === failed &&
    document.body.dataset.diagrams !== 'drawing'
  );
}
