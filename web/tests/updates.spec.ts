import {test,expect,type Page} from '@playwright/test';
import {readFileSync} from 'node:fs';
const baseState=()=>({managed:true,version:'0.1.0',status_port:0,targets:[{id:'application',name:'Paperphase',repository:'Odd-Orbit-Software/Paperphase',publisher_configured:true,installed:{version:'0.1.0',build:'sample-installed',tag:'v0.1.0-sample.1'}},{id:'weather.open-meteo',name:'Weather',repository:'Odd-Orbit-Software/Paperphase',publisher_configured:true,installed:{version:'0.1.0',build:'sample-weather',tag:'v0.1.0-sample.1'}}],job:null as any});
const sampleRelease={tag:'v0.1.0-sample.2',name:'Sample release for UI testing',notes:'Demonstration release notes.\nPreserves your display and saved designs.',url:'https://github.com/Odd-Orbit-Software/Paperphase/releases/tag/v0.1.0-sample.2',prerelease:false,asset:{name:'paperphase-aarch64-unknown-linux-gnu.ppkg',bytes:10000}};
async function open(page:Page){await page.goto('/');await page.getByRole('button',{name:'Device',exact:true}).click();await page.getByRole('button',{name:'Updates',exact:true}).click();}
test.beforeEach(async({request})=>{const state=await(await request.get('/api/state')).json();const layout=JSON.parse(readFileSync('../examples/simulator/layout.json','utf8'));layout.components=layout.components.slice(0,3);expect((await request.put('/api/layout',{data:{base_revision:state.revision,layout,refresh_settings:{quiet_hours:null}}})).ok()).toBeTruthy();});
test('release checking is manual, experimental opt-in, and activation needs confirmation',async({page})=>{
 const state=baseState();let checks:any[]=[];let installs:any[]=[];
 await page.route('**/api/updates',route=>route.fulfill({json:state}));
 await page.route('**/api/updates/check',route=>{checks.push(route.request().postDataJSON());return route.fulfill({json:{targets:{application:[sampleRelease],'weather.open-meteo':[]},errors:{}}});});
 await page.route('**/api/updates/install',route=>{installs.push(route.request().postDataJSON());state.job={id:'sample-job',target:'application',phase:'downloading',message:'Downloading the sample release…',finished:false,release:null};return route.fulfill({status:202,json:state.job});});
 await open(page);await expect(page.getByRole('button',{name:'Check for updates',exact:true})).toBeVisible();expect(checks).toHaveLength(0);
 await page.getByRole('button',{name:'Check for updates',exact:true}).click();await expect(page.getByRole('button',{name:'Update Paperphase',exact:true})).toBeEnabled();expect(checks[0]).toMatchObject({target:'all',experimental:false});
 await page.locator('.release-notes summary').click();await expect(page.locator('.release-notes-text')).toContainText('Demonstration release notes');
 await page.getByRole('button',{name:'Update Paperphase',exact:true}).click();await page.getByRole('button',{name:'Cancel',exact:true}).click();expect(installs).toHaveLength(0);
 await page.getByRole('button',{name:'Update Paperphase',exact:true}).click();await page.getByRole('button',{name:'Update and restart',exact:true}).click();
 await expect(page.getByRole('region',{name:'Update progress'})).toContainText('Downloading the sample');expect(installs[0]).toMatchObject({target:'application',tag:sampleRelease.tag,experimental:false});
 await expect(page.getByRole('button',{name:'Check for updates',exact:true})).toBeDisabled();
});
test('unsaved layout changes are retained and block installation while checks still work',async({page})=>{
 const state=baseState();await page.route('**/api/updates',r=>r.fulfill({json:state}));await page.route('**/api/updates/check',r=>r.fulfill({json:{targets:{application:[sampleRelease]},errors:{}}}));
 await page.goto('/');await page.getByRole('button',{name:'Edit layout',exact:true}).click();await page.locator('#component-list [data-component=header]').click();await page.getByRole('textbox',{name:'Text',exact:true}).first().fill('Retain this unsaved title');
 await page.getByRole('button',{name:'Device',exact:true}).click();await page.getByRole('button',{name:'Updates',exact:true}).click();
 await page.getByRole('button',{name:'Check for updates',exact:true}).click();await expect(page.getByRole('button',{name:'Update Paperphase',exact:true})).toBeDisabled();await expect(page.locator('.update-advisory')).toContainText('Save or discard');await expect(page.getByRole('button',{name:'Save to display',exact:true})).toBeVisible();
 await page.getByRole('button',{name:'Display',exact:true}).click();await expect(page.getByRole('textbox',{name:'Text',exact:true}).first()).toHaveValue('Retain this unsaved title');
});
test('the supervisor keeps progress readable through restart and reports recovery failure',async({page})=>{
 const state=baseState();state.status_port=9099;state.job={id:'sample-job',target:'application',phase:'starting',message:'Restarting Paperphase and checking it is ready…',finished:false,release:null};let mainAlive=true;let helperJob=state.job;
 await page.route('**/',async route=>{const response=await route.fetch();const headers={...response.headers(),'content-security-policy':"default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self' http://127.0.0.1:9099"};await route.fulfill({response,headers});});
 await page.route('**/api/updates',route=>mainAlive?route.fulfill({json:state}):route.abort());
 await page.route('http://127.0.0.1:9099/api/update-status',route=>route.fulfill({json:{job:helperJob},headers:{'access-control-allow-origin':new URL(page.url()).origin}}));
 await open(page);await expect(page.getByRole('region',{name:'Update progress'})).toContainText('Restarting Paperphase');mainAlive=false;
 await expect(page.locator('#device-status')).toHaveText('Restarting Paperphase');
 helperJob={...helperJob,phase:'recovery_required',message:'Automatic rollback failed. Your backup is retained; use SSH recovery.',finished:true};
 await expect(page.getByRole('region',{name:'Update progress'})).toContainText('Recovery needed');await expect(page.getByRole('button',{name:'Check for updates',exact:true})).toBeDisabled();
});
test('manual installations explain setup and advanced controls keep experiments opt-in',async({page})=>{
 let state:any={managed:false,version:'0.1.0',targets:[],job:null};await page.route('**/api/updates',route=>route.fulfill({json:state}));
 await open(page);await expect(page.getByRole('heading',{name:'Update setup needed',exact:true})).toBeVisible();
 state=baseState();await page.getByRole('button',{name:'Display settings',exact:true}).click();await page.getByRole('button',{name:'Updates',exact:true}).click();await expect(page.getByRole('button',{name:'Check for updates',exact:true})).toBeVisible();
 await page.locator('#update-advanced summary').click();await expect(page.getByRole('checkbox',{name:'Include experimental pre-releases',exact:true})).not.toBeChecked();
 await expect(page.getByLabel('GitHub release URL',{exact:true})).toBeVisible();await expect(page.getByLabel('Signed release package (.ppkg)',{exact:true})).toBeVisible();
});

