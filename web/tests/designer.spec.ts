import {expect,test} from '@playwright/test';
import {readFileSync} from 'node:fs';

test.beforeEach(async({request})=>{
  let library=await(await request.get('/api/designs')).json();
  for(const design of library.designs){const response=await request.delete(`/api/designs/${design.id}`,{data:{base_revision:library.revision}});expect(response.ok()).toBeTruthy();library=await response.json();}
  const state=await(await request.get('/api/state')).json();
  const layout=JSON.parse(readFileSync('../examples/simulator/layout.json','utf8'));
  layout.components=layout.components.slice(0,3);
  expect((await request.put('/api/layout',{data:{base_revision:state.revision,layout,refresh_settings:{quiet_hours:null}}})).ok()).toBeTruthy();
  await expect.poll(async()=>(await(await request.get('/api/state')).json()).phase).toBe('idle');
});

test('mouse selections delete components and internal items, with undo and protected text input',async({page,request})=>{
  await page.goto('/');await page.getByRole('button',{name:'Edit layout',exact:true}).click();
  await page.locator('.component-box[data-component="weather"]').click({position:{x:20,y:30}});
  await page.keyboard.press('Delete');await expect(page.locator('#component-list [data-component]')).toHaveCount(2);
  await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(page.locator('#component-list [data-component]')).toHaveCount(3);
  await page.locator('#component-list [data-component="header"]').click();
  await page.getByRole('button',{name:'Design this component',exact:true}).click();
  await expect(page.getByRole('button',{name:'Apply to layout',exact:true})).toBeEnabled();
  await page.locator('#add-open').click();await page.getByRole('button',{name:'Text',exact:true}).click();
  const text=page.getByRole('textbox',{name:'Text',exact:true});await text.fill('Keep me');await text.press('Home');await text.press('Delete');
  await expect(text).toHaveValue('eep me');await expect(page.locator('#component-list [data-element]')).toHaveCount(3);
  await expect(page.locator('.element-box')).toHaveCount(3);await page.locator('.element-box').last().click({position:{x:3,y:3}});
  await page.keyboard.press('Delete');await expect(page.locator('#component-list [data-element]')).toHaveCount(2);
  await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(page.locator('#component-list [data-element]')).toHaveCount(3);
  expect((await(await request.get('/api/state')).json()).layout.components).toHaveLength(3);
});

test('blank designs save independently, survive reload, and add only to the draft',async({page,request})=>{
  await page.goto('/');const initial=await(await request.get('/api/state')).json();const frame=await(await request.get('/api/display.png')).body();
  await page.getByRole('button',{name:'Component designs',exact:true}).click();
  await page.getByRole('button',{name:'Create design for Text & time',exact:true}).click();
  await expect(page.locator('#design-empty')).toBeVisible();await expect(page.getByRole('button',{name:'Save design',exact:true})).toBeDisabled();
  await page.locator('#add-open').click();await page.getByRole('button',{name:'Text',exact:true}).click();
  await page.getByRole('textbox',{name:'Text',exact:true}).fill('Morning notes');await page.getByLabel('Design name',{exact:true}).fill('Browser reusable note');
  await expect(page.getByRole('button',{name:'Save design',exact:true})).toBeEnabled();await page.getByRole('button',{name:'Save design',exact:true}).click();
  await expect(page.locator('#notice')).toContainText('Design saved');
  const library=await(await request.get('/api/designs')).json();const design=library.designs.find((d:any)=>d.name==='Browser reusable note');
  expect(design.component.blocks[0].font_size).toBe(14);expect(design.component.rect.height).toBeLessThan(60);expect(design.component.rect.width).toBeLessThan(180);
  expect((await(await request.get('/api/state')).json()).revision).toBe(initial.revision);expect(await(await request.get('/api/display.png')).body()).toEqual(frame);
  await page.getByRole('button',{name:'Close designer',exact:true}).click();await expect(page.getByRole('button',{name:'Edit design Browser reusable note',exact:true})).toBeVisible();
  await expect(page.locator('.design-source-choice').filter({hasText:'Text & time'})).toContainText('1 saved design');
  await page.reload();await page.getByRole('button',{name:'Component designs',exact:true}).click();await page.getByRole('button',{name:'Edit design Browser reusable note',exact:true}).click();
  await expect(page.getByRole('textbox',{name:'Text',exact:true})).toHaveValue('Morning notes');
  await page.getByLabel('Design name',{exact:true}).fill('Updated reusable note');await expect(page.getByRole('button',{name:'Save design',exact:true})).toBeEnabled();await page.getByRole('button',{name:'Save design',exact:true}).click();await expect(page.locator('#notice')).toContainText('Design saved');
  await page.getByRole('button',{name:'Close designer',exact:true}).click();await expect(page.getByRole('button',{name:'Edit design Updated reusable note',exact:true})).toBeVisible();await expect(page.getByRole('button',{name:'Edit design Browser reusable note',exact:true})).toHaveCount(0);
  await page.getByRole('button',{name:'Edit design Updated reusable note',exact:true}).click();
  await expect(page.getByRole('button',{name:'Add to display',exact:true})).toBeEnabled();await page.getByRole('button',{name:'Add to display',exact:true}).click();
  await expect(page.locator('#component-list [data-component]')).toHaveCount(4);
  expect((await(await request.get('/api/state')).json()).revision).toBe(initial.revision);
  await page.getByRole('button',{name:'Save to display',exact:true}).click();await expect.poll(async()=>(await(await request.get('/api/state')).json()).layout.components.length).toBe(4);
});

