import { expect } from 'e2e';
import { test } from './session.ts';

test('the reviewer turns on notifications and hears when the next question is ready and when the agent replies', async ({ explore, screen, browser }) => {
  await explore.open();
  await explore.askQuestion();
  await browser.evaluate(() => {
    const page = window as unknown as { shown: string[]; Notification: unknown };
    page.shown = [];
    page.Notification = class {
      static permission = 'default';
      static async requestPermission() {
        this.permission = 'granted';
        return 'granted';
      }
      onclick: unknown = null;
      constructor(title: string) {
        page.shown.push(title);
      }
      close() {}
    };
    document.hasFocus = () => false;
  });
  const shown = () => browser.evaluate(() => (window as unknown as { shown: string[] }).shown);

  await screen.getByRole('button', 'Round menu').tap();
  const notify = screen.getByRole('button', /^Notify me when the agent finishes/);
  await expect(notify).toHaveAttribute('aria-pressed', 'false');
  await notify.tap();
  await expect(notify).toHaveAttribute('aria-pressed', 'true');

  await explore.answerInPane();
  await expect(browser).toHaveTitle(/^Agent working/);
  expect(await shown()).toEqual([]);
  await explore.askQuestion();
  await expect(browser).toHaveTitle(/^Q2 · your turn/);
  expect(await shown()).toEqual(['Question 2 is ready']);

  await screen.getByRole('button', 'Talk to the agent').tap();
  const chat = screen.getByRole('complementary', 'Conversation with the agent');
  await chat.getByRole('textbox', 'Message to the agent').fill('Why?');
  await chat.getByRole('button', 'Send').tap();
  await expect(chat.getByRole('status')).toContainText('The agent is answering');
  await explore.agentReplies();
  await expect(chat.getByRole('article', 'Reply from the agent')).toBeVisible();
  expect(await shown()).toEqual(['Question 2 is ready', 'The agent replied in the chat']);
});
