import { expect } from 'e2e';
import { test } from './session.ts';

test('the reviewer reads a past round of another change, then goes back to the current round', async ({
  explore,
  screen,
  browser,
}) => {
  await browser.setViewport({ width: 1280, height: 800 });
  await explore.open();
  await explore.askQuestion();
  await screen.getByRole('link', 'Go to question 1').tap();
  await expect(screen.getByRole('region', 'Question 1')).toBeVisible();

  await screen.getByRole('button', 'Round menu').tap();
  await screen.getByRole('button', /^Past rounds/).tap();
  const list = screen.getByRole('navigation', 'Past rounds');
  await expect(list).toContainText('Name the queue');
  await list.getByRole('link').first().tap();
  await expect(list).toBeHidden();

  const banner = screen.getByRole('region', 'Past round');
  await expect(banner).toContainText('across reloads');
  await expect(banner).toContainText('The current round waits for you.');
  await expect(screen.getByRole('region', 'Conclusion')).toBeVisible();
  await expect(screen.getByRole('button', 'Implement')).toHaveCount(0);
  await expect(screen.getByRole('textbox')).toHaveCount(0);
  await screen.getByRole('button', 'Round menu').tap();
  await expect(screen.getByRole('button', /^Reset this round/)).toHaveCount(0);
  await screen.getByRole('button', 'Round menu').tap();

  await screen.getByRole('navigation', 'Round').getByRole('link', 'Q1').tap();
  await expect(screen.getByRole('region', 'Question 1 · answered')).toBeVisible();
  await expect(browser).toHaveURL(/#question-1@session-\d+-past-1-1$/);
  await browser.reload();
  await expect(screen.getByRole('region', 'Question 1 · answered')).toBeVisible();
  await expect(banner).toBeVisible();

  await browser.evaluate(() => history.back());
  await expect(screen.getByRole('region', 'Conclusion')).toBeVisible();
  await banner.getByRole('link', 'Back to the current round').tap();
  await expect(banner).toBeHidden();
  await expect(screen.getByRole('region', 'Question 1')).toBeVisible();
  await expect(screen.getByRole('button', 'Send answer')).toBeVisible();
  expect(await explore.answers()).toEqual([]);
});

test('a round the reviewer resets is listed on the start cover and opens where it stopped', async ({
  explore,
  screen,
}) => {
  await explore.open();
  await explore.askQuestion();
  await screen.getByRole('link', 'Go to question 1').tap();
  await explore.reset();
  await expect(screen.getByRole('button', 'Start')).toBeVisible();

  await screen.getByRole('button', 'Round menu').tap();
  await screen.getByRole('button', /^Past rounds/).tap();
  const list = screen.getByRole('navigation', 'Past rounds');
  await expect(list.getByRole('region').first()).toContainText('when a round reopens');
  await list.getByRole('link').first().tap();

  const question = screen.getByRole('region', 'Question 1');
  await expect(question).toBeVisible();
  await expect(screen.getByRole('region', 'Past round')).not.toContainText('waits for you');
  await expect(screen.getByRole('group', 'Choices')).toBeDisabled();
  await expect(screen.getByRole('button', 'Send answer')).toHaveCount(0);
});

test('the address the pane opens shows the past rounds over the start cover, with no round running', async ({
  explore,
  screen,
  browser,
}) => {
  await explore.reset();
  await explore.openPastRounds();

  const list = screen.getByRole('navigation', 'Past rounds');
  await expect(list).toContainText('Name the queue');
  await expect(screen.getByRole('button', 'Start')).toBeVisible();
  await expect(browser).toHaveURL(/#round$/);

  await list.getByRole('link').first().tap();
  await expect(screen.getByRole('region', 'Past round')).toBeVisible();
  expect(await explore.starts()).toEqual([]);
});

test('beside the page, the list stays open from one past round to the next, where the chat stands', async ({
  explore,
  screen,
  browser,
}) => {
  await browser.setViewport({ width: 1920, height: 1000 });
  await explore.open();
  await explore.askQuestion();
  await screen.getByRole('link', 'Go to question 1').tap();
  await screen.getByRole('button', 'Talk to the agent').tap();
  const chat = screen.getByRole('complementary', 'Conversation with the agent');
  await expect(chat).toBeVisible();

  await screen.getByRole('button', 'Round menu').tap();
  await screen.getByRole('button', /^Past rounds/).tap();
  const list = screen.getByRole('navigation', 'Past rounds');
  await expect(list).toBeVisible();
  await expect(chat).toBeHidden();

  await list.getByRole('link').first().tap();
  const banner = screen.getByRole('region', 'Past round');
  await expect(banner).toContainText('across reloads');
  await expect(list.getByRole('link').first()).toHaveAttribute('aria-current', 'page');
  await list.getByRole('link').last().tap();
  await expect(banner).toContainText('flush policy');
  await expect(list).toBeVisible();
  await expect(list.getByRole('link').last()).toHaveAttribute('aria-current', 'page');
  await expect(list.getByRole('link').first()).not.toHaveAttribute('aria-current', 'page');

  await screen.getByRole('button', 'Talk to the agent').tap();
  await expect(chat).toBeVisible();
  await expect(list).toBeHidden();
});