test('a full display still permits creating and saving a design; failed placement retains it',async({page,request})=>{
  const state=await(await request.get('/api/state')).json();const layout=state.layout;layout.components=[layout.components[0]];layout.components[0].rect={x:0,y:0,width:800,height:480};
  expect((await request.put('/api/layout',{data:{base_revision:state.revision,layout,refresh_settings:state.refresh_settings}})).ok()).toBeTruthy();
  await page.goto('/');await page.getByRole('button',{name:'Component designs',exact:true}).click();await page.getByRole('button',{name:'Create design for Text & time',exact:true}).click();
  await page.locator('#add-open').click();await page.getByRole('button',{name:'Text',exact:true}).click();await page.getByRole('textbox',{name:'Text',exact:true}).fill('A full display cannot block designing');
  await page.getByLabel('Design name',{exact:true}).fill('Crowded display note');await expect(page.getByRole('button',{name:'Save design',exact:true})).toBeEnabled();
  await page.getByRole('button',{name:'Save design',exact:true}).click();await expect(page.locator('#notice')).toContainText('Design saved');
  await page.getByRole('button',{name:'Add to display',exact:true}).click();await expect(page.locator('#notice')).toContainText('does not fit a free area');
  await expect(page.getByRole('textbox',{name:'Text',exact:true})).toHaveValue('A full display cannot block designing');
  await page.getByRole('button',{name:'Close designer',exact:true}).click();await expect(page.locator('#design-library-page')).toBeVisible();
  expect((await(await request.get('/api/state')).json()).layout.components).toHaveLength(1);
});

test('a plugin without supplied templates has a blank designer and library conflicts preserve edits',async({page})=>{
  await page.goto('/');await page.getByRole('button',{name:'Edit layout',exact:true}).click();await page.locator('#add-open').click();
  await page.getByRole('button',{name:'Design a component for Bench sensor',exact:true}).click();await expect(page.locator('#design-empty')).toBeVisible();
  await page.locator('#add-open').click();await page.getByRole('button',{name:'Text',exact:true}).click();await page.getByRole('textbox',{name:'Text',exact:true}).fill('No supplied template needed');
  await page.getByLabel('Design name',{exact:true}).fill('Bench design');
  await page.route('**/api/designs/*',route=>route.request().method()==='PUT'?route.fulfill({status:409,json:{error:'The design library changed. Reload it and try again.'}}):route.continue());
  await expect(page.getByRole('button',{name:'Save design',exact:true})).toBeEnabled();await page.getByRole('button',{name:'Save design',exact:true}).click();
  await expect(page.locator('#notice')).toContainText('Reload');await expect(page.getByRole('button',{name:'Save design',exact:true})).toBeDisabled();
  await expect(page.getByRole('textbox',{name:'Text',exact:true})).toHaveValue('No supplied template needed');
  await page.unroute('**/api/designs/*');await page.locator('#design-library-retry').click();await expect(page.getByRole('button',{name:'Save design',exact:true})).toBeEnabled();await page.getByRole('button',{name:'Save design',exact:true}).click();await expect(page.locator('#notice')).toContainText('Design saved');
});

