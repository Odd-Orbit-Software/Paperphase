import type { Rect, Binding, ClockSpec, Block, Component, Layout, BlockPlacement, IconPack } from './model';
import { sourceBinding } from './model';
import { bindDesign, compactDesign, designFits, designGeometryValid, friendlyField, builtinDesigns, type SavedDesign, type DesignLibrary } from './designs';
import { contentBounds, validElement, validElements, positioned, freeElementRect } from './elements';
type Policy = { min_interval_ms: number; full_after_partials: number; full_after_secs: number; max_partial_area_percent: number };
type Operation = { mode: 'partial' | 'full'; reason: string; region: Rect; revision: number };
type RefreshStatus = { profile: { id: string; max_partials: number; max_full_age_secs: number; max_partial_area_percent: number; min_interval_ms: number }; active: Operation | null; last_completed: Operation | null; partial_count: number; baseline_known: boolean; failures: number; wait_ms: number };
type RefreshSettings = { policy: Policy; quiet_hours: { timezone: string; start: string; end: string } | null };
type Workspace = { clock_fixed: boolean; refresh: RefreshStatus; last_error: string | null; revision: number; displayed_revision: number; data_revision: number; frame_id: string; clock_now: number; pending: boolean; phase: string; layout: Layout; refresh_settings: RefreshSettings; addresses: string[]; connector_execution: boolean; fixture_data: boolean };
type EditState = { layout: Layout; settings: RefreshSettings; design?: {source?:string;automatic:boolean;fixedWidth?:number;startingPoint?:string} };
type Dataset = { connection_id: string; connection_label?: string; dataset_id: string; fields: { id: string; kind: string; unit?: string }[]; rows: number; stale: boolean };
type Report = { id: string; state: string; warnings: string[]; blocks: BlockPlacement[] };
type Preview = { png_base64: string; components: Report[] };
type ComponentTemplate = { id: string; name: string; component: Component };
type SettingColumn = { id:string; label:string; max_bytes:number; multiline?:boolean; required:boolean; choices:{value:string;label:string}[] };
type SettingValue = string | number | boolean | null | Record<string,string>[];
type SettingInput = {type:'table';max_rows:number;columns:SettingColumn[];default:Record<string,string>[]} | { type: 'url'; default?: string | null } | { type: 'number'; min: number; max: number; default?: number | null } | { type: 'text'; multiline?: boolean; max_bytes: number; default?: string | null } | { type: 'secret'; max_bytes: number } | { type: 'integer'; min: number; max: number; default?: number | null } | { type: 'boolean'; default?: boolean | null } | { type: 'choice'; options: { value: string; label: string }[]; default?: string | null };
type SettingDefinition = { id: string; label: string; help: string; required: boolean; input: SettingInput };
type ConnectorDefinition = { id: string; name: string; description: string; min_poll_secs: number; default_poll_secs: number; templates?: ComponentTemplate[]; settings: SettingDefinition[] };
type FetchStatus = { status: string; can_fetch: boolean; fetch_after: number; next_fetch_at: number; last_attempt_at: number | null; last_success_at: number | null; last_error: string | null };
type Connection = { id: string; connector_id: string; label: string; enabled: boolean; poll_interval_secs: number; settings: Record<string, SettingValue>; secret_present: string[]; fetch?: FetchStatus };
type GoogleAccount = { id: string; label: string; requires_reauth: boolean; calendars: {id: string; label: string; primary: boolean}[] };
type GoogleStatus = {configured: boolean; client_id: string | null; client_secret_present: boolean; redirect_uri: string; accounts: GoogleAccount[]; last_link_error?: {code:string;message:string} | null};
type Connections = { revision: number; connections: Connection[]; execution: string };
class ApiFailure extends Error { constructor(message: string, readonly status: number) { super(message); } }

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const clone = <T>(value: T): T => structuredClone(value);
const button = (id: string) => $<HTMLButtonElement>(id);
const checked = (id: string) => $<HTMLInputElement>(id).checked;
const equal = <T>(a: T, b: T) => JSON.stringify(a) === JSON.stringify(b);
let saved: Workspace;
let draft: Layout;
let draftSettings: RefreshSettings;
let zones: string[] = [];
let catalog: Dataset[] = [];
let iconPacks: IconPack[] = [];
let editing = false;
let viewingDraft = false;
let selected: string | null = null;
let contentMode = false;
let selectedElement: string | null = null;
let converting = false;
let undo: EditState[] = [];
let redo: EditState[] = [];
let reports: Report[] = [];
let draftImage = '';
let displayedFrame = '';
let draftMinute = -1;
let zoom = 1;
let autoFit = true;
let saving = false;
let displayNoticePending = false;
let conflict = false;
let previewTimer = 0;
let previewSequence = 0;
let previewAbort: AbortController | undefined;
let coalescedKey = '';
let lastEdit = 0;
let connected = true;
let lastInspectedId: string | null = null;
let drag: { id: string; blockId?: string; startX: number; startY: number; original: Rect; last: Rect; baseline: EditState; resize: boolean; pointerId: number; invalid: boolean } | undefined;
type Page = 'display' | 'sources' | 'device' | 'about' | 'designer' | 'designs';
let currentPage: Page = 'display';
let inspectorTab = 'content';
let googleView = false;
let choosingSource = false;
let pendingCalendar = false;
type RetainedEditor = { state: EditState; editing: boolean; viewingDraft: boolean; selected: string | null; contentMode: boolean; selectedElement: string | null; undo: EditState[]; redo: EditState[]; image: string; reports: Report[]; conflict: boolean; page: Page; focusId?: string; focusLabel?: string };
type DesignSession = { previous: RetainedEditor; connector: string; source?: string; id?: string; baseline: Component; original?: string; busy: boolean; automatic: boolean; fixedWidth?: number; ready: boolean; error: string; needsFit: boolean; libraryConflict?: boolean; startingPoint?: string; recordFit?: EditState };
let designSession: DesignSession | undefined;
let designLibrary: DesignLibrary = {schema_version:1,revision:0,designs:[]};
let designLibraryLoaded = false;
const iconPaths: Record<string, string> = {
  undo: '<path d="M8 7H3V2M3 7a8 8 0 1 1-1 8"/>',
  redo: '<path d="M16 7h5V2m0 5a8 8 0 1 0 1 8"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  minus: '<path d="M5 12h14"/>',
  close: '<path d="m6 6 12 12M6 18 18 6"/>',
  up: '<path d="m5 14 7-7 7 7"/>', down: '<path d="m5 10 7 7 7-7"/>',
  text: '<path d="M4 5h16M12 5v14M8 19h8"/>',
  data: '<path d="M4 6h16M4 12h16M4 18h16"/><circle cx="8" cy="6" r="2"/><circle cx="16" cy="12" r="2"/><circle cx="9" cy="18" r="2"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 6v6l4 2"/>',
};
function icon(kind: string) {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 24 24'); svg.setAttribute('fill', 'none');
  svg.setAttribute('stroke', 'currentColor'); svg.setAttribute('stroke-width', '1.6');
  svg.setAttribute('stroke-linecap', 'round'); svg.setAttribute('stroke-linejoin', 'round');
  svg.setAttribute('aria-hidden', 'true'); svg.innerHTML = iconPaths[kind] ?? iconPaths.data!;
  return svg;
}
for (const element of document.querySelectorAll<HTMLElement>('[data-icon]')) element.append(icon(element.dataset.icon!));

const focusables = (root: ParentNode) => [...root.querySelectorAll<HTMLElement>('button, input, select, textarea, summary, a[href], [tabindex="0"]')];
function focusKey(target: HTMLElement) {
  if (target.id) return `id:${target.id}`;
  const scope = target.closest<HTMLElement>('[data-block], [data-component], [data-element]');
  return [scope?.dataset.block ?? scope?.dataset.component ?? scope?.dataset.element ?? '', target.tagName,
    target.getAttribute('aria-label') ?? target.getAttribute('data-focus') ?? target.textContent?.trim()].join('|');
}
let tabDestination: { element: HTMLElement; key: string } | undefined;
let changingControl: HTMLElement | undefined;
document.addEventListener('keydown', event => {
  if (event.key !== 'Tab') { tabDestination = undefined; return; }
  const items = focusables(document).filter(el => !el.closest('[hidden], [inert]') && !el.matches(':disabled') && el.getClientRects().length);
  const index = items.indexOf(document.activeElement as HTMLElement);
  const next = items[index + (event.shiftKey ? -1 : 1)];
  tabDestination = next ? { element: next, key: focusKey(next) } : undefined;
  window.setTimeout(() => { tabDestination = undefined; }, 0);
}, true);
function retainFocus(root: HTMLElement, build: () => void) {
  const activeElement = document.activeElement as HTMLElement | null;
  const previous = activeElement && root.contains(activeElement) ? activeElement : changingControl && root.contains(changingControl) ? changingControl : activeElement;
  const owned = !!previous && root.contains(previous);
  const key = owned ? focusKey(previous!) : '';
  const selection = previous instanceof HTMLTextAreaElement || previous instanceof HTMLInputElement && /^(text|password|url|search)$/.test(previous.type)
    ? [previous.selectionStart, previous.selectionEnd] : null;
  const scroll = root.scrollTop;
  const disclosures = new Map([...root.querySelectorAll('details')].map(details => [details.id || details.querySelector('summary')?.textContent || '', details.open]));
  build();
  for (const details of root.querySelectorAll('details')) { const open = disclosures.get(details.id || details.querySelector('summary')?.textContent || ''); if (open !== undefined) details.open = open; }
  if (!owned || previous?.isConnected) return;
  const replacement = focusables(root).find(el => focusKey(el) === key);
  replacement?.focus({ preventScroll: true });
  if (selection && replacement && (replacement instanceof HTMLInputElement || replacement instanceof HTMLTextAreaElement)) {
    try { replacement.setSelectionRange(selection[0] ?? null, selection[1] ?? null); } catch { /* Not a text field. */ }
  }
  root.scrollTop = scroll;
  const destination = tabDestination;
  if (destination) window.setTimeout(() => {
    const next = destination.element.isConnected ? destination.element : focusables(document).find(el => focusKey(el) === destination.key);
    next?.focus({ preventScroll: true });
  });
}
async function confirmAction(title: string, description: string, accept: string): Promise<boolean> {
  const dialog = $<HTMLDialogElement>('confirm-dialog');
  if (dialog.open) return false;
  $('confirm-title').textContent = title; $('confirm-description').textContent = description;
  button('confirm-accept').textContent = accept;
  dialog.returnValue = ''; dialog.showModal();
  return new Promise(resolve => dialog.addEventListener('close', () => resolve(dialog.returnValue === 'confirm'), { once: true }));
}
function closePicker() { $('add-picker').hidden = true; button('add-open').setAttribute('aria-expanded', 'false'); }
function showPage(page: Page) {
  if (designSession && page !== 'designer') { notice('Finish this design or choose Close designer before leaving.'); return false; }
  if (currentPage === 'sources' && page !== 'sources' && (connectionBusy || connectionDirty())) {
    connectionNotice('Save or discard your form before leaving data sources.'); return false;
  }
  currentPage = page; closePicker();
  $('display-workspace').hidden = page === 'sources' || page === 'about' || page === 'designs';
  $('display-page').hidden = page !== 'display' && page !== 'designer';
  $('design-library-page').hidden = page !== 'designs'; $('display-page').classList.toggle('design-studio',page==='designer');
  $('designer-controls').hidden = page !== 'designer'; $('device-page').hidden = page !== 'device';
  $('about-page').hidden = page !== 'about';
  if (page === 'sources') { if (!connectionDialog().open) connectionDialog().show(); }
  else connectionDialog().close();
  for (const [id, active] of [['display-open', page === 'display' || page === 'designer' || page === 'designs'], ['connections-open', page === 'sources'], ['display-settings', page === 'device'], ['about-open', page === 'about']] as const) {
    button(id).classList.toggle('active', active);
    if (active) button(id).setAttribute('aria-current', 'page'); else button(id).removeAttribute('aria-current');
  }
  $('page-title').textContent = page === 'designer' ? 'Design component' : page === 'device' ? 'Display settings' : 'Your display';
  document.title = `Paperphase · ${page === 'designer' ? 'Design component' : page === 'designs' ? 'Component designs' : page === 'sources' ? 'Data sources' : page === 'about' ? 'About' : page === 'device' ? 'Display settings' : 'Your display'}`;
  if (saved) render();
  return true;
}
function capture(): EditState { return {layout:clone(draft),settings:clone(draftSettings),design:designSession?{source:designSession.source,automatic:designSession.automatic,fixedWidth:designSession.fixedWidth,startingPoint:designSession.startingPoint}:undefined}; }
function restore(state: EditState) { draft=clone(state.layout);draftSettings=clone(state.settings);if(designSession&&state.design){Object.assign(designSession,state.design);designSession.needsFit=false;} }

