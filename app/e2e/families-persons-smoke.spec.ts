import { expect, test } from '@playwright/test';

test('persons page shows Kennedy seed and family detail shows named children', async ({ page, request }) => {
  await page.addInitScript(() => {
    window.localStorage.clear();
  });

  await page.goto('/persons');

  await expect(page.getByText('Starting RustyGene…')).toHaveCount(0, { timeout: 45_000 });

  await expect(page.getByTestId('persons-page')).toBeVisible();
  await page.getByTestId('persons-page-size-select').selectOption('250');
  await expect(page.getByTestId('person-display-name').first()).toBeVisible();

  const parent1Resp = await request.post('http://127.0.0.1:3000/api/v1/persons', {
    data: {
      given_names: ['E2E Parent One'],
      surnames: [{ value: 'Harness', origin_type: 'patrilineal', connector: null }],
      name_type: 'birth',
      gender: 'male'
    }
  });
  expect(parent1Resp.ok()).toBeTruthy();
  const parent1 = (await parent1Resp.json()) as { id: string };

  const parent2Resp = await request.post('http://127.0.0.1:3000/api/v1/persons', {
    data: {
      given_names: ['E2E Parent Two'],
      surnames: [{ value: 'Harness', origin_type: 'patrilineal', connector: null }],
      name_type: 'birth',
      gender: 'female'
    }
  });
  expect(parent2Resp.ok()).toBeTruthy();
  const parent2 = (await parent2Resp.json()) as { id: string };

  const childResp = await request.post('http://127.0.0.1:3000/api/v1/persons', {
    data: {
      given_names: ['E2E Child'],
      surnames: [{ value: 'Harness', origin_type: 'patrilineal', connector: null }],
      name_type: 'birth',
      gender: 'unknown'
    }
  });
  expect(childResp.ok()).toBeTruthy();
  const child = (await childResp.json()) as { id: string };

  const familyResp = await request.post('http://127.0.0.1:3000/api/v1/families', {
    data: {
      partner1_id: parent1.id,
      partner2_id: parent2.id,
      partner_link: 'married',
      child_ids: [child.id]
    }
  });
  expect(familyResp.ok()).toBeTruthy();
  await familyResp.json();

  await page.getByRole('link', { name: 'Families' }).click();
  await expect(page.getByTestId('families-page')).toBeVisible();
  await page.getByTestId('families-search-input').fill('E2E Parent One');

  const targetFamilyRow = page.getByTestId('family-row').filter({ hasText: 'E2E Parent One' });
  await expect(targetFamilyRow.first()).toBeVisible({ timeout: 20_000 });
  await targetFamilyRow.first().click();

  await expect(page.getByRole('heading', { name: /Family of/i })).toBeVisible();

  const childrenSection = page.getByTestId('family-children-section');
  await expect(childrenSection).toBeVisible();

  const childLabels = await page.getByTestId('family-child-name').allTextContents();
  expect(childLabels.length).toBeGreaterThan(0);
  expect(childLabels.some((label) => label.includes('E2E Child'))).toBeTruthy();

  for (const label of childLabels.map((v) => v.trim())) {
    expect(label).not.toMatch(/^Person [0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i);
  }
});