test('target controls retain focus and uploaded files survive polling, then progress is revealed once',async({page})=>{
 const state=baseState();
 await page.route('**/api/updates',r=>r.fulfill({json:state}));
 const next={...sampleRelease,tag:'v0.1.0-sample.3'};
 await page.route('**/api/updates/check',r=>r.fulfill({json:{targets:{application:[sampleRelease,next],'weather.open-meteo':[sampleRelease,next]},errors:{}}}));
 await page.route('**/api/updates/install',r=>{state.job={id:'sample-job',target:'weather.open-meteo',phase:'downloading',message:'Downloading the selected plugin…',finished:false,release:null};return r.fulfill({status:202,json:state.job});});
 await open(page);await page.getByRole('button',{name:'Check for updates',exact:true}).click();
 const weather=page.getByRole('combobox',{name:'Release for Weather',exact:true});await weather.focus();await weather.selectOption(next.tag);await expect(weather).toBeFocused();
 await page.locator('#update-advanced summary').click();
 const file=page.getByLabel('Signed release package (.ppkg)',{exact:true});await file.setInputFiles({name:'sample.ppkg',mimeType:'application/octet-stream',buffer:Buffer.from('sample test bytes')});
 const url=page.getByLabel('GitHub release URL',{exact:true});await url.fill('https://github.com/Odd-Orbit-Software/Paperphase/releases/tag/v0.1.0');await url.focus();
 state.job={id:'previous-job',target:'application',phase:'healthy',message:'Previous test update complete.',finished:true,release:null};await expect(page.getByRole('region',{name:'Update progress'})).toBeVisible();await expect(url).toBeFocused();await url.press('Escape');await expect(url).toBeFocused();
 expect(await file.evaluate((input:HTMLInputElement)=>input.files?.length)).toBe(1);
 await page.setViewportSize({width:390,height:844});await page.getByRole('button',{name:'Update Weather',exact:true}).click();await page.getByRole('button',{name:'Update and restart',exact:true}).click();
 const progress=page.getByRole('region',{name:'Update progress'});await expect(progress).toBeFocused();const box=await progress.boundingBox();expect(box!.y).toBeGreaterThanOrEqual(0);expect(box!.y).toBeLessThan(200);
 const first=await page.evaluate(()=>window.scrollY);await page.waitForTimeout(1200);expect(Math.abs(await page.evaluate(()=>window.scrollY)-first)).toBeLessThan(2);
});