test('plugin designs start compact, rebind connections and retain a pre-existing layout draft',async({page,request})=>{
  const connections=await(await request.get('/api/connections')).json();
  const original=connections.connections.find((c:any)=>c.id==='weather.browser');
  const extra={connector_id:original.connector_id,label:'Second weather source',enabled:false,poll_interval_secs:original.poll_interval_secs,settings:original.settings};
  const result=await request.put('/api/connections/weather.design-test',{data:{base_revision:connections.revision,...extra}});
  expect(result.ok()).toBeTruthy();
  await page.goto('/');await page.getByRole('button',{name:'Edit layout',exact:true}).click();
  for(const id of ['weather','agenda']){await page.locator(`#component-list [data-component="${id}"]`).click();await page.keyboard.press('Delete');}
  await page.locator('#component-list [data-component="header"]').click();
  const text=page.getByRole('textbox',{name:'Text',exact:true}).first();await text.fill('Keep this main draft');
  await page.getByRole('button',{name:'Component designs',exact:true}).click();await page.getByRole('button',{name:'Create design for Open-Meteo weather',exact:true}).click();
  await page.getByRole('combobox',{name:'Start from',exact:true}).selectOption({index:1});await page.getByLabel('Design name',{exact:true}).fill('Compact reusable weather');
  await page.getByRole('combobox',{name:'Information source',exact:true}).selectOption('weather.design-test');
  await expect(page.getByRole('button',{name:'Save design',exact:true})).toBeEnabled();await page.getByRole('button',{name:'Save design',exact:true}).click();await expect(page.locator('#notice')).toContainText('Design saved');
  const store=await(await request.get('/api/designs')).json();const design=store.designs.find((d:any)=>d.name==='Compact reusable weather');
  expect(design.component.blocks.every((b:any)=>b.font_size===14)).toBeTruthy();
  expect(design.component.blocks.filter((b:any)=>b.source.binding).every((b:any)=>b.source.binding.connection_id==='design.source')).toBeTruthy();
  await page.getByRole('button',{name:'Close designer',exact:true}).click();await page.getByRole('button',{name:'Back to display',exact:true}).click();
  await expect(page.getByRole('textbox',{name:'Text',exact:true}).first()).toHaveValue('Keep this main draft');
  await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(page.getByRole('textbox',{name:'Text',exact:true}).first()).toHaveValue('Paperphase');
  await page.getByRole('button',{name:'Redo',exact:true}).click();await expect(page.getByRole('textbox',{name:'Text',exact:true}).first()).toHaveValue('Keep this main draft');
  await page.locator('#add-open').click();await page.getByRole('button',{name:'Use design Compact reusable weather from Second weather source',exact:true}).click();
  await page.getByRole('button',{name:'Save to display',exact:true}).click();await expect(page.getByRole('button',{name:'Edit layout',exact:true})).toBeVisible();
  const saved=await(await request.get('/api/state')).json();const inserted=saved.layout.components.at(-1);
  expect(saved.layout.components[0].blocks[0].source.text).toBe('Keep this main draft');
  expect(inserted.blocks.filter((b:any)=>b.source.binding).every((b:any)=>b.source.binding.connection_id==='weather.design-test')).toBeTruthy();
  const latest=await(await request.get('/api/connections')).json();expect((await request.delete('/api/connections/weather.design-test',{data:{base_revision:latest.revision}})).ok()).toBeTruthy();
});