async function api<T>(path: string, options?: RequestInit): Promise<T> {
  const response = await fetch(path, options);
  if (!response.ok) {
    let message = `Request failed (${response.status}).`;
    try { const body = await response.json(); message = body.error || message; } catch { /* plain framework error */ }
    if (response.status === 409 && path === '/api/layout') conflict = true;
    throw new ApiFailure(message, response.status);
  }
  return response.json() as Promise<T>;
}
function notice(message = '', error = false) {
  displayNoticePending = false;
  $('notice').hidden = !message;
  $('notice').textContent = message;
  $('notice').classList.toggle('error', error);
}
function dirty() { if (designSession) return !equal(component(), designSession.baseline); return editing && (!equal(draft, saved.layout) || !equal(draftSettings, saved.refresh_settings)); }
function active() { return editing && viewingDraft && !saving; }
function component() { return draft.components.find(c => c.id === selected); }
function editingContents() { return (!!designSession || contentMode) && editing && viewingDraft && !!component(); }
function element() { return component()?.blocks.find(block => block.id === selectedElement); }
function elementName(block: Block) { return block.source.kind === 'literal' ? block.source.text.trim().split('\n')[0]?.slice(0, 36) || 'Empty text' : block.source.kind === 'field' ? (designSession?friendlyField(block.source.binding.field_id):block.source.binding.field_id) : block.source.kind === 'icon' ? 'Icon' : 'Time & date'; }
function name(c: Component): string {
  if (c.label) return c.label;
  const first = c.blocks[0];
  if (first?.source.kind === 'literal' && first.source.text.trim()) return first.source.text.trim().split('\n')[0]!.slice(0, 36);
  const field = c.blocks.find(b => b.source.kind === 'field');
  if (field?.source.kind === 'field') return `${field.source.binding.field_id} · ${field.source.binding.connection_id}`;
  if (c.blocks.some(b => b.source.kind === 'clock')) return 'Time & date';
  return 'Untitled text';
}
function validRect(rect: Rect, id: string, layout = draft): boolean {
  const target = layout.components.find(c => c.id === id);
  const padding = 2 * (target?.padding ?? 12);
  const status = target?.blocks.some(b => sourceBinding(b.source)) ? 18 : 0;
  return Object.values(rect).every(Number.isInteger)
    && rect.x >= 0 && rect.y >= 0 && rect.width >= 32 && rect.height >= 32
    && rect.width > padding && rect.height > padding + status
    && rect.x + rect.width <= layout.display.width && rect.y + rect.height <= layout.display.height
    && !layout.components.some(c => c.id !== id && rect.x < c.rect.x + c.rect.width && rect.x + rect.width > c.rect.x && rect.y < c.rect.y + c.rect.height && rect.y + rect.height > c.rect.y);
}
function locallyValid(layout: Layout) {
  if (designSession) return layout.components.length === 1 && designGeometryValid(layout.components[0]!);
  return layout.components.length <= 64 && layout.components.every(c => validRect(c.rect, c.id, layout) && validElements(c));
}
function remember(before: EditState, key = '') {
  const now = Date.now();
  if (!key || coalescedKey !== key || now - lastEdit > 800) {
    undo.push(before);
    if (undo.length > 50) undo.shift();
  }
  coalescedKey = key;
  lastEdit = now;
  redo = [];
}
function change(update: (layout: Layout) => void, key = '', rebuild = true) {
  if (!active()) return;
  const before = capture();
  update(draft);
  if (designSession) { designSession.recordFit=undefined; designSession.needsFit=designSession.automatic; const c=component(); if (c) draft.display = {id:'component-design',width:c.rect.width,height:c.rect.height}; designSession.ready=false; }
  if (!locallyValid(draft)) {
    restore(before);
    notice('That change leaves the content area or too little content space. The previous valid arrangement is kept.', true);
    renderInspector();
    return;
  }
  const quiet = draftSettings.quiet_hours;
  if (quiet && (!/^([01]\d|2[0-3]):[0-5]\d$/.test(quiet.start) || !/^([01]\d|2[0-3]):[0-5]\d$/.test(quiet.end) || quiet.start === quiet.end)) {
    restore(before); notice('Quiet hours need valid, distinct start and end times.', true); renderInspector(); return;
  }
  if (equal(before, capture())) return;
  remember(before, key);
  normaliseSelection();
  notice();
  render(rebuild);
  queuePreview();
}
function normaliseSelection() {
  if (designSession) { contentMode=true; if (selectedElement && !element()) selectedElement=null; return; }
  if (!component() || component()?.content_layout !== 'positioned') { contentMode = false; selectedElement = null; }
  if (selectedElement && !element()) selectedElement = null;
}
async function enterContents() {
  const current = component(); if (!current || converting || !active()) return;
  if (current.content_layout === 'positioned') { contentMode = true; selectedElement = current.blocks[0]?.id ?? null; autoFit = true; render(); return; }
  const baseline = clone(draft); const id = current.id;
  converting = true;
  try {
    const preview = await api<Preview>('/api/preview', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(baseline) });
    if (!active() || selected !== id || !equal(baseline, draft)) { notice('The draft changed. Choose Position elements freely again.'); return; }
    const report = preview.components.find(report => report.id === id);
    const replacement = positioned(current, report?.blocks ?? []);
    change(layout => { layout.components[layout.components.findIndex(c => c.id === id)] = replacement; });
    contentMode = true; selectedElement = current.blocks[0]?.id ?? null; autoFit = true; render();
  } catch (error) { notice(String(error), true); }
  finally { converting = false; if (selected === id && active()) renderInspector(); }
}
function leaveContents() { if (designSession) { void closeDesigner(); return; } if (drag) endDrag(); contentMode = false; selectedElement = null; autoFit = true; render(); }
function useFlow() {
  const id = selected; if (!id) return;
  change(layout => { const c = layout.components.find(c => c.id === id)!; c.content_layout = 'flow'; for (const block of c.blocks) delete block.rect; });
  contentMode = false; selectedElement = null; autoFit = true; render();
}
function queuePreview() {
  window.clearTimeout(previewTimer);
  // Invalidate in-flight responses immediately, before the debounce elapses.
  ++previewSequence;
  previewAbort?.abort();
  previewTimer = window.setTimeout(() => { void preview(); }, 120);
}
async function preview() {
  const sequence = ++previewSequence;
  previewAbort?.abort();
  previewAbort = new AbortController();
  try {
    if (designSession) {
      const current=component()!;
      if (!current.blocks.length) { designSession.ready=false; designSession.error=''; render(false); return; }
      if (designSession.automatic && designSession.needsFit) {
        designSession.ready=false;
        const measurable=clone(current); if(!measurable.label?.trim())delete measurable.label;
        const fitted=await api<{component:Component}>('/api/component-fit',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({component:measurable,width:designSession.fixedWidth??null}),signal:previewAbort.signal});
        if (sequence!==previewSequence || !designSession) return;
        const explicitFit=!!designSession.recordFit;
        if(designSession.recordFit&&!equal(designSession.recordFit.layout.components[0],fitted.component))remember(designSession.recordFit);
        designSession.recordFit=undefined;designSession.needsFit=false; fitted.component.label=current.label; draft.components[0]=fitted.component; draft.display={id:'component-design',width:fitted.component.rect.width,height:fitted.component.rect.height};
        if(explicitFit)renderInspector();
      }
    }
    const previewLayout=clone(draft); if(designSession&&!previewLayout.components[0]?.label?.trim())delete previewLayout.components[0]!.label;
    const result = await api<Preview>('/api/preview', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(previewLayout), signal: previewAbort.signal });
    if (sequence !== previewSequence || !editing) return;
    draftImage = `data:image/png;base64,${result.png_base64}`;
    reports = result.components;
    if (viewingDraft) $<HTMLImageElement>('frame').src = draftImage;
    if (designSession) { designSession.ready=true; designSession.error=''; render(false); }
    renderReport();
  } catch (error) {
    if (sequence === previewSequence && !(error instanceof DOMException && error.name === 'AbortError')) { if (designSession) { designSession.ready=false; designSession.error=String(error); render(false); } else notice(String(error), true); }
  }
}
function renderReport() {
  const report = reports.find(r => r.id === selected);
  const target = $('render-report');
  if (!target) return;
  const issues = report?.warnings.filter(w=>w!=='overflow').map(w => w.replaceAll('_', ' ')) ?? [];
  if(report?.warnings.includes('overflow'))issues.push(element()&&report.blocks.find(block=>block.id===selectedElement)?.overflow?'This item is clipped. Enlarge its box in Position, or reduce its text size in Style.':`Clipped items: ${clippedItemNames()}.`);
  if (report?.state === 'stale') issues.unshift('Last good data is shown with a stale indicator.');
  if (report?.state === 'unavailable') issues.unshift('The selected data is unavailable.');
  target.textContent = issues.length ? [...new Set(issues)].join(' · ') : 'Rendered at fixed font sizes. Resize to reflow the content.';
}
function render(rebuildInspector = true) {
  const isDirty = dirty();
  if (!designSession) { button('refresh').hidden=false; button('designs-main').hidden=false; }
  button('edit').hidden = editing;
  button('edit').disabled = !connected;
  $('draft-badge').hidden = !editing;
  button('add-open').hidden = !active();
  button('device-undo').disabled = !active() || !undo.length;
  button('device-redo').disabled = !active() || !redo.length;
  button('discard').hidden = !editing;
  button('save').hidden = !editing;
  button('save').disabled = !isDirty || saving || conflict || !connected;
  button('discard').disabled = saving;
  button('refresh').disabled = !connected || saving;
  $('inspector-body').inert = saving;
  $('save-state').textContent = saving ? 'Saving…' : conflict ? 'Newer layout available' : isDirty ? '● Unsaved changes' : `Saved · revision ${saved.revision}`;
  $('save-state').classList.toggle('dirty-dot', isDirty);
  button('view-draft').disabled = !editing;
  for (const [id, yes] of [['view-current', !viewingDraft], ['view-draft', viewingDraft]] as const) {
    button(id).classList.toggle('active', yes);
    button(id).setAttribute('aria-pressed', String(yes));
  }
  button('undo').disabled = !active() || !undo.length;
  button('redo').disabled = !active() || !redo.length;
  $('add-tools').hidden = !active();
  button('display-settings').disabled = !connected || saving;
  $('add-title').textContent=designSession?'Add an item':'Add a component';
  $('add-target').textContent = designSession?'Choose what this component should show.':editingContents() ? 'Add an element inside this component.' : 'Choose what to put on your display.';
  $('placement-note').textContent = designSession?'Items stack automatically. Arrange freely when you want to move them yourself.':editingContents() ? 'Elements each own a box. Padding and data status stay outside their space.' : 'Components sit side by side. Each one owns its space on the display.';
  $('library-title').textContent = editingContents() ? 'Elements' : 'Components';
  button('add-open').textContent = designSession?'Add item':editingContents() ? 'Add element' : 'Add component';
  $('canvas-title').textContent = editingContents() ? `Content · ${name(component()!)}` : viewingDraft ? 'Draft preview' : 'Last completed frame';
  $('canvas-caption').textContent = designSession?'Your component at the preview zoom shown.':editingContents() ? 'Element boxes reflow text at fixed font sizes. Data status stays reserved.' : viewingDraft ? 'Draft preview · Drag to move, corner to resize.' : 'The last completed display frame.';
  $('content-mode-bar').hidden = !editingContents();
  $('design-empty').hidden = !designSession || !!component()?.blocks.length;
  $<HTMLImageElement>('frame').hidden = !!designSession && !component()?.blocks.length;
  $('revision-label').textContent = `Displayed ${saved.displayed_revision} / saved ${saved.revision}`;
  $('data-mode').textContent = saved.connector_execution ? (saved.fixture_data ? 'Monochrome · Connectors + fixtures' : 'Monochrome · Connector data') : 'Monochrome · Sample data';
  const view = editingContents() ? component()!.rect : draft.display;
  $('dimensions-caption').textContent = `${draft.display.width} × ${draft.display.height} px${designSession ? ' component' : ''} · `;
  $('dimensions').textContent = `${view.width} × ${view.height} px${editingContents() ? ' · component' : ''}`;
  $('keyboard-hint').textContent = active() ? 'Arrow keys: move · Shift: 8 px · Ctrl/Cmd Z: undo · Ctrl/Cmd S: save' : 'Paperphase 0.1.0 · Experimental';
  $('grid-overlay').style.display = viewingDraft && checked('grid') ? 'block' : 'none';
  if (viewingDraft && draftImage) $<HTMLImageElement>('frame').src = draftImage;
  if (!viewingDraft) setDisplayedImage();
  renderRuntimeWarning();
  renderList();
  renderTemplates();
  renderOverlays();
  if (rebuildInspector) { renderInspector(); if (currentPage === 'device') retainFocus($('device-settings-body'), () => renderSettings($('device-settings-body'))); }
  if (designSession) renderDesignerState(); else button('design-place').hidden=true;
  fit();
}
function setDisplayedImage(force = false) {
  const image = $<HTMLImageElement>('frame');
  const path = `/api/display.png?frame=${encodeURIComponent(saved.frame_id)}`;
  if (force || image.getAttribute('src') !== path) image.src = path;
}
function node<K extends keyof HTMLElementTagNameMap>(tag: K, className = '', content = '') {
  const element = document.createElement(tag);
  element.className = className;
  element.textContent = content;
  return element;
}
function renderList() { retainFocus($('component-list'), buildComponentList); }
function buildComponentList() {
  const list = $('component-list');
  list.replaceChildren();
  if (editingContents()) {
    const c = component()!; $('component-count').textContent = String(c.blocks.length);
    for (const block of c.blocks) {
      const item = node('button', `component-item ${selectedElement === block.id ? 'selected' : ''}`);
      item.setAttribute('aria-label', `Select element ${elementName(block)}`); item.dataset.element = block.id;
      item.append(node('span', 'component-icon', block.source.kind === 'clock' ? '◷' : block.source.kind === 'icon' ? '▧' : block.source.kind === 'field' ? '≋' : 'Aa'));
      const text = node('span'); text.append(node('span', 'item-name', elementName(block)));
      if (block.rect) text.append(node('small', '', `${block.rect.width} × ${block.rect.height} · ${block.rect.x}, ${block.rect.y}`));
      item.append(text); item.disabled = !active(); item.onclick = () => { selectedElement = block.id; render(); }; list.append(item);
    }
    return;
  }
  const layout = viewingDraft ? draft : saved.layout;
  $('component-count').textContent = String(layout.components.length);
  for (const c of layout.components) {
    const item = node('button', `component-item ${active() && selected === c.id ? 'selected' : ''}`);
    item.setAttribute('aria-label', `Select ${name(c)}`); item.dataset.component = c.id;
    item.append(node('span', 'component-icon', c.blocks.some(b => b.source.kind === 'clock') ? '◷' : c.blocks.some(b => b.source.kind === 'field') ? '≋' : 'Aa'));
    const text = node('span');
    text.append(node('span', 'item-name', name(c)), node('small', '', `${c.rect.width} × ${c.rect.height} · ${c.rect.x}, ${c.rect.y}`));
    item.append(text);
    item.disabled = !active();
    item.onclick = () => { selected = c.id; contentMode = false; selectedElement = null; render(); };
    list.append(item);
  }
}
function renderRuntimeWarning() {
  const configured = connectionStore.connections.filter(connection => connection.enabled && (connectorDefinitions.find(definition => definition.id === connection.connector_id)?.templates?.length ?? 0) > 0);
  const missing = configured.filter(connection => connection.fetch?.status === 'settings_only');
  const messages: string[] = [];
  if (missing.length) messages.push('Some configured plugins are not loaded. Components can be arranged, but new data cannot be fetched. Open Data sources for setup.');
  if (configured.length && saved.clock_fixed) messages.push('The host clock is fixed for testing. Restart without --now for live data updates.');
  $('runtime-warning').hidden = !messages.length;
  $('runtime-warning').textContent = messages.join(' ');
}

function renderTemplates() {
  const parent = $('plugin-templates'); parent.replaceChildren();
  parent.hidden = !active() || editingContents();
  if (parent.hidden) return;
  const available = connectionStore.connections.flatMap(connection => {
    const definition = connectorDefinitions.find(definition => definition.id === connection.connector_id);
    return (definition?.templates ?? []).map(template => ({ connection, template }));
  });
  // A source without supplied templates still supports a blank custom design.
  parent.append(node('h3', '', 'From your data sources'));
  const groups = new Map<string, HTMLElement>();
  for (const { connection, template } of available) {
    const item = node('button', 'template plugin-template');
    item.setAttribute('aria-label', `Add ${template.name} from ${connection.label}`);
    const unloaded = connection.fetch?.status === 'settings_only';
    const caption = node('span'); caption.append(node('strong', '', template.name), node('small', '', `${connection.label}${unloaded ? ' · plugin not loaded' : ''}`));
    if (unloaded) item.title = 'The template can be edited, but live fetching needs its plugin loaded. Open Data sources for setup.';
    item.append(icon('data'), caption, icon('plus'));
    item.onclick = () => {
      const component = clone(template.component); component.id = identifier('component'); component.label = `${connection.label} · ${template.name}`;
      for (const block of component.blocks) { const binding = sourceBinding(block.source); if (binding) binding.connection_id = connection.id; }
      for (let y = 16; y + component.rect.height <= draft.display.height; y += 8) {
        for (let x = 16; x + component.rect.width <= draft.display.width; x += 8) {
          component.rect.x = x; component.rect.y = y;
          if (validRect(component.rect, component.id)) {
            selected = component.id; inspectorTab = 'content';
            change(layout => { layout.components.push(component); }); closePicker(); return;
          }
        }
      }
      startDesigner(connection.connector_id,connection.id,compactDesign(component));
      if (designSession) { designSession.automatic=true; designSession.needsFit=true; queuePreview(); }
      notice('The original template does not fit here. Customise this compact copy, then add it when ready.');
    };
    let group = groups.get(connection.id);
    if (!group) {
      const details = node('details', 'template-group'); details.open = true;
      details.append(node('summary', '', `${connection.label}${connection.enabled ? '' : ' (paused)'}`));
      group = node('div'); details.append(group); parent.append(details); groups.set(connection.id, group);
    }
    group.append(item);
  }
  for (const connection of connectionStore.connections) {
    let group=groups.get(connection.id);
    if (!group) {const details=node('details','template-group');details.open=true;details.append(node('summary','',connection.label));group=node('div');details.append(group);parent.append(details);groups.set(connection.id,group);}
    for (const design of designLibrary.designs.filter(design=>design.connector_id===connection.connector_id)) {
      action(design.name,`Use design ${design.name} from ${connection.label}`,group,()=>useSavedDesign(design,connection.id)).className='template';
    }
    action('Create custom design',`Design a component for ${connection.label}`,group,()=>startDesigner(connection.connector_id,connection.id)).className='wide-button';
  }

}

function renderOverlays() { retainFocus($('overlays'), buildOverlays); }
function buildOverlays() {
  const overlays = $('overlays');
  overlays.replaceChildren();
  if (!active()) return;
  if (editingContents()) { renderElementOverlays(overlays); return; }
  for (const c of draft.components) {
    const box = node('div', `component-box ${selected === c.id ? 'selected' : ''} ${drag?.id === c.id && drag.invalid ? 'invalid' : ''}`);
    box.dataset.component = c.id;
    box.setAttribute('role', 'button');
    box.setAttribute('aria-pressed', String(selected === c.id));
    box.setAttribute('aria-label', `Move ${name(c)}`);
    box.tabIndex = 0;
    const rect = drag?.id === c.id && drag.invalid ? proposedRect(drag, lastPointerX, lastPointerY) : c.rect;
    Object.assign(box.style, { left: `${rect.x}px`, top: `${rect.y}px`, width: `${rect.width}px`, height: `${rect.height}px` });
    if (selected === c.id) {
      const tag = node('span', 'component-tag', name(c));
      // Keep the label inside the board when the component touches its top edge.
      if (c.rect.y < 22) tag.style.top = '1px';
      const handle = node('span', 'resize-handle');
      handle.dataset.resize = 'true';
      handle.setAttribute('aria-hidden', 'true');
      box.append(tag,handle);
    }
    box.onfocus = () => {
      if (selected !== c.id) {
        selected = c.id; renderInspector(); renderList(); renderOverlays();
        for (const next of overlays.querySelectorAll<HTMLElement>('[data-component]')) if (next.dataset.component === c.id) next.focus();
      }
    };
    overlays.append(box);
  }
}
function fit() {
  if (!draft) return;
  const viewport = $('viewport');
  const view = editingContents() ? component()!.rect : { x: 0, y: 0, width: draft.display.width, height: draft.display.height };
  if (autoFit) zoom = Math.min(designSession ? 4 : editingContents() ? 2 : 1, (viewport.clientWidth - 24) / view.width, (viewport.clientHeight - 24) / view.height);
  zoom = Math.max(.1, Math.min(designSession ? 4 : 2, zoom));
  Object.assign($('board').style, { width: `${view.width}px`, height: `${view.height}px`, transform: `scale(${zoom})` });
  Object.assign($('frame').style, { width: `${draft.display.width}px`, height: `${draft.display.height}px`, position: 'absolute', left: `${-view.x}px`, top: `${-view.y}px` });
  Object.assign($('board-wrap').style, { width: `${view.width * zoom}px`, height: `${view.height * zoom}px` });
  const pad = editingContents() ? component()!.padding : 0;
  $('grid-overlay').style.backgroundPosition = `${pad}px ${pad}px`;
  $('zoom-label').textContent = `${Math.round(zoom * 100)}%`;
}

