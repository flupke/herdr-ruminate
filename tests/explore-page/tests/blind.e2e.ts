import { expect } from 'e2e';
import { test } from './session.ts';

const RECOMMENDATION = 'Renaming keeps every file, and the new names stay readable.';
const COMMENT = 'Renaming hides which file the user meant to keep.';

const ONE_WAY = (() => {
  const consequence = (summary: string) => ({ summary, evidence: [], unknowns: ['No caller was checked.'] });
  return {
    id: 'same-name-files',
    version: 1,
    topic: 'export',
    text: 'What should the export do with two files of the same name?',
    rationale: null,
    visual: null,
    alternatives: [
      { id: 'rename', text: 'Rename the clashing files', outcome: 'accepted', recommendation: RECOMMENDATION },
      { id: 'overwrite', text: 'Overwrite the older file', outcome: 'needs_follow_up', recommendation: null },
      { id: 'stop', text: 'Stop the export with an error', outcome: 'needs_follow_up', recommendation: null },
    ],
    evidence: [],
    assessments: {
      door: 'one_way',
      reversibility: consequence('Files an export overwrote are gone.'),
      blast_radius: consequence('Every export with clashing names.'),
    },
  };
})();

test('the first Send shows the recommendation and keeps the comment, then the reviewer changes the pick and confirms', async ({
  explore,
  screen,
}) => {
  await explore.open();
  await explore.askQuestion(ONE_WAY);
  await screen.getByRole('link', 'Go to question 1').tap();
  const question = screen.getByRole('region', 'Question 1');
  await expect(question).toContainText('Blind pick');
  await expect(screen.getByText(RECOMMENDATION, { exact: false })).toHaveCount(0);

  const comment = question.getByRole('textbox', 'Comment · optional');
  await comment.fill(COMMENT);
  await question.getByRole('radio', 'Overwrite the older file').check();
  await question.getByRole('button', 'Send answer').tap();
  await expect(screen.getByText(RECOMMENDATION, { exact: false })).toBeVisible();
  await expect(screen.getByText('The agent recommends another choice.')).toBeVisible();
  await expect(comment).toHaveValue(COMMENT);
  expect(await explore.answers()).toEqual([]);

  await question.getByRole('radio', 'Rename the clashing files').check();
  await question.getByRole('button', 'Confirm answer').tap();
  await expect(screen.getByRole('status')).toContainText('The agent is working');
  expect(await explore.answers()).toEqual([
    { question: 'same-name-files', version: 1, choice: 'rename', comment: COMMENT, first_pick: 'overwrite' },
  ]);
});
