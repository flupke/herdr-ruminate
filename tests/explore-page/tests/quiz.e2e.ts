import { expect } from 'e2e';
import { test } from './session.ts';

const FIRST_CORRECT = 'The answer typed before closing';
// e2e cannot build a locator from a name with an apostrophe.
const SECOND_WRONG = /^In the pane.s editor$/;

test('the reviewer picks one right answer and one wrong one, then reads the conclusion with the score', async ({
  explore,
  screen,
}) => {
  await explore.open();
  await explore.concludeWithQuiz();
  const first = screen.getByRole('region', 'Quiz · Question 1 of 2');
  await expect(first).toBeVisible();
  await expect(screen.getByRole('region', 'Conclusion')).toHaveCount(0);

  const answers = screen.getByRole('group', 'Answers');
  await answers.getByRole('radio', FIRST_CORRECT).check();
  await screen.getByRole('button', 'Check').tap();
  await expect(screen.getByRole('status')).toContainText('Correct.');
  await screen.getByRole('button', 'Next question').tap();

  await expect(screen.getByRole('region', 'Quiz · Question 2 of 2')).toBeVisible();
  await answers.getByRole('radio', SECOND_WRONG).check();
  await screen.getByRole('button', 'Check').tap();
  await expect(screen.getByRole('status')).toContainText('Not quite.');
  expect(await explore.quiz()).toEqual({
    picks: [
      { item: 0, answer: 1, correct: true },
      { item: 1, answer: 1, correct: false },
    ],
  });

  await screen.getByRole('button', 'Show the conclusion').tap();
  await expect(screen.getByRole('region', 'Conclusion')).toBeVisible();
  const panel = screen.getByRole('region', 'To be implemented');
  await expect(panel).toContainText('Quiz 1 of 2');

  await explore.open();
  await expect(panel).toContainText('Quiz 1 of 2');
});

test('the reviewer goes back to a checked item during the quiz, then reads the answered quiz from the rail', async ({
  explore,
  screen,
  browser,
}) => {
  await explore.open();
  await explore.concludeWithQuiz();
  await screen.getByRole('group', 'Answers').getByRole('radio', FIRST_CORRECT).check();
  await screen.getByRole('button', 'Check').tap();
  await screen.getByRole('button', 'Next question').tap();
  const second = screen.getByRole('region', 'Quiz · Question 2 of 2');
  await expect(second).toBeVisible();
  await expect(screen.getByRole('button', 'Next question')).toBeDisabled();

  await screen.getByRole('button', 'Previous').tap();
  const first = screen.getByRole('region', 'Quiz · Question 1 of 2');
  await expect(first.getByRole('status')).toContainText('Correct.');
  await expect(first.getByRole('radio')).toHaveCount(0);
  await expect(first.getByRole('button', 'Check')).toHaveCount(0);
  await expect(screen.getByRole('navigation', 'Round')).toContainText('Quiz 1/2');

  await screen.getByRole('button', 'Question 2, not answered yet').tap();
  await screen.getByRole('group', 'Answers').getByRole('radio', SECOND_WRONG).check();
  await screen.getByRole('button', 'Check').tap();
  await screen.getByRole('button', 'Show the conclusion').tap();
  await expect(screen.getByRole('region', 'Conclusion')).toBeVisible();

  await screen.getByRole('navigation', 'Round').getByRole('link', 'Quiz 1/2').tap();
  await expect(first).toBeVisible();
  await expect(first).toContainText('Correct.');
  await expect(first.getByRole('radio')).toHaveCount(0);
  await expect(screen.getByRole('button', 'Previous')).toBeDisabled();
  await screen.getByRole('button', 'Next question').tap();
  await expect(second).toContainText('Not quite.');
  await expect(browser).toHaveURL(/#quiz-2$/);

  await browser.reload();
  await expect(second).toContainText('Not quite.');
  await second.getByRole('link', 'Back to the conclusion').tap();
  await expect(screen.getByRole('region', 'Conclusion')).toBeVisible();
  expect((await explore.quiz()).picks).toHaveLength(2);
});

test('the reviewer skips the quiz, reads the conclusion, then the answers', async ({ explore, screen }) => {
  await explore.open();
  await explore.concludeWithQuiz();
  await expect(screen.getByRole('region', 'Quiz · Question 1 of 2')).toBeVisible();

  await screen.getByRole('button', 'Skip the quiz').tap();
  await expect(screen.getByRole('region', 'Conclusion')).toBeVisible();
  await expect(screen.getByRole('region', 'To be implemented')).toContainText('You skipped 2 questions');
  expect(await explore.quiz()).toEqual({ skipped: true });
  await expect(screen.getByRole('navigation', 'Round').getByRole('link', /Quiz/)).toHaveCount(0);

  await screen.getByRole('link', 'See the answers').tap();
  const item = screen.getByRole('region', 'Quiz · Question 1 of 2');
  await expect(item).toContainText('Skipped.');
  await expect(item.getByRole('listitem').filter({ hasText: 'Correct' })).toContainText(FIRST_CORRECT);
});
