import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';

test.beforeEach(async ({ request }) => {
  const current = await (await request.get('/api/state')).json();
  const layout = JSON.parse(readFileSync('../examples/simulator/layout.json', 'utf8'));
  layout.components = layout.components.slice(0, 3);
  expect((await request.put('/api/layout', {data: {base_revision:current.revision,layout,refresh_settings:{quiet_hours:null}}})).ok()).toBeTruthy();
  await expect.poll(async () => (await (await request.get('/api/state')).json()).phase).toBe('idle');
});

test('content editing preserves flow and keyboard focus through selection, properties and polling', async ({page,request}) => {
  await page.goto('/');
  await page.getByRole('button',{name:'Edit layout',exact:true}).click();
  const heading = page.locator('#component-list [data-component="header"]');
  await expect(heading).toHaveAccessibleName('Select Paperphase');
  await heading.focus(); await page.keyboard.press('Enter');
  await expect(heading).toBeFocused();
  await expect(page.getByRole('tab',{name:'Content',exact:true})).toHaveAttribute('aria-selected','true');
  await page.getByLabel('Text',{exact:true}).first().fill('A calmer morning');
  await expect(page.getByLabel('Text',{exact:true}).first()).toBeFocused();
  await expect(page.getByLabel('Text',{exact:true}).first()).toHaveValue('A calmer morning');
  await page.getByRole('tab',{name:'Position',exact:true}).click();
  await page.getByLabel('X',{exact:true}).fill('24');
  await page.getByLabel('X',{exact:true}).press('Tab');
  await expect(page.getByLabel('Y',{exact:true})).toBeFocused();
  await page.getByRole('tab',{name:'Style',exact:true}).click();
  await page.getByLabel('Font size (px)',{exact:true}).first().fill('27');
  await page.getByLabel('Font size (px)',{exact:true}).first().press('Tab');
  await expect(page.getByLabel('Alignment',{exact:true}).first()).toBeFocused();
  const overlay = page.locator('.component-box[data-component="header"]');
  await overlay.focus(); await page.keyboard.press('ArrowRight');
  await expect(overlay).toBeFocused();
  await page.getByRole('button',{name:'Data sources',exact:true}).click();
  const source = page.getByRole('button',{name:'Configure Bench sensor',exact:true});
  await source.focus();
  await expect.poll(async () => { await page.waitForTimeout(1200); return source.evaluate(el => el === document.activeElement); }).toBe(true);
  await page.getByRole('button',{name:'Back to display',exact:true}).click();
  await page.getByRole('button',{name:'Save to display',exact:true}).click();
  await expect(page.getByRole('button',{name:'Edit layout',exact:true})).toBeVisible();
  const state = await (await request.get('/api/state')).json();
  expect(state.layout.components.find((c:{id:string})=>c.id==='header').content_layout ?? 'flow').toBe('flow');
});

test('optional metadata failure retains the display and retry restores editing options', async ({page}) => {
  await page.route('**/api/icon-packs', route=>route.fulfill({status:503,contentType:'application/json',body:'{"error":"Unavailable"}'}));
  await page.goto('/');
  await expect(page.locator('#frame')).toHaveJSProperty('naturalWidth',800);
  await expect(page.getByRole('button',{name:'Edit layout',exact:true})).toBeEnabled();
  await expect(page.locator('#metadata-warning')).toContainText('Your display is available');
  await page.unroute('**/api/icon-packs');
  await page.getByRole('button',{name:'Retry options',exact:true}).click();
  await expect(page.locator('#metadata-warning')).toBeHidden();
});

test('startup failure has a usable retry without a broken preview', async ({page}) => {
  await page.route('**/api/state', route=>route.fulfill({status:503,contentType:'application/json',body:'{"error":"Fixture service unavailable"}'}));
  await page.goto('/');
  await expect(page.getByRole('heading',{name:'Cannot connect to Paperphase'})).toBeVisible();
  await expect(page.locator('#display-workspace')).toBeHidden();
  await page.unroute('**/api/state');
  await page.getByRole('button',{name:'Retry connection',exact:true}).click();
  await expect(page.locator('#frame')).toHaveJSProperty('naturalWidth',800);
  await expect(page.locator('#loading-state')).toBeHidden();
});