test('an overflowing existing component remains editable and can fit its content before applying',async({page,request})=>{
  const state=await(await request.get('/api/state')).json();const layout=state.layout;
  layout.components[0].blocks[0].font_size=128;
  expect((await request.put('/api/layout',{data:{base_revision:state.revision,layout,refresh_settings:state.refresh_settings}})).ok()).toBeTruthy();
  await page.goto('/');await page.getByRole('button',{name:'Edit layout',exact:true}).click();await page.locator('#component-list [data-component="header"]').click();
  await page.getByRole('button',{name:'Design this component',exact:true}).click();
  await expect(page.locator('#design-feedback')).toContainText('Clipped items:');await expect(page.getByRole('button',{name:'Save design',exact:true})).toBeEnabled();
  await page.getByRole('button',{name:'Save design',exact:true}).click();await expect(page.locator('#notice')).toContainText('Some items are clipped');
  await page.getByRole('button',{name:'Text & size',exact:true}).click();await page.getByRole('combobox',{name:'Text size',exact:true}).selectOption('14');await page.keyboard.press('Escape');
  await page.getByRole('button',{name:'Fit to contents',exact:true}).click();await expect(page.getByRole('button',{name:'Apply to layout',exact:true})).toBeEnabled();
  await page.getByRole('button',{name:'Apply to layout',exact:true}).click();await page.getByRole('button',{name:'Save to display',exact:true}).click();
  await expect(page.getByRole('button',{name:'Edit layout',exact:true})).toBeVisible();
  const current=await(await request.get('/api/state')).json();const header=current.layout.components.find((c:any)=>c.id==='header');
  expect(header.blocks.every((b:any)=>b.font_size===14)).toBeTruthy();expect(header.rect.width*header.rect.height).toBeLessThan(768*80);
});

test('designer transitions retain keyboard focus, field guidance and a reachable laptop item dock',async({page})=>{
  await page.setViewportSize({width:1024,height:768});await page.goto('/');await page.getByRole('button',{name:'Component designs',exact:true}).click();
  const create=page.getByRole('button',{name:'Create design for Text & time',exact:true});await create.focus();await page.keyboard.press('Enter');
  await expect(page.getByLabel('Design name',{exact:true})).toBeFocused();await expect(page).toHaveTitle('Paperphase · Design component');
  await page.locator('#add-open').click();await page.keyboard.press('Enter');
  const text=page.getByRole('textbox',{name:'Text',exact:true});await expect(text).toBeFocused();await text.fill('A useful note');
  const name=page.getByLabel('Design name',{exact:true});await name.fill('');await expect(name).toHaveAttribute('aria-invalid','true');await expect(page.locator('#design-name-error')).toContainText('Enter a design name');
  await expect.poll(()=>page.locator('#frame').evaluate((image:HTMLImageElement)=>image.naturalWidth)).toBeGreaterThan(0);await expect(page.locator('#design-feedback')).not.toContainText('layout.components');
  await name.fill('Laptop note');await expect(page.getByRole('button',{name:'Save design',exact:true})).toBeEnabled();
  await page.locator('#add-open').scrollIntoViewIfNeeded();
  const hit=await page.locator('#add-open').evaluate(el=>{const r=el.getBoundingClientRect();return el.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2));});expect(hit).toBeTruthy();
  expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBe(1024);
  await page.getByRole('button',{name:'Close designer',exact:true}).click();await page.getByRole('button',{name:'Discard design edits',exact:true}).click();await expect(create).toBeFocused();
});


test('an oversized plugin template can be redesigned before any placement',async({page,request})=>{
  await page.route('**/api/connectors',async route=>{
    const response=await route.fetch();const catalog=await response.json();
    const template=catalog.connectors.find((plugin:any)=>plugin.id==='weather.open-meteo').templates[0];
    template.component.rect={x:0,y:0,width:1600,height:1000};
    for(const block of template.component.blocks)block.font_size=96;
    await route.fulfill({response,json:catalog});
  });
  await page.goto('/');const before=await(await request.get('/api/state')).json();
  await page.getByRole('button',{name:'Component designs',exact:true}).click();await page.getByRole('button',{name:'Create design for Open-Meteo weather',exact:true}).click();
  await page.getByRole('combobox',{name:'Start from',exact:true}).selectOption({index:1});
  await expect(page.getByRole('button',{name:'Save design',exact:true})).toBeEnabled();
  await page.getByLabel('Design name',{exact:true}).fill('Resized oversized template');await page.getByRole('button',{name:'Save design',exact:true}).click();await expect(page.locator('#notice')).toContainText('Design saved');
  const library=await(await request.get('/api/designs')).json();const design=library.designs[0];
  expect(design.component.rect.width).toBeLessThanOrEqual(800);expect(design.component.rect.height).toBeLessThanOrEqual(480);expect(design.component.blocks.every((b:any)=>b.font_size===14)).toBeTruthy();
  expect((await(await request.get('/api/state')).json()).revision).toBe(before.revision);
});