function renderElementOverlays(overlays: HTMLElement) {
  const c = component()!; const bounds = contentBounds(c);
  const area = node('div', 'content-boundary');
  Object.assign(area.style, { left: `${c.padding}px`, top: `${c.padding}px`, width: `${bounds.width}px`, height: `${bounds.height}px` }); overlays.append(area);
  if (c.blocks.some(block => sourceBinding(block.source))) {
    const band = node('div', 'reserved-area');
    Object.assign(band.style, { left: `${c.padding}px`, top: `${c.padding + bounds.height}px`, width: `${bounds.width}px`, height: '18px' });
    band.setAttribute('aria-label', 'Reserved data status area'); overlays.append(band);
  }
  for (const block of c.blocks) {
    const measured=block.rect ?? (designSession ? reports.find(report=>report.id===c.id)?.blocks.find(item=>item.id===block.id)?.rect : null);
    if (!measured) continue;
    const box = node('div', `element-box ${selectedElement === block.id ? 'selected' : ''} ${drag?.blockId === block.id && drag.invalid ? 'invalid' : ''}`);
    box.dataset.element = block.id; box.tabIndex = 0; box.setAttribute('role', 'button'); box.setAttribute('aria-label', `Move element ${elementName(block)}`); box.setAttribute('aria-pressed', String(selectedElement === block.id));
    const rect = drag?.blockId === block.id && drag.invalid ? proposedRect(drag, lastPointerX, lastPointerY) : measured;
    Object.assign(box.style, { left: `${c.padding + rect.x}px`, top: `${c.padding + rect.y}px`, width: `${rect.width}px`, height: `${rect.height}px` });
    if (selectedElement === block.id) {
      const tag = node('span', 'element-tag', elementName(block)); if (c.padding + rect.y < 22) tag.style.top = '1px';
      const handle = node('span', 'resize-handle'); handle.dataset.resize = 'true'; handle.setAttribute('aria-hidden', 'true'); box.append(tag); if (c.content_layout==='positioned') box.append(handle);
    }
    box.onfocus = () => {
      if (selectedElement !== block.id) { selectedElement = block.id; renderInspector(); renderList(); renderOverlays();
        for (const next of overlays.querySelectorAll<HTMLElement>('[data-element]')) if (next.dataset.element === block.id) next.focus();
      }
    };
    overlays.append(box);
  }
}

function label(title: string, input: HTMLElement, parent: HTMLElement) {
  // During blur-triggered change events the browser may already report BODY as
  // active. Keep the initiating control until its synchronous redraw finishes.
  const onChange = input.onchange;
  if (onChange) input.onchange = event => {
    const previous = changingControl; changingControl = input;
    try { onChange.call(input, event); } finally { changingControl = previous; }
  };
  input.setAttribute('aria-label', title);
  const element = node('label');
  element.append(node('span', '', title), input);
  parent.append(element);
  return input;
}
function numeric(title: string, value: number, min: number, max: number, parent: HTMLElement, update: (value: number) => void) {
  const input = node('input');
  input.type = 'number'; input.value = String(value); input.min = String(min); input.max = String(max); input.step = '1';
  input.onchange = () => {
    const n = input.valueAsNumber;
    if (Number.isInteger(n) && n >= min && n <= max) update(n);
    else { input.value = String(value); notice(`${title} must be a whole number from ${min} to ${max}.`, true); }
  };
  return label(title, input, parent);
}
function select(title: string, value: string, options: [string, string][], parent: HTMLElement, update: (value: string) => void) {
  const input = node('select');
  // Preserve bindings to a missing source, so opening the inspector never rewrites a layout.
  if (!options.some(([id]) => id === value)) options = [[value, `${value} (unavailable)`], ...options];
  for (const [id, text] of options) { const option = node('option', '', text); option.value = id; input.append(option); }
  input.value = value; input.onchange = () => update(input.value);
  return label(title, input, parent);
}
function textField(title: string, value: string, parent: HTMLElement, update: (value: string) => void, multiline = false, maxLength = 128) {
  const input = multiline ? node('textarea') : node('input');
  input.value = value; input.maxLength = maxLength;
  input.oninput = () => update(input.value);
  return label(title, input, parent);
}
function action(text: string, title: string, parent: HTMLElement, callback: () => void, disabled = false) {
  const input = node('button', 'small', text);
  input.type = 'button'; input.title = title; input.setAttribute('aria-label', title); input.disabled = disabled; input.onclick = callback;
  parent.append(input);
  return input;
}
function section(title: string, parent: HTMLElement) {
  const result = node('section', 'property-section'); result.append(node('h3', '', title)); parent.append(result); return result;
}
function renderInspector() { retainFocus($('inspector-body'), buildInspector); }
function buildInspector() {
  const inspector = $('inspector-body');
  const inspected = editingContents() ? `${selected}/${selectedElement}` : selected;
  if (lastInspectedId !== inspected) {
    inspector.parentElement!.scrollTop = 0; inspectorTab = 'content';
    lastInspectedId = inspected;
  }
  inspector.replaceChildren();
  const c = component();
  if (!active() || !c) {
    const empty = node('div', 'empty-properties');
    empty.append(node('h3', '', active() ? 'Start with a component.' : editing ? 'Viewing your display.' : 'Your display, your way.'), node('p', '', active() ? 'Select one on the canvas or in the strip below it. Choose Add component for text, time and connected data.' : editing ? 'Switch to Draft to continue editing. Your changes are kept.' : 'Choose Edit layout to arrange components and customise their content.'));
    inspector.append(empty); return;
  }
  const id = c.id;
  const updateComponent = (fn: (c: Component) => void, key = '', rebuild = true) => change(l => fn(l.components.find(c => c.id === id)!), key, rebuild);
  if (editingContents() && !element() && !designSession) {
    const empty = node('div', 'empty-properties'); empty.append(node('h3', '', 'Choose an element.'), node('p', '', 'Choose text, information or a clock on the canvas or in the strip below. Use Add element to include more.')); inspector.append(empty); return;
  }
  inspector.append(node('div', 'inspector-heading', ''));
  inspector.firstElementChild!.append(node('h2', '', editingContents() && element() ? elementName(element()!) : name(c)));
  const tabs = node('div', 'inspector-tabs'); tabs.setAttribute('role', 'tablist'); tabs.setAttribute('aria-label', 'Component properties'); inspector.append(tabs);
  const panes = new Map<string, HTMLElement>();
  for (const [value, title] of [['content', 'Content'], ['position', 'Position'], ['style', 'Style']]) {
    const tab = node('button', '', title); tab.type = 'button'; tab.id = `inspector-tab-${value}`;
    tab.setAttribute('role', 'tab'); tab.setAttribute('aria-selected', String(inspectorTab === value)); tab.setAttribute('aria-controls', `inspector-pane-${value}`); tab.tabIndex = inspectorTab === value ? 0 : -1;
    tab.onclick = () => { inspectorTab = value!; renderInspector(); button(`inspector-tab-${value}`).focus(); };
    tab.onkeydown = event => { if (['ArrowLeft','ArrowRight','Home','End'].includes(event.key)) { event.preventDefault(); const order = ['content','position','style']; inspectorTab = event.key === 'Home' ? 'content' : event.key === 'End' ? 'style' : order[(order.indexOf(value!) + (event.key === 'ArrowRight' ? 1 : 2)) % 3]!; renderInspector(); button(`inspector-tab-${inspectorTab}`).focus(); } };
    tabs.append(tab);
    const pane = node('div'); pane.id = `inspector-pane-${value}`; pane.setAttribute('role', 'tabpanel'); pane.setAttribute('aria-labelledby', tab.id); pane.hidden = inspectorTab !== value; panes.set(value!, pane); inspector.append(pane);
  }
  const positionPane = panes.get('position')!; const stylePane = panes.get('style')!;
  if (designSession) {
    const sizing=section('Design dimensions',positionPane); const sizeFields=node('div','field-grid'); sizing.append(sizeFields);
    for(const key of ['width','height'] as const) numeric(key==='width'?'Width':'Height',c.rect[key],32,4096,sizeFields,value=>{designSession!.automatic=false;updateComponent(component=>{component.rect[key]=value;});});
    const spacing=section('Component spacing',stylePane); const fields=node('div','field-grid');spacing.append(fields);
    numeric('Padding',c.padding,0,64,fields,value=>updateComponent(component=>{component.padding=value;}));
    numeric('Item gap',c.gap,0,64,fields,value=>updateComponent(component=>{component.gap=value;}));
    const border=node('input');border.type='checkbox';border.checked=c.border;border.onchange=()=>updateComponent(component=>{component.border=border.checked;});
    const borderLabel=node('label','check');borderLabel.append(border,document.createTextNode('Draw component border'));spacing.append(borderLabel);
    if(!element()) panes.get('content')!.append(node('p','property-section subtle','Select an item on the preview or in the strip below to change its content.'));
  }
  if (!editingContents()) {
    const placement = section('Position & size', positionPane);
    const geometry = node('div', 'field-grid'); placement.append(geometry);
    for (const [key, title, min, max] of [['x', 'X', 0, draft.display.width - c.rect.width], ['y', 'Y', 0, draft.display.height - c.rect.height], ['width', 'Width', 32, draft.display.width], ['height', 'Height', 32, draft.display.height]] as const) {
      numeric(title, c.rect[key], min, max, geometry, n => updateComponent(c => { c.rect[key] = n; }));
    }
    const appearance = section('Spacing & border', stylePane);
    const spacing = node('div', 'field-grid'); appearance.append(spacing);
    numeric('Padding', c.padding, 0, 64, spacing, n => updateComponent(c => { c.padding = n; }));
    numeric('Block gap', c.gap, 0, 64, spacing, n => updateComponent(c => { c.gap = n; }));
    const borderLabel = node('label', 'check'); const border = node('input'); border.type = 'checkbox'; border.checked = c.border;
    border.onchange = () => updateComponent(c => { c.border = border.checked; }); borderLabel.append(border, document.createTextNode('Draw component border')); appearance.append(borderLabel);
    const advanced = node('details', 'advanced'); advanced.append(node('summary', '', 'Advanced: display updates')); stylePane.append(advanced);
    const refresh = section('Refresh preference', advanced);
    select('Refresh request', c.refresh_hint ?? 'automatic', [['automatic', 'Automatic'], ['partial', 'Prefer partial'], ['full', 'Request full']], refresh, value => updateComponent(c => { c.refresh_hint = value as Component['refresh_hint']; }));
    refresh.append(node('p', 'render-report', 'The host may require a full refresh to respect display limits.'));
    const layout = section('Element layout', positionPane);
    select('Content layout', c.content_layout ?? 'flow', [['flow', 'Vertical flow'], ['positioned', 'Positioned elements']], layout, value => { if (value === 'flow') useFlow(); else void enterContents(); });
    layout.append(node('p', 'render-report', 'Free positioning converts the current flow into individual element boxes. You can undo this change.'));
    action(c.content_layout === 'positioned' ? 'Edit individual elements' : 'Position elements freely', c.content_layout === 'positioned' ? 'Edit individual elements' : 'Position elements freely', layout, () => { void enterContents(); }, converting).className = 'wide-button';
  }
  const content = section(editingContents() ? 'Selected element' : 'Component content', panes.get('content')!);
  if (!editingContents()) action('Design this component', 'Design this component', content, () => { void designExisting(c); }).className='wide-button';
  if (!editingContents()) { const identity = section('Component name', stylePane); textField('Component name', c.label ?? '', identity, value => updateComponent(c => { c.label = value.trim() ? value : undefined; }, `${id}.label`, false), false, 96); }
  c.blocks.forEach((block, index) => {
    if (editingContents() && block.id !== selectedElement) return;
    const updateBlock = (fn: (b: Block) => void, key = '', rebuild = true) => updateComponent(c => fn(c.blocks.find(b => b.id === block.id)!), key, rebuild);
    const card = node('div', 'block-card'); card.dataset.block = block.id; content.append(card);
    const heading = node('div', 'block-heading'); heading.append(node('strong', '', `${block.source.kind === 'literal' ? 'Text' : block.source.kind === 'field' ? 'Data value' : block.source.kind === 'clock' ? 'Time & date' : 'Icon'} ${index + 1}`)); card.append(heading);
    const move = (direction: number) => updateComponent(c => { const i = c.blocks.findIndex(b => b.id === block.id); [c.blocks[i], c.blocks[i + direction]] = [c.blocks[i + direction]!, c.blocks[i]!]; });
    if (c.content_layout !== 'positioned') {
      action('', 'Move block up', heading, () => move(-1), index === 0).append(icon('up'));
      action('', 'Move block down', heading, () => move(1), index === c.blocks.length - 1).append(icon('down'));
    }
    action('', editingContents() ? 'Remove element' : 'Remove block', heading, () => updateComponent(c => { c.blocks = c.blocks.filter(b => b.id !== block.id); }), c.blocks.length === 1).append(icon('close'));
    if (editingContents() && block.rect) {
      const geometry = node('div', 'field-grid'); positionPane.append(geometry); const bounds = contentBounds(c);
      for (const [key, title, min, max] of [['x', 'Element X', 0, bounds.width - block.rect.width], ['y', 'Element Y', 0, bounds.height - block.rect.height], ['width', 'Element width', 8, bounds.width], ['height', 'Element height', 8, bounds.height]] as const) {
        numeric(title, block.rect[key], min, max, geometry, value => updateBlock(b => { b.rect![key] = value; }));
      }
    }
    const sourceOptions = node('details', 'content-type'); sourceOptions.append(node('summary', '', 'Change content type')); card.append(sourceOptions);
    select('Content source', block.source.kind, [['literal', 'Text'], ['field', 'Dataset field'], ['clock', 'Time & date'], ['icon', 'Icon']], sourceOptions, kind => updateBlock(b => {
      if (kind === 'literal') b.source = { kind: 'literal', text: 'Your text' };
      else if (kind === 'clock') b.source = { kind: 'clock', clock: newClock('time') };
      else if (kind === 'icon') b.source = { kind: 'icon', pack_id: iconPacks[0]?.id ?? 'missing', icon: { kind: 'literal', icon_id: iconPacks[0]?.icons[0] ?? 'unknown' }, size: 48 };
      else if (bindingCatalog()[0]) b.source = { kind: 'field', binding: newBinding(bindingCatalog()[0]!) };
    }));
    if (block.source.kind === 'literal') {
      textField('Text', block.source.text, card, value => updateBlock(b => { if (b.source.kind === 'literal') b.source.text = value; }, `${id}.${block.id}.text`, false), true, 4096);
    } else if (block.source.kind === 'clock') {
      const clock = block.source.clock;
      const updateClock = (fn: (clock: ClockSpec) => void) => updateBlock(b => { if (b.source.kind === 'clock') fn(b.source.clock); });
      select('Show', clock.content, [['time', 'Time'], ['date', 'Date'], ['date_time', 'Time and date']], card, value => updateClock(c => { c.content = value as ClockSpec['content']; }));
      select('Timezone', clock.timezone, zones.map(zone => [zone, zone]), card, value => updateClock(c => { c.timezone = value; }));
      if (clock.content !== 'date') select('Time format', clock.hour_cycle, [['twenty_four', '24-hour'], ['twelve', '12-hour']], card, value => updateClock(c => { c.hour_cycle = value as ClockSpec['hour_cycle']; }));
      if (clock.content !== 'time') select('Date format', clock.date_format, [['long', 'Tuesday, 10 November 2026'], ['short', '10 Nov 2026'], ['iso', '2026-11-10'], ['weekday', 'Tuesday']], card, value => updateClock(c => { c.date_format = value as ClockSpec['date_format']; }));
    } else if (block.source.kind === 'icon') {
      const source = block.source;
      const updateIcon = (fn: (source: Extract<Block['source'], {kind: 'icon'}>) => void) => updateBlock(b => { if (b.source.kind === 'icon') fn(b.source); });
      const pack = iconPacks.find(pack => pack.id === source.pack_id);
      select('Icon pack', source.pack_id, iconPacks.map(pack => [pack.id, pack.name]), card, value => updateIcon(s => { s.pack_id = value; if (s.icon.kind === 'literal') s.icon.icon_id = iconPacks.find(p => p.id === value)!.icons[0]!; }));
      if (!pack) card.append(node('p', 'render-report', 'Icon pack unavailable. Load its plugin asset manifest and restart the service.'));
      numeric('Icon size (px)', source.size, 8, 256, card, value => updateIcon(s => { s.size = value; }));
      const iconDatasets=bindingCatalog().filter(dataset=>dataset.fields.some(field=>field.kind==='text'));
      const iconSelection=select('Icon selection', source.icon.kind, [['literal', 'Choose an icon'], ['field', 'Dataset field']], card, value => updateIcon(s => {
        if (value === 'literal') s.icon = { kind: 'literal', icon_id: pack?.icons[0] ?? 'unknown' };
        else if (iconDatasets[0]) {const dataset=iconDatasets.find(dataset=>dataset.fields.some(field=>/^icon(?:[-_](?:key|id))?$/.test(field.id)))??iconDatasets[0]!;s.icon = { kind: 'field', binding: newIconBinding(dataset) };}
      }));
      iconSelection.querySelector<HTMLOptionElement>('option[value=field]')!.disabled=!iconDatasets.length;
      if(!iconDatasets.length)card.append(node('p','render-report','No text fields are available for this design. Choose a plugin-backed design with an information source, or choose an icon here.'));
      if (source.icon.kind === 'literal') {
        select('Icon', source.icon.icon_id, pack?.icons.map(id => [id, id.replaceAll('_', ' ')]) ?? [], card, value => updateIcon(s => { s.icon = { kind: 'literal', icon_id: value }; }));
      } else {
        const binding = source.icon.binding;
        const key = `${binding.connection_id}/${binding.dataset_id}`;
        const dataset = bindingCatalog().find(d => `${d.connection_id}/${d.dataset_id}` === key);
        select(designSession ? 'Information group' : 'Dataset', key, bindingCatalog().map(d => [`${d.connection_id}/${d.dataset_id}`, designSession ? friendlyField(d.dataset_id) : `${d.connection_label ?? d.connection_id} / ${d.dataset_id}`]), card, value => updateIcon(s => { s.icon = { kind: 'field', binding: newIconBinding(bindingCatalog().find(d => `${d.connection_id}/${d.dataset_id}` === value)!) }; }));
        select(designSession ? 'Show information' : 'Field', binding.field_id, dataset?.fields.filter(f => f.kind === 'text').map(f => [f.id, designSession?friendlyField(f.id):f.id]) ?? [], card, value => updateIcon(s => { if (s.icon.kind === 'field') s.icon.binding.field_id = value; }));
        card.append(node('p','render-report','This field must contain an icon name from the selected pack. For weather, use Icon key.'));
        numeric(designSession?'Item number':'Row (first row = 0)', (binding.row??0)+(designSession?1:0), designSession?1:0, designSession?512:511, card, value => updateIcon(s => { if (s.icon.kind === 'field') s.icon.binding.row = value-(designSession?1:0); }));
      }
    } else {
      const binding = block.source.binding;
      const updateBinding = (fn: (b: Binding) => void, key = '', rebuild = true) => updateBlock(b => { if (b.source.kind === 'field') fn(b.source.binding); }, key, rebuild);
      const key = `${binding.connection_id}/${binding.dataset_id}`;
      const dataset = bindingCatalog().find(d => `${d.connection_id}/${d.dataset_id}` === key);
      select(designSession ? 'Information group' : 'Dataset', key, bindingCatalog().map(d => [`${d.connection_id}/${d.dataset_id}`, designSession ? friendlyField(d.dataset_id) : `${d.connection_label ?? d.connection_id} / ${d.dataset_id}`]), card, value => {
        const d = bindingCatalog().find(d => `${d.connection_id}/${d.dataset_id}` === value)!;
        updateBlock(b => { b.source = { kind: 'field', binding: newBinding(d) }; });
      });
      select(designSession ? 'Show information' : 'Field', binding.field_id, dataset?.fields.map(f => [f.id, designSession ? friendlyField(f.id) : `${f.id} · ${f.kind}`]) ?? [], card, value => updateBinding(b => { b.field_id = value; b.decimals = null; }));
      const format=designSession?node('details','advanced'):card; if(designSession){format.append(node('summary','','More formatting'));card.append(format);}
      numeric(designSession?'Item number':'Row (first row = 0)', (binding.row??0)+(designSession?1:0), designSession?1:0, designSession?512:511, format, n => updateBinding(b => { b.row = n-(designSession?1:0); }));
      if (dataset?.fields.find(f => f.id === binding.field_id)?.kind === 'number') {
        select('Decimal places', binding.decimals == null ? '' : String(binding.decimals), [['', 'Automatic'], ...Array.from({ length: 7 }, (_, i): [string, string] => [String(i), String(i)])], format, value => updateBinding(b => { b.decimals = value === '' ? null : Number(value); }));
      }
      textField(designSession?'Text before':'Prefix', binding.prefix ?? '', format, value => updateBinding(b => { b.prefix = value; }, `${id}.${block.id}.prefix`, false));
      textField(designSession?'Text after':'Suffix', binding.suffix ?? '', format, value => updateBinding(b => { b.suffix = value; }, `${id}.${block.id}.suffix`, false));
    }
    card.append(sourceOptions);
    const styles = section(elementName(block), stylePane); styles.dataset.block = block.id;
    const typography = node('div', 'field-grid'); styles.append(typography);
    if (block.source.kind !== 'icon') numeric('Font size (px)', block.font_size, 8, 256, typography, value => updateBlock(b => { b.font_size = value; }));
    select('Alignment', block.align ?? 'left', [['left', 'Left'], ['center', 'Centre'], ['right', 'Right']], typography, value => updateBlock(b => { b.align = value; }));
    if (block.source.kind !== 'icon') select('Overflow', block.overflow ?? 'wrap', [['wrap', 'Wrap'], ['clip', 'Clip'], ['ellipsis', 'Ellipsis']], styles, value => updateBlock(b => { b.overflow = value; }));
  });
  action(editingContents() ? '+ Add text element' : '+ Add text block', editingContents() ? 'Add text element' : 'Add text block', content, () => {
    if (designSession) addDesignBlock('text'); else if (c.content_layout === 'positioned') addElement('text'); else updateComponent(c => { c.blocks.push(newBlock()); });
  }, c.blocks.length >= 32).className = 'wide-button';
  const report = node('p', 'render-report'); report.id = 'render-report'; content.append(report); renderReport();
  if (!editingContents()) {
    const remove = section('Component actions', inspector);
    action('Remove component', 'Remove component', remove, () => { change(l => { l.components = l.components.filter(c => c.id !== id); }); selected = null; render(); }).className = 'danger wide-button';
  }
}
function newIconBinding(dataset: Dataset): Binding { const binding = newBinding(dataset); binding.field_id = dataset.fields.find(f => f.kind==='text'&&/^icon(?:[-_](?:key|id))?$/.test(f.id))?.id ?? dataset.fields.find(f => f.kind === 'text')?.id ?? binding.field_id; return binding; }
function newBinding(dataset: Dataset): Binding { return { connection_id: dataset.connection_id, dataset_id: dataset.dataset_id, field_id: dataset.fields[0]!.id, row: 0, decimals: null, prefix: '', suffix: '' }; }
// getRandomValues also works over plain HTTP on a LAN; randomUUID requires HTTPS.
function identifier(prefix: string) { return `${prefix}-${Array.from(crypto.getRandomValues(new Uint8Array(8)), n => n.toString(16).padStart(2, '0')).join('')}`; }
function newBlock(): Block { return { id: identifier('block'), source: { kind: 'literal', text: 'Your text' }, font_size: designSession ? 14 : 20, align: 'left', overflow: 'wrap' }; }
function newClock(content: ClockSpec['content']): ClockSpec { return { timezone: 'Europe/Dublin', content, hour_cycle: 'twenty_four', date_format: 'short' }; }
function renderSettings(parent: HTMLElement) {
  parent.replaceChildren();
  const panel = section('Refresh & quiet hours', parent);
  const toggle = node('label', 'check');
  const input = node('input'); input.type = 'checkbox'; input.checked = !!draftSettings.quiet_hours;
  input.onchange = () => change(() => { draftSettings.quiet_hours = input.checked ? { timezone: 'Europe/Dublin', start: '22:00', end: '07:00' } : null; });
  toggle.append(input, document.createTextNode('Enable quiet hours')); panel.append(toggle);
  panel.append(node('p', 'render-report', 'During quiet hours the displayed frame stays still. The clock continues; the latest saved layout is rendered when quiet hours end.'));
  const advanced = node('details', 'advanced'); advanced.append(node('summary', '', 'Advanced refresh policy')); panel.append(advanced);
  const policy = section('Refresh limits', advanced);
  policy.append(node('p', 'render-report', 'Software simulator only. These example limits are not validated for a physical panel. Profile limits always apply.'));
  const settings = draftSettings.policy;
  const profile = saved.refresh.profile;
  policy.append(node('p', 'render-report', `Simulator limits: at least ${profile.min_interval_ms} ms between completed updates and the next start, at most ${profile.max_partials} partial updates, ${profile.max_full_age_secs} seconds between full updates when content changes, and ${profile.max_partial_area_percent}% partial area.`));
  for (const [key, title, min, max] of [['min_interval_ms', 'Minimum refresh interval (ms)', 0, 3600000], ['full_after_partials', 'Full refresh after partial updates', 1, 1000], ['full_after_secs', 'Maintenance interval (seconds)', 1, 604800], ['max_partial_area_percent', 'Largest partial area (%)', 1, 100]] as const) {
    numeric(title, settings[key], min, max, policy, value => change(() => { draftSettings.policy[key] = value; }));
  }
  const quiet = draftSettings.quiet_hours;
  if (quiet) {
    select('Quiet hours timezone', quiet.timezone, zones.map(zone => [zone, zone]), panel, value => change(() => { draftSettings.quiet_hours!.timezone = value; }));
    for (const [key, title] of [['start', 'Pause at'], ['end', 'Resume at']] as const) {
      const time = node('input'); time.type = 'time'; time.value = quiet[key]; time.step = '60';
      time.onchange = () => change(() => { draftSettings.quiet_hours![key] = time.value; });
      label(title, time, panel);
    }
  }
  panel.append(advanced);
  panel.append(node('p', 'render-report', 'Use Save to apply these settings, or Discard to restore them. Refresh display also respects quiet hours.'));
}
function addComponent(kind: 'text' | 'data' | 'clock') {
  if (designSession) { addDesignBlock(kind); return; }
  if (editingContents()) { addElement(kind); return; }
  const block = newBlock();
  const blocks = [block];
  if (kind === 'clock') {
    block.source = { kind: 'clock', clock: newClock('time') }; block.font_size = 40;
    const date = newBlock(); date.source = { kind: 'clock', clock: newClock('date') }; date.font_size = 14; blocks.push(date);
  } else if (kind === 'data') {
    if (!bindingCatalog()[0]) { notice('No datasets are available. Start with a text component.'); return; }
    block.source = { kind: 'field', binding: newBinding(bindingCatalog()[0]!) };
  }
  const c: Component = { id: identifier('component'), rect: { x: 0, y: 0, width: 256, height: 128 }, padding: 12, gap: 8, border: false, blocks };
  for (let y = 16; y + c.rect.height <= draft.display.height; y += 8) {
    for (let x = 16; x + c.rect.width <= draft.display.width; x += 8) {
      c.rect.x = x; c.rect.y = y;
      if (validRect(c.rect, c.id)) { selected = c.id; change(l => { l.components.push(c); }); inspectorTab = 'content'; closePicker(); renderInspector(); return; }
    }
  }
  notice('No free 256 × 128 area is available. Resize or remove a component to make room.');
}

