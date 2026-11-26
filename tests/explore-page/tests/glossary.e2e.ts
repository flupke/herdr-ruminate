import { expect } from 'e2e';
import { test } from './session.ts';

test('a marked term shows its definition, and the design lists every term', async ({ explore, screen }) => {
  await explore.open();
  await explore.askQuestion();
  await screen.getByRole('link', 'Go to question 1').tap();
  const question = screen.getByRole('region', 'Question 1');

  await question.getByRole('button', /^draft$/).first().tap();
  await expect(screen.getByRole('tooltip')).toHaveText(
    'The text the reviewer typed in the answer box and has not sent yet.',
  );
  await expect(question.getByRole('radio', 'Keep the draft')).not.toBeChecked();

  await explore.openDesign();
  const glossary = screen.getByRole('region', 'Glossary');
  await expect(glossary).toContainText('reopen()');
  await expect(glossary).toContainText('The text the reviewer typed in the answer box and has not sent yet.');
});
