import {test,expect,type Page} from '@playwright/test';
import {readFileSync} from 'node:fs';

test.beforeEach(async({request})=>{
  const state=await(await request.get('/api/state')).json();
  const layout=JSON.parse(readFileSync('../examples/simulator/rotation.json','utf8'));
  layout.display=state.layout.display;
  expect((await request.put('/api/layout',{data:{base_revision:state.revision,layout,refresh_settings:{quiet_hours:null}}})).ok()).toBeTruthy();
  await expect.poll(async()=>(await(await request.get('/api/state')).json()).phase).toBe('idle');
});
async function openArea(page:Page){
  await page.goto('/');await page.getByRole('button',{name:'Edit layout',exact:true}).click();
  await page.locator('#component-list [data-component="news"]').click();
}
test('rotation previews are independent; timing, pause, order and removal save normally',async({page,request})=>{
  await openArea(page);
  const before=await(await request.get('/api/state')).json();
  const pixels=await(await request.get('/api/display.png')).body();
  await page.locator('.rotation-timing summary').click();
  await expect(page.getByLabel('Minutes per item',{exact:true})).toHaveValue('5');
  await page.getByRole('button',{name:'Preview next item',exact:true}).click();
  await expect(page.locator('.rotation-item.previewing')).toContainText('World');
  expect(await(await request.get('/api/display.png')).body()).toEqual(pixels);
  expect((await(await request.get('/api/state')).json()).revision).toBe(before.revision);
  await expect(page.getByRole('button',{name:'Save to display',exact:true})).toBeDisabled();
  await page.getByLabel('Minutes per item',{exact:true}).fill('7');
  await page.getByLabel('Minutes per item',{exact:true}).press('Tab');
  await page.getByRole('checkbox',{name:'Pause rotation'}).check();
  await expect(page.getByLabel('Keep showing',{exact:true})).toHaveValue('world');
  await page.locator('.rotation-item').nth(1).getByRole('button',{name:'Move item up',exact:true}).click();
  await page.getByRole('button',{name:'Remove Culture headlines from rotation',exact:true}).click();
  await page.getByRole('button',{name:'Save to display',exact:true}).click();
  await expect(page.getByRole('button',{name:'Edit layout',exact:true})).toBeVisible();
  await expect.poll(async()=>(await(await request.get('/api/state')).json()).phase).toBe('idle');
  const after=await(await request.get('/api/state')).json();
  expect(after.layout.components[0].rotation).toMatchObject({interval_secs:420,paused:true,paused_member:'world'});
  expect(after.layout.components[0].rotation.members[0].id).toBe('world');
});
test('moving a component into an area frees its old space and remains undoable',async({page,request})=>{
  await openArea(page);
  await page.getByRole('button',{name:'Add item to rotation',exact:true}).click();
  await page.getByRole('button',{name:'Move Pinned information to rotation',exact:true}).click();
  await expect(page.locator('#component-list [data-component="pinned"]')).toHaveCount(0);
  await expect(page.locator('.rotation-item')).toHaveCount(4);
  await page.getByRole('button',{name:'Undo',exact:true}).click();
  await expect(page.locator('#component-list [data-component="pinned"]')).toHaveCount(1);
  await expect(page.locator('.rotation-item')).toHaveCount(3);
  await page.getByRole('button',{name:'Redo',exact:true}).click();
  await page.getByRole('button',{name:'Save to display',exact:true}).click();
  await expect(page.getByRole('button',{name:'Edit layout',exact:true})).toBeVisible();
  const after=await(await request.get('/api/state')).json();
  expect(after.layout.components).toHaveLength(1);
  expect(after.layout.components[0].rotation.members).toHaveLength(4);
});
test('individual item designer preserves the area and pinned neighbours',async({page,request})=>{
  await openArea(page);
  await page.getByRole('button',{name:'Edit design for World headlines',exact:true}).click();
  await expect(page.locator('#page-title')).toHaveText('Design component');
  await page.getByRole('tab',{name:'Style',exact:true}).click();
  await page.getByLabel('Update method',{exact:true}).selectOption('partial');
  await page.locator('#design-name').fill('World edited');
  await page.getByRole('button',{name:'Apply to item',exact:true}).click();
  await expect(page.locator('.rotation-item')).toContainText(['Local headlines','World edited','Culture headlines']);
  await page.getByRole('button',{name:'Save to display',exact:true}).click();
  await expect(page.getByRole('button',{name:'Edit layout',exact:true})).toBeVisible();
  const after=await(await request.get('/api/state')).json();
  expect(after.layout.components[0].rect).toEqual({x:8,y:8,width:160,height:120});
  expect(after.layout.components[1].id).toBe('pinned');
  expect(after.layout.components[0].rotation.members[1].refresh_hint).toBe('partial');
  expect(after.layout.components[0].rotation.members[1].rect).toEqual({x:0,y:0,width:128,height:96});
});
test('preview selection endpoint validates all items and never changes displayed state',async({request})=>{
  const before=await(await request.get('/api/state')).json();
  const response=await request.post('/api/preview',{data:{layout:before.layout,rotation_selection:{news:'world'}}});
  expect(response.ok()).toBeTruthy();
  const after=await(await request.get('/api/state')).json();
  expect(after.rotation_epoch).toBe(before.rotation_epoch);expect(after.frame_id).toBe(before.frame_id);
  expect((await request.post('/api/preview',{data:{layout:before.layout,rotation_selection:{news:'missing'}}})).status()).toBe(422);
});

