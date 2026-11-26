import { expect } from 'e2e';
import { test } from './session.ts';

test('the page waits while the reviewer restarts, then shows the round as it is now', async ({ explore, screen }) => {
  await explore.open('reconnect-delay=0');
  await explore.askQuestion();
  await screen.getByRole('link', 'Go to question 1').tap();
  await expect(screen.getByRole('region', 'Question 1')).toBeVisible();

  await explore.restartReviewer();
  await expect(screen.getByText('Reconnecting to the review tool', { exact: false })).toBeVisible();
  await expect(screen.getByRole('button', 'Send answer')).toBeDisabled();

  await explore.answerInPane();
  await explore.reviewerBack();
  await expect(screen.getByRole('status')).toContainText('The agent is working on your answer');
  await expect(screen.getByRole('region', 'Your answer to question 1')).toContainText('Keep the draft');
  await expect(screen.getByText('Reconnecting to the review tool', { exact: false })).toBeHidden();
});