test('internal items can overlap, stay within their component, and save with a clipping warning',async({page,request})=>{
  const state=await(await request.get('/api/state')).json();const layout=state.layout;const c=layout.components[0];c.content_layout='positioned';c.blocks[0].rect={x:0,y:0,width:240,height:40};c.blocks[1].rect={x:0,y:24,width:240,height:24};
  expect((await request.put('/api/layout',{data:{base_revision:state.revision,layout,refresh_settings:state.refresh_settings}})).ok()).toBeTruthy();
  await page.goto('/');await page.getByRole('button',{name:'Edit layout',exact:true}).click();await page.locator('#component-list [data-component="header"]').click();await page.getByRole('button',{name:'Design this component',exact:true}).click();
  await page.getByRole('tab',{name:'Position',exact:true}).click();await page.getByLabel('Element Y',{exact:true}).fill('8');await page.getByLabel('Element Y',{exact:true}).press('Tab');await expect(page.getByLabel('Element Y',{exact:true})).toHaveValue('8');
  await page.getByLabel('Element Y',{exact:true}).fill('55');await page.getByLabel('Element Y',{exact:true}).press('Tab');await expect(page.getByLabel('Element Y',{exact:true})).toHaveValue('8');
  await page.getByLabel('Element height',{exact:true}).fill('8');await page.getByLabel('Element height',{exact:true}).press('Tab');await expect(page.locator('#design-feedback')).toContainText('Clipped items:');await expect(page.getByRole('button',{name:'Save design',exact:true})).toBeEnabled();
  await page.getByRole('button',{name:'Save design',exact:true}).click();await expect(page.locator('#notice')).toContainText('Some items are clipped');
  const library=await(await request.get('/api/designs')).json();expect(library.designs[0].component.blocks[0].rect.y).toBe(8);expect(library.designs[0].component.blocks[1].rect.y).toBe(24);
  await page.getByRole('button',{name:'Fit to contents',exact:true}).click();await expect(page.locator('#design-feedback')).not.toContainText('Clipped items:');
  await expect(page.getByRole('button',{name:'Apply to layout',exact:true})).toBeEnabled();
  await expect.poll(()=>page.getByLabel('Element height',{exact:true}).inputValue().then(Number)).toBeGreaterThan(8);
});


test('icons follow a source field and default to its icon key',async({page,request})=>{
  await page.goto('/');await page.getByRole('button',{name:'Component designs',exact:true}).click();await page.getByRole('button',{name:'Create design for Open-Meteo weather',exact:true}).click();await page.getByRole('combobox',{name:'Start from',exact:true}).selectOption({index:1});
  await expect(page.locator('#component-list [data-element="symbol"]')).toBeVisible();await page.locator('#component-list [data-element="symbol"]').click();
  await page.getByRole('combobox',{name:'Icon selection',exact:true}).selectOption('literal');await page.getByRole('combobox',{name:'Icon selection',exact:true}).selectOption('field');
  await expect(page.getByRole('combobox',{name:'Show information',exact:true})).toHaveValue('icon-key');await expect(page.getByLabel('Item number',{exact:true})).toHaveValue('1');
  await page.getByLabel('Design name',{exact:true}).fill('Weather with a linked icon');await expect(page.getByRole('button',{name:'Save design',exact:true})).toBeEnabled();await page.getByRole('button',{name:'Save design',exact:true}).click();await expect(page.locator('#notice')).toContainText('Design saved');
  const library=await(await request.get('/api/designs')).json();const symbol=library.designs[0].component.blocks.find((b:any)=>b.id==='symbol');expect(symbol.source.icon.kind).toBe('field');expect(symbol.source.icon.binding.field_id).toBe('icon-key');expect(symbol.source.icon.binding.connection_id).toBe('design.source');
});
