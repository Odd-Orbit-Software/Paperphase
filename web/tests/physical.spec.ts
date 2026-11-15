import {expect,test} from '@playwright/test';

test('physical startup distinguishes unconfirmed pixels from a completed screen update',async({page,request})=>{
  const state=await(await request.get('/api/state')).json();
  state.mode='physical';state.display_frame_available=false;state.phase='pending';state.pending=true;
  state.refresh.baseline_known=false;state.refresh.profile.partial=false;state.refresh.profile.id='waveshare-7.5-v2-full';state.refresh.profile.min_interval_ms=180000;state.refresh.profile.max_partials=0;state.refresh.wait_ms=180000;
  await page.route('**/api/state',route=>route.fulfill({json:state}));
  await page.goto('/');await expect(page.locator('#frame')).toBeHidden();await expect(page.locator('#canvas-caption')).toContainText('Waiting for the first screen update');await expect(page.locator('#device-status')).toHaveText('Display update pending');await expect(page.locator('#canvas-caption')).toContainText('Next update can start in about 180 seconds');
  state.display_frame_available=true;state.refresh.baseline_known=true;state.phase='idle';state.pending=false;state.frame_id+='-confirmed';
  await expect(page.locator('#frame')).toBeVisible();await expect(page.locator('#device-status')).toHaveText('Display ready');await expect(page.locator('#canvas-caption')).not.toContainText('Next update');await expect(page.locator('#canvas-caption')).toContainText('Last completed screen update');
  await page.getByRole('button',{name:'Device',exact:true}).click();await expect(page.locator('#device-settings-body')).toContainText('at least three minutes apart');await page.locator('#device-settings-body details summary').click();await expect(page.locator('#device-settings-body')).toContainText('Partial refresh is not enabled');
});

test('an unconfirmed restored image remains labelled during quiet hours',async({page,request})=>{
  const state=await(await request.get('/api/state')).json();state.mode='physical';state.display_frame_available=true;state.phase='paused';state.pending=true;state.refresh.baseline_known=false;
  await page.route('**/api/state',route=>route.fulfill({json:state}));await page.goto('/');await expect(page.locator('#frame')).toBeVisible();await expect(page.locator('#canvas-caption')).toContainText('Screen state is unconfirmed');await expect(page.locator('#device-status')).toContainText('Quiet hours');await expect(page.locator('#canvas-caption')).toContainText('latest content waits until updates resume');
  await page.setViewportSize({width:390,height:844});state.phase='error';state.last_error='Display update failed; retrying with a full refresh.';state.refresh.wait_ms=120000;
  await expect(page.locator('#canvas-caption')).toContainText('Display update failed; retrying with a full refresh.');await expect(page.locator('#canvas-caption')).toContainText('Retry can start in about 120 seconds');
});


test('physical partial profile explains spacing and maintenance without claiming full-only output',async({page,request})=>{
  const state=await(await request.get('/api/state')).json();state.mode='physical';state.refresh.profile.partial=true;state.refresh.profile.scheduled_full=true;state.refresh.profile.id='waveshare-7.5-v2-partial';state.refresh.profile.min_interval_ms=180000;state.refresh.profile.max_partials=3;state.refresh.profile.max_full_age_secs=1800;
  await page.route('**/api/state',route=>route.fulfill({json:state}));await page.goto('/');await page.getByRole('button',{name:'Device',exact:true}).click();
  await expect(page.locator('#device-settings-body')).toContainText('Small changes can update part of the screen');await expect(page.locator('#device-settings-body')).toContainText('at most three partial updates');await expect(page.locator('#device-settings-body')).toContainText('even when content stays unchanged');
  await page.locator('#device-settings-body details summary').click();await expect(page.locator('#device-settings-body')).toContainText('Large changes and recovery use a full refresh');await expect(page.locator('#device-settings-body')).not.toContainText('Partial refresh is not enabled');await expect(page.getByLabel('Full refresh after partial updates',{exact:true})).toHaveValue('3');await expect(page.getByLabel('Full refresh after partial updates',{exact:true})).toHaveAttribute('max','3');await expect(page.getByLabel('Minimum refresh interval (ms)',{exact:true})).toHaveValue('180000');await expect(page.getByLabel('Full refresh interval (seconds)',{exact:true})).toHaveValue('1800');await expect(page.getByLabel('Full refresh interval (seconds)',{exact:true})).toHaveAttribute('max','1800');
});
