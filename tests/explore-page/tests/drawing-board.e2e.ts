import { expect } from 'e2e';
import { test } from './session.ts';

const SENT_BACK = { question: 'keep-draft', version: 1, choice: null, comment: '', drawing_board: true };
const REASON = 'One queue per conversation: the global queue reorders replies.';

test('the reviewer sends the agent back to the drawing board from the design screen, then takes it back', async ({
  explore,
  screen,
}) => {
  await explore.open();
  await explore.askQuestion();
  const design = screen.getByRole('region', 'Design of the change');

  await design.getByRole('button', 'Back to the drawing board…').tap();
  await expect(design.getByRole('button', 'Back to the drawing board…')).toHaveCount(0);
  expect(await explore.answers()).toEqual([]);
  await design.getByRole('button', 'Confirm: back to the drawing board').tap();
  await expect(screen.getByRole('status')).toContainText('The agent is working on your answer to question 1');
  await expect(screen.getByRole('region', 'Your answer to question 1')).toContainText('Back to the drawing board');
  expect(await explore.answers()).toEqual([SENT_BACK]);

  await explore.conclude();
  await expect(screen.getByRole('textbox', 'To be implemented')).toBeVisible();
  await expect(screen.getByRole('region', 'Your decisions')).toContainText('Back to the drawing board');

  await screen.getByRole('button', 'Cancel this answer…').tap();
  await screen.getByRole('button', 'Confirm: cancel my answer').tap();
  await expect(screen.getByRole('button', 'Send answer')).toBeVisible();
});

test('the comment typed in the answer goes to the agent from the design screen', async ({ explore, screen }) => {
  await explore.open();
  await explore.askQuestion();
  await screen.getByRole('link', 'Go to question 1').tap();
  await screen.getByRole('region', 'Question 1').getByRole('textbox', 'Comment · optional').fill(REASON);
  await explore.openDesign();
  const design = screen.getByRole('region', 'Design of the change');

  await design.getByRole('button', 'Back to the drawing board…').tap();
  await design.getByRole('button', 'Confirm: back to the drawing board').tap();

  await expect(screen.getByRole('link', 'Q1 · working')).toBeVisible();
  expect(await explore.answers()).toEqual([{ ...SENT_BACK, comment: REASON }]);
  await screen.getByRole('link', 'Q1 · working').tap();
  await expect(screen.getByRole('region', 'Your answer to question 1')).toContainText(REASON);
});

test('the reviewer sends the agent back to the drawing board from the menu, with the comment typed', async ({
  explore,
  screen,
}) => {
  await explore.open();
  await explore.askQuestion();
  await screen.getByRole('link', 'Go to question 1').tap();
  const question = screen.getByRole('region', 'Question 1');
  await question.getByRole('button', 'Back to the drawing board…').tap();
  await question.getByRole('button', 'Confirm: back to the drawing board').press('Escape');
  await expect(question.getByRole('button', 'Back to the drawing board…')).toBeVisible();
  await expect(question.getByRole('button', 'Confirm: back to the drawing board')).toHaveCount(0);
  await question.getByRole('textbox', 'Comment · optional').fill(REASON);

  await screen.getByRole('button', 'Round menu').tap();
  await screen.getByRole('button', 'Back to the drawing board… ends the questions').tap();
  await screen.getByRole('button', 'Confirm: back to the drawing board').tap();
  await expect(screen.getByRole('status')).toContainText('The agent is working on your answer to question 1');
  expect(await explore.answers()).toEqual([{ ...SENT_BACK, comment: REASON }]);
  await expect(screen.getByRole('button', 'Confirm: back to the drawing board')).toHaveCount(0);
});

test('under the answer, the comment typed goes with it, and Cancel answer gives it back to the box', async ({
  explore,
  screen,
}) => {
  await explore.open();
  await explore.askQuestion();
  await screen.getByRole('link', 'Go to question 1').tap();
  const question = screen.getByRole('region', 'Question 1');
  await question.getByRole('textbox', 'Comment · optional').fill(REASON);

  await question.getByRole('button', 'Back to the drawing board…').tap();
  await question.getByRole('button', 'Confirm: back to the drawing board').tap();
  await expect(screen.getByRole('region', 'Your answer to question 1')).toContainText(REASON);
  expect(await explore.answers()).toEqual([{ ...SENT_BACK, comment: REASON }]);

  await screen.getByRole('button', 'Cancel this answer…').tap();
  await screen.getByRole('button', 'Confirm: cancel my answer').tap();
  await expect(screen.getByRole('region', 'Question 1').getByRole('textbox', 'Comment · optional')).toHaveValue(REASON);
});