test('saved connection deletion requires explicit confirmation and cancel keeps the form', async ({page,request}) => {
  let deletes = 0;
  await page.route('**/api/connections/temperature.browser', async route=>{
    if (route.request().method()==='DELETE') { deletes++; await route.fulfill({status:409,contentType:'application/json',body:'{"error":"Fixture keeps this connection"}'}); }
    else await route.continue();
  });
  await page.goto('/'); await page.getByRole('button',{name:'Data sources',exact:true}).click();
  await page.getByRole('button',{name:'Configure Bench sensor',exact:true}).click();
  await page.getByRole('button',{name:'Delete connection',exact:true}).click();
  await expect(page.getByRole('dialog',{name:'Delete Bench sensor?'})).toBeVisible();
  await page.getByRole('button',{name:'Cancel',exact:true}).click();
  expect(deletes).toBe(0);
  await expect(page.getByLabel('Location label',{exact:true})).toHaveValue('Workshop');
  await page.getByRole('button',{name:'Delete connection',exact:true}).click();
  await page.locator('#confirm-accept').click();
  await expect.poll(()=>deletes).toBe(1);
  await expect(page.locator('#connections-notice')).toContainText('Fixture keeps this connection');
  expect((await (await request.get('/api/connections')).json()).connections.some((c:{id:string})=>c.id==='temperature.browser')).toBe(true);
});

test('Google prerequisites precede calendar creation and disconnect has a confirmation boundary', async ({page}) => {
  await page.route('**/api/google',route=>route.fulfill({json:{configured:false,client_id:null,client_secret_present:false,redirect_uri:'https://auth.paperphase.oddorbit.software/oauth/google/callback',accounts:[]}}));
  await page.goto('/'); await page.getByRole('button',{name:'Data sources',exact:true}).click();
  await page.getByRole('button',{name:'Add data source',exact:true}).click();
  await page.getByRole('button',{name:/^Google Calendar/}).click();
  await expect(page.locator('#google-panel')).toBeVisible();
  await expect(page.locator('#google-panel')).toContainText('needs a Google OAuth client');
  await expect(page.locator('#connection-form')).toHaveCount(0);
  await expect(page.getByRole('button',{name:'Link Google account',exact:true})).toBeDisabled();
  await page.unroute('**/api/google');
  await page.route('**/api/google',route=>route.fulfill({json:{configured:true,client_id:'fixture.apps.googleusercontent.com',client_secret_present:true,redirect_uri:'https://auth.paperphase.oddorbit.software/oauth/google/callback',accounts:[{id:'fixture',label:'Personal',requires_reauth:false,calendars:[{id:'primary',label:'Primary',primary:true}]}]}}));
  await page.getByRole('button',{name:'Back to display',exact:true}).click();
  await page.getByRole('button',{name:'Data sources',exact:true}).click();
  await page.getByRole('button',{name:'Google accounts',exact:true}).click();
  await expect(page.getByLabel('Google OAuth client secret',{exact:true})).toBeHidden();
  await page.getByRole('button',{name:'Disconnect Personal',exact:true}).click();
  await expect(page.getByRole('dialog',{name:'Disconnect Personal?'})).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button',{name:'Choose a calendar from Personal',exact:true})).toBeVisible();
});

test('branding is served locally and surrounding controls reflow at narrow effective desktop widths', async ({page,request}) => {
  for (const path of ['/brand/odd-orbit.png','/brand/orbi.png']) {
    const response = await request.get(path); expect(response.ok()).toBeTruthy(); expect(response.headers()['content-type']).toBe('image/png');
  }
  await page.goto('/');
  await expect(page.locator('.page-footer')).toContainText('Developed by Odd Orbit Software');
  await page.getByRole('button',{name:'About',exact:true}).click();
  await expect(page.locator('.orbi-portrait')).toBeVisible();
  await expect(page.locator('.orbi-portrait')).toHaveJSProperty('complete',true);
  await page.getByRole('button',{name:'Back to display',exact:true}).click();
  await page.setViewportSize({width:720,height:900});
  await page.getByRole('button',{name:'Edit layout',exact:true}).click();
  await page.locator('#component-list [data-component="header"]').click();
  await expect(page.getByLabel('Text',{exact:true}).first()).toBeVisible();
  expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBe(720);
  await page.getByRole('button',{name:'Device',exact:true}).click();
  await expect(page.getByRole('checkbox',{name:'Enable quiet hours'})).toBeVisible();
  expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBe(720);
});