function addElement(kind: 'text' | 'data' | 'clock') {
  const current = component(); if (!current || current.content_layout !== 'positioned' || !active()) return;
  if (current.blocks.length >= 32) { notice('Each component can contain at most 32 elements.'); return; }
  const block = newBlock();
  if (kind === 'data') { if (!bindingCatalog()[0]) return; block.source = { kind: 'field', binding: newBinding(bindingCatalog()[0]!) }; }
  if (kind === 'clock') block.source = { kind: 'clock', clock: newClock('date_time') };
  const candidate = clone(current); candidate.blocks.push(block);
  const rect = freeElementRect(candidate, block.id, kind === 'clock' ? 60 : 30);
  if (!rect) { notice('No free element-sized area is available. Resize or move an existing element to make room.'); return; }
  block.rect = rect;
  if (!validElements(candidate)) { notice('Leave enough content space for every element and the data status strip.', true); return; }
  contentMode = true; selectedElement = block.id; autoFit = true;
  change(layout => { layout.components.find(c => c.id === current.id)!.blocks.push(block); }); closePicker();
}

let lastPointerX = 0;
let lastPointerY = 0;
function proposedRect(state: NonNullable<typeof drag>, x: number, y: number): Rect {
  const snap = (n: number) => Math.round(n / (checked('snap') ? 8 : 1)) * (checked('snap') ? 8 : 1);
  const dx = (x - state.startX) / zoom, dy = (y - state.startY) / zoom;
  const rect = { ...state.original };
  const minimum = state.blockId ? 8 : 32;
  if (state.resize) { rect.width = Math.max(minimum, snap(rect.width + dx)); rect.height = Math.max(minimum, snap(rect.height + dy)); }
  else { rect.x = snap(rect.x + dx); rect.y = snap(rect.y + dy); }
  return rect;
}
$('board').addEventListener('pointerdown', event => {
  if (!active() || event.button !== 0 || drag) return;
  const target = event.target as HTMLElement;
  const inside = editingContents();
  const box = target.closest<HTMLElement>(inside ? '[data-element]' : '[data-component]');
  if (!box) { if (inside) selectedElement = null; else selected = null; render(); return; }
  event.preventDefault();
  if (inside) selectedElement = box.dataset.element!;
  else selected = box.dataset.component!;

  const c = component()!;
  if (designSession && c.content_layout !== 'positioned') { render(); for (const next of $('overlays').querySelectorAll<HTMLElement>('[data-element]')) if (next.dataset.element===selectedElement) next.focus({preventScroll:true}); return; }
  const rect = inside ? element()!.rect! : c.rect;
  drag = { id: c.id, blockId: inside ? selectedElement! : undefined, startX: event.clientX, startY: event.clientY, original: { ...rect }, last: { ...rect }, baseline: capture(), resize: !!target.closest('[data-resize]'), pointerId: event.pointerId, invalid: false };
  lastPointerX = event.clientX; lastPointerY = event.clientY;
  $('board').setPointerCapture(event.pointerId);
  document.body.classList.add('dragging');
  coalescedKey = '';
  render(); $('viewport').focus({preventScroll:true});
});
$('board').addEventListener('pointermove', event => {
  if (!drag || event.pointerId !== drag.pointerId) return;
  lastPointerX = event.clientX; lastPointerY = event.clientY;
  const rect = proposedRect(drag, event.clientX, event.clientY);
  const c = component()!;
  drag.invalid = drag.blockId ? !validElement(rect, drag.blockId, c) : !validRect(rect, drag.id) || !validElements({ ...c, rect });
  if (!drag.invalid) {
    drag.last = rect;
    if (drag.blockId) c.blocks.find(block => block.id === drag!.blockId)!.rect = rect; else c.rect = rect;
    queuePreview();
  }
  renderOverlays();
});
function endDrag(cancel = false) {
  if (!drag) return;
  const state = drag;
  drag = undefined;
  if (cancel) restore(state.baseline);
  else {
    if (state.blockId) component()!.blocks.find(block => block.id === state.blockId)!.rect = state.last;
    else component()!.rect = state.last;
    if (!equal(state.baseline, capture())) remember(state.baseline);
    if (state.invalid) notice(state.blockId ? 'Items must stay inside the component’s content area. Restored the last valid position.' : 'Components must fit the display and their positioned contents. Restored the last valid position.');
  }
  if ($('board').hasPointerCapture(state.pointerId)) $('board').releasePointerCapture(state.pointerId);
  document.body.classList.remove('dragging');
  render(); queuePreview();
}
$('board').addEventListener('pointerup', () => endDrag());
$('board').addEventListener('pointercancel', () => endDrag(true));
$('board').addEventListener('lostpointercapture', () => endDrag(true));
function history(direction: 'undo' | 'redo') {
  if (!active() || drag) return;
  const from = direction === 'undo' ? undo : redo;
  const to = direction === 'undo' ? redo : undo;
  const layout = from.pop();
  if (!layout) return;
  to.push(capture()); restore(layout); coalescedKey = '';
  if (!component()) selected = null;
  normaliseSelection();
  notice(); render(); queuePreview();
}
async function save() {
  if (designSession) { await saveDesign(); return; }
  if (!dirty() || saving || conflict || !connected || drag) return;
  saving = true; render(false); notice();
  try {
    const result = await api<{ revision: number; phase: string }>('/api/layout', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ base_revision: saved.revision, layout: draft, refresh_settings: draftSettings }) });
    saved = { ...saved, revision: result.revision, layout: clone(draft), refresh_settings: clone(draftSettings), phase: result.phase };
    currentPage = 'display'; showPage('display'); editing = false; viewingDraft = false; selected = null; contentMode = false; selectedElement = null; undo = []; redo = [];
    ++previewSequence; previewAbort?.abort(); window.clearTimeout(previewTimer);
    notice(result.phase === 'paused' ? 'Saved. Display updates are paused for quiet hours.' : result.phase === 'pending' ? 'Saved. A display update is pending.' : result.phase === 'refreshing' ? 'Saved. The simulator is refreshing.' : 'Saved. The display is already up to date.');
    displayNoticePending = result.phase !== 'idle';
  } catch (error) { notice(String(error), true); }
  finally { saving = false; render(); }
}
button('edit').onclick = () => {
  editing = true; viewingDraft = true; draft = clone(saved.layout); draftSettings = clone(saved.refresh_settings); contentMode = false; selectedElement = null; draftImage = $<HTMLImageElement>('frame').src;
  undo = []; redo = []; selected = null; conflict = false; notice(); render(); queuePreview();
};
button('discard').onclick = async () => {
  if (designSession) { await closeDesigner(); return; }
  if (drag) endDrag(true);
  try {
    const current = await api<Workspace>('/api/state');
    saved = current; draft = clone(current.layout); draftSettings = clone(current.refresh_settings); contentMode = false; selectedElement = null; editing = false; viewingDraft = false; selected = null; conflict = false;
    ++previewSequence; previewAbort?.abort(); window.clearTimeout(previewTimer); notice(); showPage('display'); render(); updateStatus();
  } catch (error) { notice(String(error), true); }
};
button('save').onclick = () => { void save(); };
button('view-current').onclick = () => { if (drag) endDrag(); viewingDraft = false; render(); };
button('view-draft').onclick = () => { viewingDraft = true; render(); };
button('undo').onclick = () => history('undo'); button('redo').onclick = () => history('redo');
button('add-text').onclick = () => addComponent('text'); button('add-data').onclick = () => addComponent('data'); button('add-clock').onclick = () => addComponent('clock');
button('display-settings').onclick = () => { if (!saved || !connected || saving) return; if (currentPage === 'sources' && (connectionBusy || connectionDirty())) { connectionNotice('Save or discard your form before leaving data sources.'); return; } if (!editing) button('edit').click(); viewingDraft = true; showPage('device'); };
button('device-undo').onclick = () => history('undo');
button('device-redo').onclick = () => history('redo');
button('display-open').onclick = () => { if (saved) showPage('display'); };
button('about-open').onclick = () => { if (saved) showPage('about'); };
button('about-close').onclick = () => showPage('display');
button('add-open').onclick = () => { const open = $('add-picker').hidden; $('add-picker').hidden = !open; button('add-open').setAttribute('aria-expanded', String(open)); if (open) { renderTemplates(); button('add-text').focus(); } };
button('add-close').onclick = () => { closePicker(); button('add-open').focus(); };
document.addEventListener('pointerdown', event => { if (!(event.target as HTMLElement).closest('#add-picker, #add-open')) closePicker(); });
button('exit-content').onclick = leaveContents;
button('use-flow').onclick = useFlow;
button('refresh').onclick = async () => {
  button('refresh').disabled = true;
  try {
    const result = await api<{ phase: string }>('/api/refresh', { method: 'POST' });
    notice(result.phase === 'paused' ? 'Refresh queued until quiet hours end.' : result.phase === 'idle' ? 'The display is already up to date.' : 'Refreshing the saved layout.');
    displayNoticePending = result.phase !== 'idle';
    await poll();
  } catch (error) { notice(String(error), true); }
  finally { button('refresh').disabled = !connected; }
};
$('grid').onchange = () => render(false);
button('zoom-fit').onclick = () => { autoFit = true; fit(); };
button('zoom-in').onclick = () => { autoFit = false; zoom += .1; fit(); };
button('zoom-out').onclick = () => { autoFit = false; zoom -= .1; fit(); };
new ResizeObserver(() => { if (!drag) fit(); }).observe($('viewport'));
window.addEventListener('beforeunload', event => { if (dirty() || connectionDirty() || !!designSession?.previous.editing && (!equal(designSession.previous.state.layout, saved.layout) || !equal(designSession.previous.state.settings,saved.refresh_settings))) { event.preventDefault(); event.returnValue = ''; } });
document.addEventListener('keydown', event => {
  if (document.querySelector(':popover-open')) return;
  if ($<HTMLDialogElement>('confirm-dialog').open || currentPage === 'sources' || currentPage === 'about' || currentPage === 'designs') return;
  if (event.key === 'Escape' && !$('add-picker').hidden) { event.preventDefault(); closePicker(); button('add-open').focus(); return; }
  const typing = (event.target as HTMLElement).closest('input,textarea,select,[contenteditable=true]');
  const mod = event.ctrlKey || event.metaKey;
  if (mod && event.key.toLowerCase() === 's') { event.preventDefault(); void save(); return; }
  if (!active()) return;
  if (event.key === 'Escape') {
    if (designSession && !drag) { $('viewport').focus({preventScroll:true}); selectedElement=null; render(); return; }
    if (drag) endDrag(true);
    else if (editingContents()) { $('viewport').focus({preventScroll:true}); if (selectedElement) { selectedElement = null; render(); } else leaveContents(); }
    else { $('viewport').focus({preventScroll:true}); selected = null; render(); }
    return;
  }
  if (typing || drag) return;
  if (mod && event.key.toLowerCase() === 'z') { event.preventDefault(); history(event.shiftKey ? 'redo' : 'undo'); return; }
  const c = component();
  if (!c || (currentPage !== 'display' && currentPage !== 'designer')) return;
  if (event.key === 'Delete' || event.key === 'Backspace') {
    event.preventDefault();
    if (editingContents()) {
      if (selectedElement && c.blocks.length > 1) { const id = selectedElement; change(l => { const target = l.components.find(component => component.id === c.id)!; target.blocks = target.blocks.filter(block => block.id !== id); }); selectedElement = null; render(); }
    } else { const id = c.id; change(l => { l.components = l.components.filter(c => c.id !== id); }); selected = null; render(); }
    return;
  }
  const delta: Record<string, [number, number]> = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
  const movement = delta[event.key];
  if (movement) {
    if (designSession && c.content_layout !== 'positioned') return;
    if (!(event.target as HTMLElement).closest('#overlays, #component-list, #viewport')) return;
    event.preventDefault(); const step = event.shiftKey ? 8 : 1;
    if (editingContents() && !selectedElement) return;
    change(l => {
      const target = l.components.find(c => c.id === selected)!;
      const rect = editingContents() ? target.blocks.find(block => block.id === selectedElement)!.rect! : target.rect;
      rect.x += movement[0] * step; rect.y += movement[1] * step;
    }, `move.${c.id}.${editingContents() ? selectedElement : ''}`);
  }
});
function renderRefreshStatus() {
  const refresh = saved.refresh; const operation = refresh.active ?? refresh.last_completed;
  const reasons: Record<string, string> = { changed_area: 'Changed area', partial_unsupported: 'Full refresh required by display', unknown_baseline: 'Recovering display state', component_request: 'Requested by component', partial_limit: 'Partial update limit', maintenance_due: 'Maintenance interval reached', large_area: 'Large changed area' };
  const waitingAfterError = saved.last_error && !refresh.active;
  $('refresh-mode').textContent = !connected ? 'Refresh status unavailable' : waitingAfterError ? 'Display update failed' : operation ? `${operation.mode === 'full' ? 'Full' : 'Partial'} refresh${refresh.active ? ' in progress' : ' completed'}` : 'Software simulator';
  $('refresh-detail').textContent = !connected ? 'Last completed frame retained.' : waitingAfterError ? `${saved.last_error}${refresh.wait_ms > 0 ? ` Retry in ${Math.ceil(refresh.wait_ms / 1000)} s.` : ''}` : saved.phase === 'paused' ? 'Quiet hours · Latest content waits until updates resume.' : saved.pending && !refresh.active ? `Latest content queued${refresh.wait_ms > 0 ? ` · Eligible in ${Math.ceil(refresh.wait_ms / 1000)} s` : ''}.` : operation ? `${refresh.active && operation.revision < saved.revision ? 'Newer saved layout queued · ' : ''}${reasons[operation.reason] ?? operation.reason} · ${refresh.partial_count} partial updates since full refresh` : 'No refresh operation since startup.';
  $('refresh-mode').title = operation ? `${operation.region.width} × ${operation.region.height} pixels at ${operation.region.x}, ${operation.region.y} · revision ${operation.revision}` : '';
}
function updateStatus() {
  renderRefreshStatus();
  $('device-status').textContent = !connected ? 'Service unavailable' : saved.phase === 'paused' ? 'Quiet hours · paused' : saved.phase === 'pending' ? 'Display update pending' : saved.phase === 'error' ? 'Display update failed' : saved.phase === 'refreshing' ? 'Simulator refreshing' : 'Simulator idle';
  const dot = document.querySelector('.device .status-dot')!;
  dot.classList.toggle('busy', connected && saved.phase === 'refreshing'); dot.classList.toggle('offline', !connected);
  const onLan = location.hostname !== 'localhost' && location.hostname !== '127.0.0.1' && location.hostname !== '[::1]';
  $('host-address').textContent = onLan ? location.host : saved.addresses[0] ?? location.host;
  $('host-address').title = ['Browser address: ' + location.host, ...saved.addresses].join('\n');
}
async function poll() {
  if (saving || drag) return;
  try {
    const current = await api<Workspace>('/api/state');
    if (saving || drag) return;
    const dataChanged = current.data_revision !== saved.data_revision || current.frame_id.split('-').slice(0, -1).join('-') !== saved.frame_id.split('-').slice(0, -1).join('-');
    if (dataChanged) {
      const updated = await api<Dataset[]>('/api/catalog');
      if (saving || drag) return;
      const changedSchema = JSON.stringify(updated.map(d => [d.connection_id, d.connection_label, d.dataset_id, d.fields])) !== JSON.stringify(bindingCatalog().map(d => [d.connection_id, d.connection_label, d.dataset_id, d.fields]));
      catalog = updated;
      if (editing && viewingDraft) queuePreview();
      if (changedSchema && active() && !document.activeElement?.closest('input,textarea,select')) renderInspector();
    }
    if (designSession) { const minute=Math.floor(current.clock_now/60); if(minute!==draftMinute&&component()?.blocks.some(block=>block.source.kind==='clock')){draftMinute=minute;queuePreview();} connected=true; saved.phase=current.phase; saved.refresh=current.refresh; saved.clock_now=current.clock_now; saved.data_revision=current.data_revision; updateStatus(); return; }
    const changed = !connected || current.revision !== saved.revision || current.frame_id !== saved.frame_id || current.displayed_revision !== saved.displayed_revision || current.phase !== saved.phase;
    connected = true;
    if (editing) {
      if (current.revision !== saved.revision && !conflict) { conflict = true; notice('A newer layout was saved in another tab. Your draft is retained. Discard to load the newer layout.', true); }
      saved.phase = current.phase; saved.displayed_revision = current.displayed_revision; saved.frame_id = current.frame_id; saved.clock_now = current.clock_now; saved.clock_fixed = current.clock_fixed; saved.data_revision = current.data_revision;
      saved.refresh = current.refresh; saved.last_error = current.last_error;
      saved.connector_execution = current.connector_execution; saved.fixture_data = current.fixture_data;
    } else { saved = current; draft = clone(current.layout); draftSettings = clone(current.refresh_settings); }
    if (!viewingDraft && displayedFrame !== current.frame_id) { displayedFrame = current.frame_id; setDisplayedImage(true); }
    const minute = Math.floor(current.clock_now / 60);
    if (editing && viewingDraft && minute !== draftMinute && draft.components.some(c => c.blocks.some(b => b.source.kind === 'clock'))) { draftMinute = minute; queuePreview(); }
    updateStatus();
    if (displayNoticePending && current.phase === 'idle') notice('Display is up to date.');
    // Polling must not replace focused component buttons every second.
    if (changed) render(false);
  } catch { connected = false; if (saved) { updateStatus(); render(false); } }
}
let pollTimer = 0;
async function loadMetadata() {
  const requests = await Promise.allSettled([
    api<Dataset[]>('/api/catalog'), api<string[]>('/api/timezones'),
    api<{ connectors: ConnectorDefinition[] }>('/api/connectors'), api<Connections>('/api/connections'), api<IconPack[]>('/api/icon-packs'), api<DesignLibrary>('/api/designs'),
  ]);
  const [datasets, timezones, definitions, connections, packs, library] = requests;
  if (datasets?.status === 'fulfilled') catalog = datasets.value as Dataset[];
  if (timezones?.status === 'fulfilled') zones = timezones.value as string[];
  if (definitions?.status === 'fulfilled') connectorDefinitions = (definitions.value as {connectors: ConnectorDefinition[]}).connectors;
  if (connections?.status === 'fulfilled') connectionStore = connections.value as Connections;
  if (packs?.status === 'fulfilled') iconPacks = packs.value as IconPack[];
  if (library?.status === 'fulfilled') { designLibrary=library.value as DesignLibrary; designLibraryLoaded=true; }
  const failures = requests.filter(result => result.status === 'rejected').length;
  $('metadata-warning').hidden = !failures;
  if (failures) $('metadata-message').textContent = 'Some data sources or editing options could not load. Your display is available. Retry to restore the missing options.';
  render();
}
async function start() {
  button('retry-start').disabled = true;
  try {
    saved = await api<Workspace>('/api/state'); connected = true;
    draft = clone(saved.layout); draftSettings = clone(saved.refresh_settings);
    $('loading-state').hidden = true; $('display-workspace').hidden = false;
    updateStatus(); render(); await loadMetadata();
    const linked = new URL(location.href).searchParams.get('google');
    if (linked) {
      const url = new URL(location.href); url.searchParams.delete('google'); window.history.replaceState(null, '', url);
      let message = 'Google account linking failed or expired. Try linking again from Data sources → Google accounts.';
      try { googleStatus = await api<GoogleStatus>('/api/google'); if (googleStatus.last_link_error) message = `Google account linking failed. ${googleStatus.last_link_error.message}`; } catch { /* Retain recovery guidance. */ }
      const returnToCalendar = sessionStorage.getItem('paperphase.pending-source') === 'calendar.google';
      sessionStorage.removeItem('paperphase.pending-source');
      await openConnections();
      if (linked === 'connected' && returnToCalendar) {
        const definition = connectorDefinitions.find(definition => definition.id === 'calendar.google');
        if (definition) beginConnection(undefined, definition);
      } else showGoogle();
      connectionNotice(linked === 'connected' ? 'Google account connected. Choose a calendar for your display.' : message, linked !== 'connected');
    }
    if (!pollTimer) pollTimer = window.setInterval(() => { void poll(); }, 1000);
  } catch (error) {
    connected = false; $('loading-state').hidden = false; $('display-workspace').hidden = true;
    $('loading-title').textContent = 'Cannot connect to Paperphase';
    $('loading-message').textContent = 'Check that the service is running, then retry. Your saved display has not changed.';
    $('device-status').textContent = 'Service unavailable';
    button('retry-start').hidden = false; button('edit').disabled = true;
    notice(String(error), true);
  } finally { button('retry-start').disabled = false; }
}
button('retry-start').onclick = () => { notice(); void start(); };
button('retry-metadata').onclick = async () => { button('retry-metadata').disabled = true; try { await loadMetadata(); } finally { button('retry-metadata').disabled = false; } };