test('Show in turns creates a compact item through the existing designer and returns picker focus',async({page,request})=>{
  await openArea(page);
  await page.locator('#component-list [data-component="pinned"]').click();
  await page.getByRole('button',{name:'Show this component in turns with others',exact:true}).click();
  await expect(page.locator('.rotation-item')).toHaveCount(1);
  const add=page.getByRole('button',{name:'Add item to rotation',exact:true});
  await add.click();await expect(add).toHaveAttribute('aria-expanded','true');
  await page.getByRole('button',{name:'Close component picker',exact:true}).click();
  await expect(add).toBeFocused();await expect(add).toHaveAttribute('aria-expanded','false');
  await add.click();await page.keyboard.press('Escape');await expect(add).toBeFocused();
  await page.keyboard.press('Delete');await expect(page.locator('#component-list [data-component="pinned"]')).toHaveCount(1);
  await add.click();await page.getByRole('button',{name:'Create a rotating item',exact:true}).click();
  await page.locator('#add-open').click();await page.getByRole('button',{name:'Text',exact:true}).click();
  await page.getByRole('textbox',{name:'Text',exact:true}).fill('Quick note');
  await page.getByLabel('Design name',{exact:true}).fill('Small note');
  await page.getByRole('button',{name:'Add to rotation',exact:true}).click();
  await expect(page.locator('.rotation-item')).toHaveCount(2);
  await page.getByRole('button',{name:'Save to display',exact:true}).click();
  await expect(page.getByRole('button',{name:'Edit layout',exact:true})).toBeVisible();
  const state=await(await request.get('/api/state')).json();
  const area=state.layout.components.find((c:any)=>c.id==='pinned');
  expect(area.rotation.interval_secs).toBe(300);expect(area.rotation.members[1].label).toBe('Small note');
  expect(area.rect).toEqual({x:176,y:8,width:136,height:96});
});

test('incompatible saved designs require explicit resizing and remain independent of the library',async({page,request})=>{
  let library=await(await request.get('/api/designs')).json();
  const design={id:'wide-note',label:'Wide sample',rect:{x:0,y:0,width:240,height:120},padding:8,gap:4,border:false,blocks:[{id:'note',font_size:14,source:{kind:'literal',text:'Independent copy'}}]};
  const saved=await request.put('/api/designs/rotation-copy',{data:{base_revision:library.revision,connector_id:'paperphase.builtin',name:'Wide sample',component:design}});expect(saved.ok()).toBeTruthy();
  library=await saved.json();
  await openArea(page);
  await page.getByRole('button',{name:'Add item to rotation',exact:true}).click();
  const choice=page.locator('.rotation-choice').filter({has:page.locator('strong',{hasText:'Wide sample'})});
  await expect(choice).toContainText('Larger than this area');
  // The pinned neighbour blocks enlargement. It must stay put.
  await expect(choice.getByRole('button',{name:/Resize area/})).toHaveCount(0);
  await choice.getByRole('button',{name:'Customise Wide sample for rotation',exact:true}).click();
  await expect(page.getByRole('button',{name:'Add to rotation',exact:true})).toBeDisabled();
  await page.getByRole('tab',{name:'Position',exact:true}).click();
  await page.getByLabel('Width',{exact:true}).fill('152');await page.getByLabel('Width',{exact:true}).press('Tab');
  await page.getByRole('button',{name:'Add to rotation',exact:true}).click();
  await expect(page.locator('.rotation-item')).toHaveCount(4);
  expect((await request.delete('/api/designs/rotation-copy',{data:{base_revision:library.revision}})).ok()).toBeTruthy();
  await page.getByRole('button',{name:'Save to display',exact:true}).click();
  await expect(page.getByRole('button',{name:'Edit layout',exact:true})).toBeVisible();
  const state=await(await request.get('/api/state')).json();
  expect(state.layout.components[0].rect.width).toBe(160);
  expect(state.layout.components[0].rotation.members[3].blocks[0].source.text).toBe('Independent copy');
  expect(state.layout.components[1].rect.x).toBe(176);
});