void start();

let connectorDefinitions: ConnectorDefinition[] = [];
let connectionStore: Connections = { revision: 0, connections: [], execution: 'settings_only' };
let connectionDraft: Connection | undefined;
let connectionBaseline = '';
let connectionBusy = false;
let connectionConflict = false;
let connectionIsNew = false;
let googleStatus: GoogleStatus | undefined;
const connectionDialog = () => $<HTMLDialogElement>('connections-dialog');
function connectionDirty() { return !!connectionDraft && JSON.stringify(connectionDraft) !== connectionBaseline; }
function connectionNotice(message = '', error = false) {
  const target = $('connections-notice'); target.hidden = !message; target.textContent = message; target.classList.toggle('error', error);
}
function connectionButtons() {
  const save = $('connection-save') as HTMLButtonElement | null;
  if (save) save.disabled = connectionBusy || connectionConflict || (!connectionIsNew && !connectionDirty());
  button('connection-new').disabled = connectionBusy || !!connectionDraft || !connectorDefinitions.length;
  button('connections-close').disabled = connectionBusy;
  button('google-open').disabled = connectionBusy || !!connectionDraft;
  $('connection-properties').inert = connectionBusy;
  $('connections-list').inert = connectionBusy;
  const google = document.getElementById('google-accounts'); if (google) google.inert = connectionBusy || !!connectionDraft;
}
async function loadConnections() {
  const [definitions, store, google] = await Promise.allSettled([api<{ connectors: ConnectorDefinition[] }>('/api/connectors'), api<Connections>('/api/connections'), api<GoogleStatus>('/api/google')]);
  if (store.status === 'rejected') throw store.reason;
  connectionStore = store.value;
  if (definitions.status === 'fulfilled') connectorDefinitions = definitions.value.connectors;
  googleStatus = google.status === 'fulfilled' ? google.value : undefined;
  document.querySelector('.connections-scope')!.textContent = connectionStore.execution === 'compiled' ? 'Fetch now checks for new data using saved settings. Display updates respect quiet hours.' : 'Plugins are not loaded. You can save settings and arrange components; fetching needs service setup.';
  $('fetch-setup').hidden = !connectionStore.connections.some(connection => connection.fetch?.status === 'settings_only');
  connectionDraft = undefined; connectionBaseline = ''; connectionConflict = false; connectionIsNew = false;
  renderGoogleAccounts(); renderConnections(); renderTemplates(); renderRuntimeWarning();
  if (definitions.status === 'rejected' || google.status === 'rejected') connectionNotice('Some setup options are unavailable. Return to Display and reopen Data sources to retry.', true);
}
function showGoogle() {
  if (connectionDraft || connectionBusy) return;
  choosingSource = false; googleView = true; renderGoogleAccounts(); renderConnections();
  $('google-panel').querySelector<HTMLElement>('h2')?.focus();
}
function chooseCalendar(account: GoogleAccount) {
  const definition = connectorDefinitions.find(item => item.id === 'calendar.google');
  if (!definition) { connectionNotice('The Google Calendar plugin is unavailable. Reload its plugin configuration and try again.', true); return; }
  beginConnection(undefined, definition);
  if (connectionDraft) { connectionDraft.settings.account_id = account.id; connectionDraft.settings.calendar_id = 'primary'; renderConnections(); }
}
function renderGoogleAccounts() {
  const parent = node('section', 'google-accounts'); parent.id = 'google-accounts'; $('google-panel').replaceChildren(parent);
  const title = node('h2', '', 'Google accounts'); title.tabIndex = -1; parent.append(title);
  parent.append(node('p', '', 'Connect an account, then choose a calendar for your display. Google will ask you to approve read-only calendar access.'));
  const status = googleStatus;
  if (!status) { parent.append(node('p', 'error', 'Google setup is unavailable. Reopen Data sources to retry.')); return; }
  if (status.last_link_error) parent.append(node('p', 'error', status.last_link_error.message));
  if (!status.configured) parent.append(node('p', 'setup-guidance', 'This Paperphase needs a Google OAuth client before accounts can be connected. Create a Web application client in Google Cloud, enable Calendar API and register the hosted callback URL below.'));
  const setup = node('details', 'advanced'); setup.id = 'google-setup'; setup.open = !status.configured;
  setup.append(node('summary', '', status.configured ? 'Advanced: Google app setup' : 'Set up Google app'));
  const guide = node('a', '', 'Google setup instructions'); guide.href = 'https://github.com/Odd-Orbit-Software/Paperphase/blob/main/docs/usage/google-calendar.md'; guide.target = '_blank'; guide.rel = 'noreferrer'; setup.append(guide);
  parent.append(setup);
  const config = node('form', 'google-config'); setup.append(config);
  const client = node('input'); client.required = true; client.value = status.client_id ?? ''; client.maxLength = 1024;
  label('Google OAuth client ID', client, config);
  const secret = node('input'); secret.type = 'password'; secret.autocomplete = 'new-password'; secret.required = !status.client_secret_present; secret.maxLength = 1024;
  secret.placeholder = status.client_secret_present ? 'Saved; leave blank to keep' : 'OAuth client secret'; label('Google OAuth client secret', secret, config);
  const redirect = node('input'); redirect.type = 'url'; redirect.required = true; redirect.value = status.redirect_uri; label('Hosted callback URL', redirect, config);
  const save = node('button', 'secondary', 'Save Google setup'); save.type = 'submit'; config.append(save);
  config.onsubmit = async event => {
    event.preventDefault(); connectionBusy = true; connectionButtons();
    try {
      googleStatus = await api<GoogleStatus>('/api/google/config', {method: 'PUT', headers: {'Content-Type':'application/json'}, body: JSON.stringify({client_id:client.value.trim(), client_secret:secret.value, redirect_uri:redirect.value.trim()})});
      secret.value = ''; renderGoogleAccounts(); connectionNotice('Google setup saved. You can link an account now.');
    } catch (error) { connectionNotice(String(error), true); }
    finally { connectionBusy = false; connectionButtons(); }
  };
  for (const account of status.accounts) {
    const row = node('div', 'google-account'); row.append(node('span', '', account.label + (account.requires_reauth ? ' · Reconnect required' : ` · ${account.calendars.length} calendars`)));
    const choose = node('button', 'small', 'Choose a calendar'); choose.type = 'button'; choose.setAttribute('aria-label', `Choose a calendar from ${account.label}`); choose.onclick = () => chooseCalendar(account);
    const reconnect = node('button', 'small', 'Reconnect'); reconnect.type = 'button'; reconnect.setAttribute('aria-label', `Reconnect ${account.label}`); reconnect.onclick = () => { void linkGoogle(account.label); };
    const remove = node('button', 'small', 'Disconnect'); remove.type = 'button'; remove.setAttribute('aria-label', `Disconnect ${account.label}`);
    remove.onclick = async () => {
      if (!await confirmAction(`Disconnect ${account.label}?`, 'This removes the saved account access from this Paperphase. Calendar connections using it will need another account.', 'Disconnect account')) return;
      connectionBusy = true; connectionButtons();
      try { googleStatus = await api<GoogleStatus>(`/api/google/accounts/${encodeURIComponent(account.id)}`, {method:'DELETE'}); renderGoogleAccounts(); connectionNotice('Account disconnected from this Pi.'); }
      catch (error) { connectionNotice(String(error), true); }
      finally { connectionBusy = false; connectionButtons(); }
    };
    row.append(choose, reconnect, remove); parent.append(row);
  }
  const form = node('form', 'google-link'); const accountLabel = node('input'); accountLabel.required = true; accountLabel.maxLength = 128; accountLabel.placeholder = 'Personal or work'; label('New Google account label', accountLabel, form);
  const link = node('button', 'primary', 'Link Google account'); link.type = 'submit'; link.disabled = !status.configured || status.accounts.length >= 8; form.append(link); parent.append(form);
  form.onsubmit = event => { event.preventDefault(); void linkGoogle(accountLabel.value); };
  if (status.configured) parent.append(setup);
  parent.append(node('p', 'field-help', 'Finish linking in this browser while on the same network as your Paperphase. Account access tokens are stored on your Paperphase, not on the public callback service.'));
}
async function linkGoogle(label: string) {
  if (dirty() || connectionDraft) { connectionNotice('Save or discard your layout and connection drafts before linking Google.', true); return; }
  connectionBusy = true; connectionButtons();
  try {
    const result = await api<{url:string}>('/api/google/begin', {method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({label})});
    if (pendingCalendar) sessionStorage.setItem('paperphase.pending-source', 'calendar.google');
    window.location.assign(result.url);
  } catch (error) { connectionNotice(String(error), true); connectionBusy = false; connectionButtons(); }
}
function beginConnection(connection?: Connection, chosenDefinition?: ConnectorDefinition) {
  if (connectionDraft || connectionBusy) return;
  const definition = chosenDefinition;
  if (!connection && !definition) { choosingSource = true; googleView = false; renderConnections(); return; }
  if (!connection && definition?.id === 'calendar.google' && !googleStatus?.accounts.length) { pendingCalendar = true; showGoogle(); return; }
  choosingSource = false; googleView = false; pendingCalendar = false;
  connectionIsNew = !connection;
  connectionDraft = connection ? clone(connection) : newConnection(definition!);
  // Treat setting IDs as literal keys, including names inherited by plain objects.
  connectionDraft.settings = Object.assign(Object.create(null), connectionDraft.settings);
  connectionBaseline = connection ? JSON.stringify(connectionDraft) : '';
  connectionConflict = false; connectionNotice(); renderConnections();
}
function renderServicePicker(parent: HTMLElement) {
  const picker = node('div', 'service-picker'); parent.append(picker);
  picker.append(node('h2', '', 'What would you like to connect?'), node('p', '', 'Choose a service, then give the source a name and adjust its settings.'));
  const list = node('div', 'service-list'); picker.append(list);
  const advanced = node('details', 'advanced'); advanced.append(node('summary', '', 'Advanced: demo & test connectors'));
  const advancedList = node('div', 'service-list'); advanced.append(advancedList);
  for (const definition of connectorDefinitions) {
    const choice = node('button', 'service-choice'); choice.type = 'button';
    const text = node('span'); text.append(node('strong', '', definition.name), node('small', '', definition.description)); choice.append(text);
    choice.onclick = () => beginConnection(undefined, definition);
    (/^(demo|example)\./.test(definition.id) ? advancedList : list).append(choice);
  }
  if (advancedList.childElementCount) picker.append(advanced);
  if (!list.childElementCount) picker.append(node('p', '', 'No live service plugins are available. Check the service plugin configuration.'));
}
function newConnection(definition: ConnectorDefinition): Connection {
  const settings: Connection['settings'] = Object.create(null);
  for (const field of definition.settings) if ('default' in field.input && field.input.default != null) settings[field.id] = clone(field.input.default);
  return { id: identifier('connection'), connector_id: definition.id, label: definition.name, enabled: true, poll_interval_secs: definition.default_poll_secs, settings, secret_present: [] };
}
function renderConnections() { retainFocus($('connections-dialog'), buildConnections); }
function buildConnections() {
  $('google-panel').hidden = !googleView; $('connection-properties').hidden = googleView;
  button('google-open').disabled = !!connectionDraft || connectionBusy;
  const list = $('connections-list'); list.replaceChildren();
  for (const connection of connectionStore.connections) {
    const item = node('button', `connection-item ${connectionDraft?.id === connection.id ? 'selected' : ''}`);
    item.setAttribute('aria-label', `Configure ${connection.label}`); item.dataset.focus = connection.id;
    item.append(node('strong', '', connection.label), node('small', '', `${!connection.enabled ? 'Paused' : connection.fetch?.status === 'settings_only' ? 'Plugin not loaded' : connection.fetch?.status === 'fetching' ? 'Fetching' : 'Enabled'} · ${connection.poll_interval_secs >= 60 && connection.poll_interval_secs % 60 === 0 ? `${connection.poll_interval_secs / 60} min` : `${connection.poll_interval_secs} s`}`));
    item.disabled = !!connectionDraft;
    item.onclick = () => beginConnection(connection);
    list.append(item);
    if (connection.fetch) {
      const status = node('div', 'connection-fetch'); status.dataset.fetchId = connection.id;
      const text = node('small', 'field-help'); text.dataset.fetchDetails = 'true'; status.append(text);
      const fetch = node('button', 'small', 'Fetch now'); fetch.dataset.fetchButton = 'true'; fetch.setAttribute('aria-label', `Fetch ${connection.label} now`);
      fetch.onclick = () => { void fetchConnection(connection.id); }; status.append(fetch); list.append(status);
      updateFetchControl(status, connection);
    }
  }
  const parent = $('connection-properties'); parent.replaceChildren();
  const current = connectionDraft;
  if (!current) {
    if (choosingSource) { renderServicePicker(parent); connectionButtons(); return; }
    const empty = node('div', 'empty-properties');
    empty.append(node('h3', '', connectionStore.connections.length ? 'Choose a data source.' : 'Start with a data source.'), node('p', '', connectionStore.connections.length ? 'Select a source to adjust what it shows and how often it updates, or add another source.' : 'Add weather, a calendar, headlines or another source to bring useful information to your display.'));
    action('Add data source', 'Add your first data source', empty, () => beginConnection());
    parent.append(empty); connectionButtons(); return;
  }
  const definition = connectorDefinitions.find(d => d.id === current.connector_id)!;
  const form = node('form', 'connection-form'); form.id = 'connection-form'; parent.append(form);
  const fields = node('div', 'connection-fields'); form.append(fields);
  fields.append(node('h3', '', connectionIsNew ? 'New connection' : current.label), node('p', 'connection-description', definition.description));
  if (connectionIsNew) select('Connector', definition.id, connectorDefinitions.map(d => [d.id, d.name]), fields, value => {
    const nextDefinition = connectorDefinitions.find(d => d.id === value)!;
    if (value === 'calendar.google' && !googleStatus?.accounts.length) { connectionDraft = undefined; connectionBaseline = ''; pendingCalendar = true; showGoogle(); return; }
    const next = newConnection(nextDefinition); next.id = current.id;
    connectionDraft = next; renderConnections();
  });
  const labelInput = node('input'); labelInput.value = current.label; labelInput.required = true; labelInput.maxLength = 96;
  labelInput.oninput = () => { current.label = labelInput.value; connectionButtons(); };
  label('Connection name', labelInput, fields);
  const enabledLabel = node('label', 'check'); const enabled = node('input'); enabled.type = 'checkbox'; enabled.checked = current.enabled;
  enabled.onchange = () => { current.enabled = enabled.checked; connectionButtons(); }; enabledLabel.append(enabled, document.createTextNode('Enabled')); fields.append(enabledLabel);
  const interval = node('input'); interval.type = 'number'; interval.min = String(definition.min_poll_secs); interval.max = '86400'; interval.step = '1'; interval.required = true; interval.value = String(current.poll_interval_secs);
  interval.oninput = () => { current.poll_interval_secs = interval.valueAsNumber; connectionButtons(); };
  label('Update interval (seconds)', interval, fields);
  const sourceSettings = node('div', 'source-settings'); fields.append(sourceSettings);
  const forecast = node('details', 'advanced forecast-options'); forecast.append(node('summary', '', 'Forecast options'));
  const forecastFields = node('div', 'source-settings'); forecast.append(forecastFields);
  for (const field of definition.settings) {
    const group = node('div', 'connection-setting');
    if (field.input.type === 'table' || field.input.type === 'secret' || field.input.type === 'url' || field.input.type === 'text' && field.input.multiline) group.classList.add('full-width');
    const forecastOption = definition.id === 'weather.open-meteo' && ['forecast_hours', 'forecast_days', 'rain_threshold'].includes(field.id);
    (forecastOption ? forecastFields : sourceSettings).append(group);
    const inputDefinition = field.input;
    const value = current.settings[field.id];
    let input: HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement;
    if (definition.id === 'calendar.google' && (field.id === 'account_id' || field.id === 'calendar_id')) {
      const accounts = googleStatus?.accounts ?? [];
      const account = accounts.find(a => a.id === current.settings.account_id);
      const options: [string,string][] = field.id === 'account_id' ? accounts.map(a => [a.id, a.label + (a.requires_reauth ? ' (reconnect)' : '')]) : (account?.calendars ?? []).map(c => [c.id, c.label + (c.primary ? ' (primary)' : '')]);
      if (field.id === 'calendar_id' && account) options.unshift(['primary', 'Primary calendar']);
      const chosen = typeof value === 'string' ? value : '';
      const control = select(field.label, chosen, [['', 'Choose…'], ...options], group, selected => {
        current.settings[field.id] = selected;
        if (field.id === 'account_id') { current.settings.calendar_id = 'primary'; renderConnections(); }
        connectionButtons();
      }); (control as HTMLSelectElement).required = true;
      if (field.id === 'account_id' && !accounts.length) group.append(node('small', 'field-help', 'No accounts are available. Discard this form and choose Google accounts to reconnect.'));
      continue;
    }
    if (inputDefinition.type === 'table') {
      const section=node('section','setting-table');section.setAttribute('aria-label',field.label);group.append(section);
      section.append(node('h4','',field.label));
      if(field.help)section.append(node('p','setting-help',field.help));
      const rows=(Array.isArray(value)?value:clone(inputDefinition.default)) as Record<string,string>[];
      current.settings[field.id]=rows;
      const entries=node('div','setting-table-entries');section.append(entries);
      const draw=()=>{
        entries.replaceChildren();
        rows.forEach((row,index)=>{
          const entry=node('fieldset','setting-table-row');entry.append(node('legend','',`Row ${index+1}`));entries.append(entry);
          const cells=node('div','setting-table-cells');entry.append(cells);
          inputDefinition.columns.forEach(column=>{
            const cell=column.choices.length?node('select'):column.multiline?node('textarea'):node('input');
            if(cell instanceof HTMLSelectElement){
              if(!column.required){const option=node('option','','Not set');option.value='';cell.append(option);}
              column.choices.forEach(choice=>{const option=node('option','',choice.label);option.value=choice.value;cell.append(option);});
            }else{cell.maxLength=column.max_bytes;if(cell instanceof HTMLTextAreaElement)cell.rows=3;}
            cell.required=column.required;cell.value=row[column.id]??'';
            cell.setAttribute('aria-label',`${column.label} ${index+1}`);
            cell.oninput=()=>{row[column.id]=cell.value;connectionButtons();};
            label(column.label,cell,cells);cell.setAttribute('aria-label',`${column.label} ${index+1}`);if(column.multiline)cells.lastElementChild?.classList.add('wide-cell');
          });
          const remove=node('button','subtle',`Remove row ${index+1}`);remove.type='button';remove.setAttribute('aria-label',`Remove row ${index+1} from ${field.label}`);
          remove.onclick=()=>{rows.splice(index,1);draw();connectionButtons();};entry.append(remove);
        });
        add.disabled=rows.length>=inputDefinition.max_rows;
      };
      const add=node('button','secondary',`Add row to ${field.label}`);add.type='button';
      add.onclick=()=>{rows.push(Object.fromEntries(inputDefinition.columns.map(column=>[column.id,column.required&&column.choices.length?column.choices[0]?.value??'':''])));draw();connectionButtons();entries.lastElementChild?.querySelector<HTMLInputElement>('input,select')?.focus();};
      section.append(add);draw();continue;
    }
    if (inputDefinition.type === 'choice') {
      input = node('select');
      if (!field.required) { const option = node('option', '', 'Not set'); option.value = ''; input.append(option); }
      for (const choice of inputDefinition.options) { const option = node('option', '', choice.label); option.value = choice.value; input.append(option); }
      input.value = typeof value === 'string' ? value : '';
      input.onchange = () => { current.settings[field.id] = input.value === '' ? null : input.value; connectionButtons(); };
      input.required = field.required;
    } else if (inputDefinition.type === 'text' && inputDefinition.multiline) {
      input = node('textarea'); input.rows = 3; input.maxLength = inputDefinition.max_bytes; input.required = field.required; input.value = typeof value === 'string' ? value : '';
      input.oninput = () => { current.settings[field.id] = input.value; connectionButtons(); };
    } else {
      input = node('input');
      if (inputDefinition.type === 'boolean') {
        input.type = 'checkbox'; input.checked = value === true;
        input.onchange = () => { current.settings[field.id] = (input as HTMLInputElement).checked; connectionButtons(); };
      } else if (inputDefinition.type === 'integer' || inputDefinition.type === 'number') {
        input.type = 'number'; input.min = String(inputDefinition.min); input.max = String(inputDefinition.max); input.step = inputDefinition.type === 'number' ? 'any' : '1'; input.required = field.required; input.value = typeof value === 'number' ? String(value) : '';
        input.oninput = () => { current.settings[field.id] = input.value === '' ? null : (input as HTMLInputElement).valueAsNumber; connectionButtons(); };
      } else if (inputDefinition.type === 'secret') {
        input.type = 'password'; input.autocomplete = 'new-password'; input.maxLength = inputDefinition.max_bytes;
        const present = current.secret_present.includes(field.id);
        input.placeholder = present ? 'Saved · leave blank to keep' : 'Not set';
        input.required = field.required && !present;
        input.value = typeof value === 'string' ? value : '';
        const clear = node('input'); clear.type = 'checkbox'; clear.checked = value === null;
        input.oninput = () => {
          clear.checked = false; input.required = field.required && !present;
          if (input.value) { current.settings[field.id] = input.value; }
          else delete current.settings[field.id];
          connectionButtons();
        };
        clear.onchange = () => {
          if (clear.checked) current.settings[field.id] = null; else delete current.settings[field.id];
          input.value = ''; input.required = field.required && (!present || clear.checked); connectionButtons();
        };
        const secretGroup = node('div', 'secret-setting'); label(field.label, input, secretGroup);
        const clearLabel = node('label', 'check'); clearLabel.append(clear, document.createTextNode(`Clear ${field.label}`)); secretGroup.append(clearLabel);
        secretGroup.append(node('small', 'field-help', `${present ? 'A value is saved. ' : ''}${field.help}`)); group.append(secretGroup);
        continue;
      } else {
        input.type = inputDefinition.type === 'url' ? 'url' : 'text'; input.maxLength = inputDefinition.type === 'url' ? 2048 : inputDefinition.max_bytes; input.required = field.required; input.value = typeof value === 'string' ? value : '';
        input.oninput = () => { current.settings[field.id] = input.value; connectionButtons(); };
      }
    }
    label(field.label, input, group);
    if (inputDefinition.type === 'boolean') input.parentElement!.classList.add('boolean-setting');
    if (field.help) group.append(node('small', 'field-help', field.help));
  }
  if (forecastFields.childElementCount) fields.append(forecast);
  const actions = node('div', 'connection-actions'); form.append(actions);
  const discard = node('button', '', 'Discard form'); discard.type = 'button'; discard.onclick = async () => {
    connectionDraft = undefined; connectionBaseline = ''; connectionConflict = false;
    $('connection-properties').replaceChildren(); connectionBusy = true; connectionButtons();
    try { await loadConnections(); connectionNotice(); } catch (error) { connectionNotice(String(error), true); }
    finally { connectionBusy = false; connectionButtons(); }
  };
  const save = node('button', 'primary', 'Save connection'); save.type = 'submit'; save.id = 'connection-save'; actions.append(discard, save);
  if (!connectionIsNew) {
    const remove = node('button', 'danger', 'Delete connection'); remove.type = 'button';
    remove.onclick = async () => { if (await confirmAction(`Delete ${current.label}?`, 'This removes the saved connection and its credentials. Components using this source will no longer receive its data.', 'Delete connection')) void writeConnection(true); }; actions.prepend(remove);
  }
  form.onsubmit = event => { event.preventDefault(); if (form.reportValidity()) void writeConnection(false); };
  connectionButtons();
}
function updateFetchControl(container: HTMLElement, connection: Connection) {
  const status = connection.fetch; if (!status) return;
  const fetch = container.querySelector<HTMLButtonElement>('[data-fetch-button]')!;
  fetch.disabled = !status.can_fetch || connectionBusy;
  fetch.title = 'Fetch using saved connection settings.';
  const details = container.querySelector<HTMLElement>('[data-fetch-details]')!;
  if (status.status === 'preparing') { details.textContent = 'Preparing reference data…'; fetch.title = 'Fetching will start once the reference data is ready.'; return; }
  if (status.status === 'settings_only') {
    details.textContent = 'Plugin not loaded by this service. Settings and components alone do not fetch data.';
    fetch.title = 'Load the plugin and restart the service. See Enable fetching below.';
    return;
  }
  if (!connection.enabled) { details.textContent = 'Paused. Enable this source to fetch new data.'; return; }
  details.textContent = status.status === 'fetching' ? 'Fetching…' : status.last_error ?? (status.last_success_at ? `Last fetched ${new Date(status.last_success_at * 1000).toLocaleTimeString()}` : 'Waiting for the first fetch.');
  if (!status.can_fetch && status.fetch_after > 0) fetch.title = `Next eligible: ${new Date(status.fetch_after * 1000).toLocaleString()}`;
}
async function refreshConnectionStatus() {
  if (!connectionDialog().open || connectionBusy) return;
  try {
    const current = await api<Connections>('/api/connections');
    if (!connectionDialog().open || connectionBusy) return;
    if (connectionDraft || current.revision === connectionStore.revision) {
      // Configuration and password inputs stay untouched while status changes.
      for (const container of $('connections-list').querySelectorAll<HTMLElement>('[data-fetch-id]')) {
        const connection = current.connections.find(c => c.id === container.dataset.fetchId);
        if (connection) updateFetchControl(container, connection);
      }
      if (!connectionDraft) connectionStore = current;
    } else { connectionStore = current; renderConnections(); }
  } catch { /* Keep the current form during a transient service outage. */ }
}
async function fetchConnection(id: string) {
  const container = [...$('connections-list').querySelectorAll<HTMLElement>('[data-fetch-id]')].find(element => element.dataset.fetchId === id);
  const fetch = container?.querySelector<HTMLButtonElement>('[data-fetch-button]'); if (fetch) fetch.disabled = true;
  try {
    await api(`/api/connections/${encodeURIComponent(id)}/fetch`, { method: 'POST' });
    connectionNotice('Fetch started using saved settings. The display will update through its refresh policy.');
  } catch (error) { connectionNotice(String(error), true); }
  await refreshConnectionStatus();
}
async function writeConnection(remove: boolean) {
  if (!connectionDraft || connectionBusy || connectionConflict) return;
  connectionBusy = true; connectionNotice(); connectionButtons();
  const current = connectionDraft;
  try {
    const request = remove ? { base_revision: connectionStore.revision } : { base_revision: connectionStore.revision, connector_id: current.connector_id, label: current.label, enabled: current.enabled, poll_interval_secs: current.poll_interval_secs, settings: current.settings };
    await api(`/api/connections/${encodeURIComponent(current.id)}`, { method: remove ? 'DELETE' : 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(request) });
    // Wipe the form's secret inputs before any subsequent request can fail.
    connectionDraft = undefined; connectionBaseline = ''; $('connection-properties').replaceChildren();
    await loadConnections(); connectionNotice(remove ? 'Connection deleted.' : 'Connection settings saved.');
  } catch (error) { if (error instanceof ApiFailure && error.status === 409) connectionConflict = true; connectionNotice(String(error), true); }
  finally { connectionBusy = false; connectionButtons(); }
}
async function openConnections() {
  if (!saved || saving) return;
  if (!showPage('sources')) return;
  if (connectionDraft) return;
  connectionNotice('Loading data sources…'); connectionBusy = true; connectionButtons();
  try { await loadConnections(); connectionNotice(); } catch (error) { connectionNotice(`Could not load data sources. ${String(error)} Return to Display and try again.`, true); }
  finally { connectionBusy = false; connectionButtons(); }
}
button('connections-open').onclick = () => { void openConnections(); };
button('google-open').onclick = showGoogle;
button('connection-new').onclick = () => beginConnection();
function closeConnections(event?: Event) {
  if (connectionBusy || connectionDirty()) { event?.preventDefault(); connectionNotice('Save or discard your form before closing connections.'); return; }
  connectionDraft = undefined; connectionBaseline = ''; $('connection-properties').replaceChildren(); googleView = false; choosingSource = false; showPage('display');
  if (active()) renderInspector();
}
button('connections-close').onclick = () => closeConnections();
connectionDialog().addEventListener('cancel', event => closeConnections(event));
connectionDialog().addEventListener('keydown', event => {
  if (event.key === 'Escape' && !$<HTMLDialogElement>('confirm-dialog').open) { event.preventDefault(); closeConnections(event); return; }
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') {
    event.preventDefault();
    if (!connectionBusy && !connectionConflict && (connectionIsNew || connectionDirty())) $<HTMLFormElement>('connection-form')?.requestSubmit();
  }
});
window.setInterval(() => { void refreshConnectionStatus(); }, 1000);

function bindingCatalog(): Dataset[] {
  if (!designSession) return catalog;
  const source=designSession.source ?? 'design.source';
  const available=catalog.filter(dataset=>dataset.connection_id===source);
  const inferred=new Map<string,Dataset>();
  for (const template of connectorDefinitions.find(item=>item.id===designSession?.connector)?.templates ?? []) {
    for (const block of template.component.blocks) {
      const binding=sourceBinding(block.source); if (!binding) continue;
      let dataset=inferred.get(binding.dataset_id);
      if (!dataset) { dataset={connection_id:source,dataset_id:binding.dataset_id,fields:[],rows:0,stale:true}; inferred.set(binding.dataset_id,dataset); }
      if (!dataset.fields.some(field=>field.id===binding.field_id)) dataset.fields.push({id:binding.field_id,kind:block.source.kind==='icon'?'text':binding.decimals!=null?'number':'text'});
    }
  }
  return [...available,...[...inferred.values()].filter(dataset=>!available.some(item=>item.dataset_id===dataset.dataset_id))];
}
function useSavedDesign(design: SavedDesign, source: string) {
  const c=bindDesign(design.component,source); c.id=identifier('component'); c.label=design.name;
  const rect=designFits(c,draft);
  if (!rect) {startDesigner(design.connector_id,source,design.component,design.id);notice('No free area fits this design yet. It stays editable here.');return;}
  c.rect=rect; selected=c.id; contentMode=false; selectedElement=null;
  change(layout=>layout.components.push(c)); closePicker();
}
function retainEditor(): RetainedEditor {
  return {state:capture(),editing,viewingDraft,selected,contentMode,selectedElement,undo:clone(undo),redo:clone(redo),image:draftImage,reports:clone(reports),conflict,page:currentPage,focusId:(document.activeElement as HTMLElement)?.id,focusLabel:document.activeElement?.getAttribute('aria-label')??undefined};
}
function restoreEditor(previous: RetainedEditor) {
  ++previewSequence; previewAbort?.abort(); window.clearTimeout(previewTimer);
  restore(previous.state); editing=previous.editing; viewingDraft=previous.viewingDraft;
  selected=previous.selected; contentMode=previous.contentMode; selectedElement=previous.selectedElement;
  undo=previous.undo; redo=previous.redo; draftImage=previous.image; reports=previous.reports; conflict=previous.conflict;
  designSession=undefined; lastInspectedId=null;
  showPage(previous.page); if(previous.page==='designs')renderDesignLibrary(); notice(); render(); if (editing) queuePreview(); void poll();
  const origin=previous.focusId?document.getElementById(previous.focusId):previous.focusLabel?document.querySelector<HTMLElement>(`[aria-label="${CSS.escape(previous.focusLabel)}"]`):null;
  (origin??(previous.page==='designs'?$('design-library-title'):$('page-title'))).focus({preventScroll:true});
}
async function loadDesignLibrary() {
  designLibrary=await api<DesignLibrary>('/api/designs'); designLibraryLoaded=true;
}
async function openDesignLibrary() {
  if (!saved || designSession) return;
  if (!showPage('designs')) return;
  renderDesignLibrary();
  try { await loadDesignLibrary(); renderDesignLibrary(); }
  catch (error) { $('design-library-notice').textContent=`Could not load saved designs. ${String(error)} Use Reload designs to try again.`; }
}
function pluginLabel(id: string) { return id===builtinDesigns ? 'Text & time' : connectorDefinitions.find(plugin=>plugin.id===id)?.name ?? 'Unavailable plugin'; }
function renderDesignLibrary() {
  const sources=$('design-source-choices'); sources.replaceChildren();
  const definitions=[{id:builtinDesigns,name:'Text & time'},...connectorDefinitions];
  const advanced=node('details','advanced');advanced.append(node('summary','','Demo & test sources'));const advancedList=node('div');advanced.append(advancedList);
  for (const definition of definitions) {
    const row=node('div','design-source-choice');
    row.append(node('h3','',definition.name));
    const count=designLibrary.designs.filter(design=>design.connector_id===definition.id).length;
    row.append(node('p','subtle',`${count} saved ${count===1?'design':'designs'}`));
    action('Create design',`Create design for ${definition.name}`,row,()=>startDesigner(definition.id)); (/^(demo|example)\./.test(definition.id)?advancedList:sources).append(row);
  }
  if(advancedList.childElementCount)sources.append(advanced);
  const list=$('saved-design-list'); list.replaceChildren();
  const page=$('design-library-page'); const savedSection=list.parentElement!; const creation=$('design-source-choices').parentElement!;
  if(designLibrary.designs.length)page.insertBefore(savedSection,creation);else page.append(savedSection);
  if (!designLibrary.designs.length) list.append(node('p','subtle','Your saved designs will appear here. Create one from a source or start with text and time.'));
  for (const design of designLibrary.designs) {
    const row=node('div','saved-design-row'); const copy=node('div');
    copy.append(node('h3','',design.name),node('p','subtle',`${pluginLabel(design.connector_id)} · ${design.component.rect.width} × ${design.component.rect.height} px`)); row.append(copy);
    const available=design.connector_id===builtinDesigns || connectorDefinitions.some(plugin=>plugin.id===design.connector_id);
    action('Edit design',`Edit design ${design.name}`,row,()=>startDesigner(design.connector_id,undefined,design.component,design.id),!available);
    const remove=action('Delete',`Delete design ${design.name}`,row,async()=>{
      if (!await confirmAction(`Delete ${design.name}?`,'This removes the reusable design. Components already added to your display are kept.','Delete design')) return;
      remove.disabled=true;
      try { designLibrary=await api<DesignLibrary>(`/api/designs/${encodeURIComponent(design.id)}`,{method:'DELETE',headers:{'Content-Type':'application/json'},body:JSON.stringify({base_revision:designLibrary.revision})}); renderDesignLibrary(); }
      catch (error) { $('design-library-notice').textContent=String(error); remove.disabled=false; }
    }); remove.classList.add('danger'); list.append(row);
  }
}
function startDesigner(connector: string, source?: string, componentCopy?: Component, id?: string, original?: string) {
  if (!saved || designSession || saving) return;
  if (connectionDraft) { connectionNotice('Save or discard the connection form before designing a component.'); return; }
  const previous=retainEditor(); closePicker();
  const connection=source ?? connectionStore.connections.find(connection=>connection.connector_id===connector)?.id;
  const c=componentCopy ? bindDesign(componentCopy,connection) : {id:identifier('design'),label:'New design',rect:{x:0,y:0,width:32,height:32},padding:4,gap:2,border:false,content_layout:'flow' as const,blocks:[]};
  if (componentCopy && original) { c.blocks=clone(componentCopy.blocks); c.rect.x=0; c.rect.y=0; }
  c.label ||= componentCopy?name(componentCopy):'New design';
  designSession={previous,connector,source:connection,id,baseline:clone(c),original,busy:false,automatic:!componentCopy,fixedWidth:undefined,ready:false,error:'',needsFit:true,startingPoint:componentCopy&&!original&&!id?connectorDefinitions.find(plugin=>plugin.id===connector)?.templates?.find(template=>equal(template.component.blocks.map(block=>block.id),componentCopy.blocks.map(block=>block.id)))?.id:undefined};
  draft={schema_version:1,display:{id:'component-design',width:c.rect.width,height:c.rect.height},components:[c]};
  selected=c.id; selectedElement=c.blocks[0]?.id??null; editing=true; viewingDraft=true; contentMode=true;
  undo=[]; redo=[]; reports=[]; draftImage=''; conflict=false; autoFit=true; lastInspectedId=null;
  ++previewSequence; previewAbort?.abort(); window.clearTimeout(previewTimer);
  $<HTMLImageElement>('frame').removeAttribute('src');
  showPage('designer'); renderDesignerControls(); render(); queuePreview(); $('design-name').focus({preventScroll:true});
}
async function designExisting(c: Component) {
  const source=c.blocks.map(block=>sourceBinding(block.source)?.connection_id).find(Boolean);
  const firstBinding=c.blocks.map(block=>sourceBinding(block.source)).find(Boolean);
  const connector=connectionStore.connections.find(connection=>connection.id===source)?.connector_id
    ?? (firstBinding ? connectorDefinitions.find(plugin=>plugin.templates?.some(template=>template.component.blocks.some(block=>sourceBinding(block.source)?.dataset_id===firstBinding.dataset_id)))?.id : undefined);
  // Existing legacy/fixture bindings stay intact even without a configured source.
  startDesigner(connector ?? builtinDesigns,source,c,undefined,c.id);
}
function renderDesignerControls() {
  const session=designSession; if (!session) return;
  const nameInput=$<HTMLInputElement>('design-name'); nameInput.value=component()?.label??'';
  const source=$<HTMLSelectElement>('design-connection'); source.replaceChildren();
  const connections=connectionStore.connections.filter(connection=>connection.connector_id===session.connector);
  const empty=node('option','','Choose a source later'); empty.value=''; source.append(empty);
  for (const connection of connections) { const option=node('option','',connection.label); option.value=connection.id; source.append(option); }
  if (session.source && !connections.some(connection=>connection.id===session.source)) { const option=node('option','','Current component source'); option.value=session.source; source.append(option); }
  source.value=session.source??''; source.parentElement!.hidden=session.connector===builtinDesigns; $('design-source-label').textContent=pluginLabel(session.connector);
  const starts=$<HTMLSelectElement>('design-starting-point'); starts.replaceChildren();
  const blank=node('option','','Blank design'); blank.value=''; starts.append(blank);
  for (const template of connectorDefinitions.find(plugin=>plugin.id===session.connector)?.templates??[]) { const option=node('option','',template.name); option.value=template.id; starts.append(option); }
  starts.value=session.startingPoint??'';
  $('design-starting-label').hidden=!!session.original || !!session.id || starts.options.length===1;
  source.onchange=()=>{
    const selectedSource=source.value||undefined;
    change(layout=>{session.source=selectedSource;layout.components[0]=bindDesign(layout.components[0]!,session.source);},'design.source'); renderDesignerState();
  };
  starts.onchange=()=>{
    const template=connectorDefinitions.find(plugin=>plugin.id===session.connector)?.templates?.find(template=>template.id===starts.value);
    const c=template ? compactDesign(bindDesign(template.component,session.source)) : {id:component()!.id,label:'New design',rect:{x:0,y:0,width:32,height:32},padding:4,gap:2,border:false,content_layout:'flow' as const,blocks:[]};
    c.id=component()!.id; if (template) c.label=template.name;
    const startingPoint=starts.value;
    change(layout=>{session.automatic=true;session.fixedWidth=undefined;session.startingPoint=startingPoint;selectedElement=c.blocks[0]?.id??null;layout.components[0]=c;},''); renderDesignerControls();
  };
  nameInput.oninput=()=>change(layout=>{layout.components[0]!.label=nameInput.value;},'design.name',false);
}
function clippedItemNames() { const c=component(); const ids=reports.find(report=>report.id===selected)?.blocks.filter(block=>block.overflow).map(block=>block.id)??[]; return c?.blocks.filter(block=>ids.includes(block.id)).map(elementName).join(', ')||'some content'; }
function designHasOverflow() { return reports.find(report=>report.id===selected)?.warnings.includes('overflow') ?? false; }
function renderDesignerState() {
  const session=designSession; const c=component(); if (!session || !c) return;
  button('edit').hidden=true; button('refresh').hidden=true; button('discard').hidden=false; button('discard').textContent='Close designer';
  button('save').hidden=false; button('save').textContent='Save design'; button('save').classList.remove('primary');
  button('save').disabled=!!session.libraryConflict || session.busy || !session.ready || !c.label?.trim() || !designGeometryValid(c) || !validElements(c) || !c.blocks.length;
  button('design-place').hidden=false; button('design-place').textContent=session.original?'Apply to layout':'Add to display';
  const nameMissing=!c.label?.trim(); const nameTooLong=new TextEncoder().encode(c.label??'').length>128;
  $('design-name-error').textContent=nameMissing?'Enter a design name before saving.':nameTooLong?'Use a shorter design name (up to 128 bytes).':'';
  $('design-name-error').hidden=!nameMissing&&!nameTooLong; $('design-name').setAttribute('aria-invalid',String(nameMissing||nameTooLong));
  button('save').disabled ||=nameTooLong;
  const requiresSource=c.blocks.some(block=>sourceBinding(block.source));
  button('design-place').disabled=nameMissing||nameTooLong||session.busy||!session.ready||!validElements(c)||!c.blocks.length||requiresSource&&!session.source;
  $('inspector-body').inert=session.busy;
  $('designer-controls').inert=session.busy;
  $('save-state').textContent=session.busy?'Saving design…':dirty()?'Unsaved design changes':session.id?'Design saved':'New component design';
  $('draft-badge').textContent='Component design';
  $('canvas-title').textContent='Component preview';
  button('view-current').hidden=true; button('view-draft').textContent='Component preview';
  button('exit-content').hidden=true; button('use-flow').hidden=true;
  $('content-mode-bar').hidden=true;
  button('design-flow-toggle').hidden=!c.blocks.length; button('design-flow-toggle').textContent=c.content_layout==='positioned'?'Stack items':'Arrange freely';
  $('library-title').textContent='Items';
  button('designs-main').hidden=true;
  button('custom-design-new').hidden=true; button('designs-open').hidden=true;
  const needed=c.rect.width*c.rect.height/(session.previous.state.layout.display.width*session.previous.state.layout.display.height)*100;
  $('design-footprint').textContent=`${c.rect.width} × ${c.rect.height} px · ${Math.max(.1,Math.round(needed*10)/10)}% of your display`;
  $('design-feedback').textContent=session.error || (designHasOverflow()?`Clipped items: ${clippedItemNames()}. You can save this design. Use Fit to contents, or enlarge the affected items.`:requiresSource&&!session.source?'Choose a source to use live information when adding. You can save this design first.':!c.blocks.length?'Add your first item to begin.':session.ready?'Preview uses current values. Leave extra room for longer information.':'Preparing your preview…');
  $('design-feedback').classList.toggle('error',!!session.error);
  $('design-feedback').classList.toggle('clipping-warning',!session.error&&designHasOverflow());
  const width=$<HTMLInputElement>('design-width'); width.max=String(Math.max(session.previous.state.layout.display.width,c.rect.width)); width.min=String(Math.max(32,2*c.padding+1)); if (document.activeElement!==width) width.value=String(c.rect.width);
  const fonts=[...new Set(c.blocks.filter(block=>block.source.kind!=='icon').map(block=>block.font_size))]; const size=$<HTMLSelectElement>('design-text-size'); size.value=fonts.length===1?String(fonts[0]):''; size.disabled=c.content_layout==='positioned';
  button('design-library-retry').hidden=designLibraryLoaded;
  button('design-fit').disabled=session.busy||!c.blocks.length;
  if(document.activeElement!==$('design-connection'))$<HTMLSelectElement>('design-connection').value=session.source??'';
  if(document.activeElement!==$('design-starting-point'))$<HTMLSelectElement>('design-starting-point').value=session.startingPoint??'';
  if (document.activeElement!==$('design-name')) $<HTMLInputElement>('design-name').value=c.label??'';
}
function clearDesignerChrome() {
  button('refresh').hidden=false; button('discard').textContent='Discard'; button('save').textContent='Save to display'; button('save').classList.add('primary');
  button('design-place').hidden=true; $('draft-badge').textContent='Editing draft'; button('view-current').hidden=false; button('view-draft').textContent='Draft'; button('exit-content').hidden=false; button('use-flow').hidden=false; button('design-flow-toggle').hidden=true;
  button('designs-main').hidden=false; button('custom-design-new').hidden=false; button('designs-open').hidden=false; $('inspector-body').inert=false;
}
async function closeDesigner() {
  if (!designSession || designSession.busy) return;
  if (dirty() && !await confirmAction('Close without saving this design?','Unsaved design edits will be discarded. Your display layout and any previously saved design are kept.','Discard design edits')) return;
  const previous=designSession.previous; clearDesignerChrome(); restoreEditor(previous);
}
function focusDesignContent() { [...$('inspector-body').querySelectorAll<HTMLElement>('textarea,input:not([type=checkbox]),select')].find(control=>control.getClientRects().length>0&&!control.closest('details:not([open])'))?.focus({preventScroll:true}); }
function addDesignBlock(kind: 'text'|'data'|'clock') {
  const session=designSession; const c=component(); if (!session||!c||c.blocks.length>=32) return;
  if (c.content_layout==='positioned') { addElement(kind); focusDesignContent(); return; }
  const block=newBlock();
  if (kind==='clock') block.source={kind:'clock',clock:newClock('date_time')};
  if (kind==='data') { const dataset=bindingCatalog()[0]; if (!dataset) { notice('No information fields are available yet. Set up a source in Data sources, or add text and time first.'); return; } block.source={kind:'field',binding:newBinding(dataset)}; }
  selectedElement=block.id;
  change(layout=>{session.automatic=true;layout.components[0]!.blocks.push(block);}); closePicker(); focusDesignContent();
}
async function saveDesign() {
  const session=designSession; const c=component(); if (!session||!c||session.busy||session.libraryConflict||!session.ready) return;
  session.busy=true; render(false);
  try {
    if (!designLibraryLoaded) await loadDesignLibrary();
    const id=session.id??identifier('design');
    designLibrary=await api<DesignLibrary>(`/api/designs/${encodeURIComponent(id)}`,{method:'PUT',headers:{'Content-Type':'application/json'},body:JSON.stringify({base_revision:designLibrary.revision,connector_id:session.connector,name:c.label,component:c})});
    if (designSession!==session) return;
    session.id=id; session.baseline=clone(c); notice(designHasOverflow()?'Design saved. Some items are clipped; you can keep editing. Your display has not changed.':'Design saved to Paperphase. Your display has not changed.');
  } catch (error) { if (error instanceof ApiFailure && error.status===409) {designLibraryLoaded=false;session.libraryConflict=true;} notice(String(error),true); }
  finally { if (designSession===session) { session.busy=false; render(false); } }
}
function placeDesign() {
  const session=designSession; const c=component(); if (!session||!c||!c.label?.trim()||!session.ready||session.busy||c.blocks.some(block=>sourceBinding(block.source))&&!session.source) return;
  const layout=clone(session.previous.state.layout); const target=clone(c);
  const original=layout.components.find(component=>component.id===session.original);
  if (session.original) layout.components=layout.components.filter(component=>component.id!==session.original);
  let rect: Rect|null=null;
  if (original && validRect({...target.rect,x:original.rect.x,y:original.rect.y},target.id,{...layout,components:[...layout.components,target]})) rect={...target.rect,x:original.rect.x,y:original.rect.y};
  rect??=designFits(target,layout);
  if (!rect) { notice('This design does not fit a free area yet. Make it smaller, or save it and make room on your display. Your design is kept.',true); return; }
  target.rect=rect; target.id=session.original??identifier('component');
  const previous=session.previous; const originalId=session.original; clearDesignerChrome(); restoreEditor(previous);
  if (!editing) { editing=true; viewingDraft=true; undo=[]; redo=[]; draft=clone(previous.state.layout); draftSettings=clone(previous.state.settings); }
  currentPage='display'; showPage('display'); viewingDraft=true; selected=target.id; contentMode=false; selectedElement=null;
  change(layout=>{if (originalId) layout.components=layout.components.filter(component=>component.id!==originalId);layout.components.push(target);});
  notice(originalId?'Component design applied to your layout draft. Save to update the display.':'Component added to your layout draft. Save to update the display.');
}
button('design-place').onclick=placeDesign;
button('designs-main').onclick=()=>{void openDesignLibrary();};
button('designs-open').onclick=()=>{void openDesignLibrary();};
button('custom-design-new').onclick=()=>{void openDesignLibrary();};
button('design-library-back').onclick=()=>showPage('display');
button('design-library-reload').onclick=async()=>{try{await loadDesignLibrary();$('design-library-notice').textContent='Design library reloaded. Your current component edits are kept.';renderDesignLibrary();}catch(error){$('design-library-notice').textContent=String(error);}};
button('design-library-retry').onclick=async()=>{try{await loadDesignLibrary();if(designSession)designSession.libraryConflict=false;render(false);notice('Design library reloaded. You can save your draft again.');}catch(error){notice(String(error),true);}};
button('design-fit').onclick=()=>{if(designSession){designSession.recordFit=capture();designSession.automatic=true;designSession.needsFit=true;designSession.fixedWidth=undefined;queuePreview();}};
$<HTMLInputElement>('design-width').oninput=()=>{if(designSession){const width=$<HTMLInputElement>('design-width').valueAsNumber;change(layout=>{designSession!.automatic=true;designSession!.fixedWidth=width;layout.components[0]!.rect.width=width;},'design.width',false);}};
$<HTMLSelectElement>('design-text-size').onchange=()=>{if(designSession){const size=Number($<HTMLSelectElement>('design-text-size').value);if(!size)return;change(layout=>{designSession!.automatic=true;for(const block of layout.components[0]!.blocks)if(block.source.kind!=='icon')block.font_size=size;});}};
button('design-flow-toggle').onclick=()=>{if(!designSession)return;if(component()?.content_layout==='positioned'){designSession.automatic=true;useFlow();}else{designSession.automatic=false;void enterContents();}};
