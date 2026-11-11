export {};
type Rect = { x: number; y: number; width: number; height: number };
type Binding = { connection_id: string; dataset_id: string; field_id: string; row: number; decimals: number | null; prefix: string; suffix: string };
type ClockSpec = { timezone: string; content: 'time' | 'date' | 'date_time'; hour_cycle: 'twenty_four' | 'twelve'; date_format: 'long' | 'short' | 'iso' | 'weekday' };
type RefreshSettings = { quiet_hours: { timezone: string; start: string; end: string } | null };
type Source = { kind: 'literal'; text: string } | { kind: 'field'; binding: Binding } | { kind: 'clock'; clock: ClockSpec };
type Block = { id: string; source: Source; font_size: number; align: string; overflow: string };
type Component = { id: string; rect: Rect; padding: number; gap: number; border: boolean; blocks: Block[] };
type Layout = { schema_version: number; display: { id: string; width: number; height: number }; components: Component[] };
type Workspace = { revision: number; displayed_revision: number; data_revision: number; frame_id: string; clock_now: number; pending: boolean; phase: string; layout: Layout; refresh_settings: RefreshSettings; addresses: string[]; connector_execution: boolean; fixture_data: boolean };
type EditState = { layout: Layout; settings: RefreshSettings };
type Dataset = { connection_id: string; connection_label?: string; dataset_id: string; fields: { id: string; kind: string; unit?: string }[]; rows: number; stale: boolean };
type Report = { id: string; state: string; warnings: string[] };
type Preview = { png_base64: string; components: Report[] };
type SettingInput = { type: 'text'; max_bytes: number; default?: string | null } | { type: 'secret'; max_bytes: number } | { type: 'integer'; min: number; max: number; default?: number | null } | { type: 'boolean'; default?: boolean | null } | { type: 'choice'; options: { value: string; label: string }[]; default?: string | null };
type SettingDefinition = { id: string; label: string; help: string; required: boolean; input: SettingInput };
type ConnectorDefinition = { id: string; name: string; description: string; min_poll_secs: number; default_poll_secs: number; settings: SettingDefinition[] };
type FetchStatus = { status: string; can_fetch: boolean; fetch_after: number; next_fetch_at: number; last_attempt_at: number | null; last_success_at: number | null; last_error: string | null };
type Connection = { id: string; connector_id: string; label: string; enabled: boolean; poll_interval_secs: number; settings: Record<string, string | number | boolean | null>; secret_present: string[]; fetch?: FetchStatus };
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
let settingsSelected = false;
let catalog: Dataset[] = [];
let editing = false;
let viewingDraft = false;
let selected: string | null = null;
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
let drag: { id: string; startX: number; startY: number; original: Rect; last: Rect; baseline: EditState; resize: boolean; pointerId: number; invalid: boolean } | undefined;
function capture(): EditState { return { layout: clone(draft), settings: clone(draftSettings) }; }
function restore(state: EditState) { draft = clone(state.layout); draftSettings = clone(state.settings); }

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
function dirty() { return editing && (!equal(draft, saved.layout) || !equal(draftSettings, saved.refresh_settings)); }
function active() { return editing && viewingDraft && !saving; }
function component() { return draft.components.find(c => c.id === selected); }
function name(c: Component): string {
  const first = c.blocks.find(b => b.source.kind === 'literal');
  if (first?.source.kind === 'literal' && first.source.text.trim()) return first.source.text.trim().split('\n')[0]!.slice(0, 36);
  const field = c.blocks.find(b => b.source.kind === 'field');
  if (field?.source.kind === 'field') return `${field.source.binding.field_id} · ${field.source.binding.connection_id}`;
  if (c.blocks.some(b => b.source.kind === 'clock')) return 'Time & date';
  return 'Untitled text';
}
function validRect(rect: Rect, id: string, layout = draft): boolean {
  const target = layout.components.find(c => c.id === id);
  const padding = 2 * (target?.padding ?? 12);
  const status = target?.blocks.some(b => b.source.kind === 'field') ? 18 : 0;
  return Object.values(rect).every(Number.isInteger)
    && rect.x >= 0 && rect.y >= 0 && rect.width >= 32 && rect.height >= 32
    && rect.width > padding && rect.height > padding + status
    && rect.x + rect.width <= layout.display.width && rect.y + rect.height <= layout.display.height
    && !layout.components.some(c => c.id !== id && rect.x < c.rect.x + c.rect.width && rect.x + rect.width > c.rect.x && rect.y < c.rect.y + c.rect.height && rect.y + rect.height > c.rect.y);
}
function locallyValid(layout: Layout) {
  return layout.components.length <= 64 && layout.components.every(c => validRect(c.rect, c.id, layout));
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
  if (!locallyValid(draft)) {
    restore(before);
    notice('That change leaves too little space or overlaps another component. The previous position is kept.', true);
    renderInspector();
    return;
  }
  const quiet = draftSettings.quiet_hours;
  if (quiet && (!/^([01]\d|2[0-3]):[0-5]\d$/.test(quiet.start) || !/^([01]\d|2[0-3]):[0-5]\d$/.test(quiet.end) || quiet.start === quiet.end)) {
    restore(before); notice('Quiet hours need valid, distinct start and end times.', true); renderInspector(); return;
  }
  if (equal(before, capture())) return;
  remember(before, key);
  notice();
  render(rebuild);
  queuePreview();
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
    const result = await api<Preview>('/api/preview', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(draft), signal: previewAbort.signal });
    if (sequence !== previewSequence || !editing) return;
    draftImage = `data:image/png;base64,${result.png_base64}`;
    reports = result.components;
    if (viewingDraft) $<HTMLImageElement>('frame').src = draftImage;
    renderReport();
  } catch (error) {
    if (sequence === previewSequence && !(error instanceof DOMException && error.name === 'AbortError')) notice(String(error), true);
  }
}
function renderReport() {
  const report = reports.find(r => r.id === selected);
  const target = $('render-report');
  if (!target) return;
  const issues = report?.warnings.map(w => w.replaceAll('_', ' ')) ?? [];
  if (report?.state === 'stale') issues.unshift('Last good data is shown with a stale indicator.');
  if (report?.state === 'unavailable') issues.unshift('The selected data is unavailable.');
  target.textContent = issues.length ? [...new Set(issues)].join(' · ') : 'Rendered at fixed font sizes. Resize to reflow the content.';
}
function render(rebuildInspector = true) {
  const isDirty = dirty();
  button('edit').hidden = editing;
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
  $('library-description').textContent = active() ? 'Select a component to make it yours.' : 'Your saved arrangement.';
  $('canvas-title').textContent = viewingDraft ? 'Draft preview' : 'Last completed frame';
  $('canvas-caption').textContent = viewingDraft ? 'Rust-rendered pixels · Drag to move, corner to resize.' : 'Exactly the pixels sent to the simulated display.';
  $('revision-label').textContent = `Displayed ${saved.displayed_revision} / saved ${saved.revision}`;
  $('data-mode').textContent = saved.connector_execution ? (saved.fixture_data ? 'Monochrome · Connectors + fixtures' : 'Monochrome · Connector data') : 'Monochrome · Sample data';
  $('dimensions').textContent = `${draft.display.width} × ${draft.display.height} px`;
  $('keyboard-hint').textContent = active() ? 'Arrow keys: move · Shift: 8 px · Ctrl/Cmd Z: undo · Ctrl/Cmd S: save' : 'Paperphase 0.1.0 · Experimental';
  $('grid-overlay').style.display = viewingDraft && checked('grid') ? 'block' : 'none';
  if (viewingDraft && draftImage) $<HTMLImageElement>('frame').src = draftImage;
  if (!viewingDraft) setDisplayedImage();
  renderList();
  renderOverlays();
  if (rebuildInspector) renderInspector();
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
function renderList() {
  const list = $('component-list');
  list.replaceChildren();
  const layout = viewingDraft ? draft : saved.layout;
  $('component-count').textContent = String(layout.components.length);
  for (const c of layout.components) {
    const item = node('button', `component-item ${active() && selected === c.id ? 'selected' : ''}`);
    item.setAttribute('aria-label', `Select ${c.id}`);
    item.append(node('span', 'component-icon', c.blocks.some(b => b.source.kind === 'clock') ? '◷' : c.blocks.some(b => b.source.kind === 'field') ? '≋' : 'Aa'));
    const text = node('span');
    text.append(node('span', 'item-name', name(c)), node('small', '', `${c.rect.width} × ${c.rect.height} · ${c.rect.x}, ${c.rect.y}`));
    item.append(text);
    item.disabled = !active();
    item.onclick = () => { selected = c.id; settingsSelected = false; render(); };
    list.append(item);
  }
}
function renderOverlays() {
  const overlays = $('overlays');
  overlays.replaceChildren();
  if (!active()) return;
  for (const c of draft.components) {
    const box = node('div', `component-box ${selected === c.id ? 'selected' : ''} ${drag?.id === c.id && drag.invalid ? 'invalid' : ''}`);
    box.dataset.component = c.id;
    box.setAttribute('role', 'button');
    box.setAttribute('aria-pressed', String(selected === c.id));
    box.setAttribute('aria-label', `Move ${c.id}`);
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
      box.append(tag, handle);
    }
    box.onfocus = () => {
      if (selected !== c.id) {
        selected = c.id; settingsSelected = false; renderInspector(); renderList(); renderOverlays();
        for (const next of overlays.querySelectorAll<HTMLElement>('[data-component]')) if (next.dataset.component === c.id) next.focus();
      }
    };
    overlays.append(box);
  }
}
function fit() {
  if (!draft) return;
  const viewport = $('viewport');
  if (autoFit) zoom = Math.min(1, (viewport.clientWidth - 68) / draft.display.width, (viewport.clientHeight - 68) / draft.display.height);
  zoom = Math.max(.1, Math.min(2, zoom));
  Object.assign($('board').style, { width: `${draft.display.width}px`, height: `${draft.display.height}px`, transform: `scale(${zoom})` });
  Object.assign($('board-wrap').style, { width: `${draft.display.width * zoom}px`, height: `${draft.display.height * zoom}px` });
  $('zoom-label').textContent = `${Math.round(zoom * 100)}%`;
}

function label(title: string, input: HTMLElement, parent: HTMLElement) {
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
function renderInspector() {
  const inspector = $('inspector-body');
  if (lastInspectedId !== selected) {
    inspector.parentElement!.scrollTop = 0;
    lastInspectedId = selected;
  }
  inspector.replaceChildren();
  if (active() && settingsSelected) { renderSettings(inspector); return; }
  const c = component();
  if (!active() || !c) {
    const empty = node('div', 'empty-properties');
    empty.append(node('span', '', '↖'), node('h3', '', active() ? 'Start with a component.' : 'Your display, your way.'), node('p', '', active() ? 'Select one on the canvas, or add text and data from the library.' : 'Choose Edit layout to arrange components and customise their content.'));
    inspector.append(empty); return;
  }
  const id = c.id;
  const updateComponent = (fn: (c: Component) => void, key = '', rebuild = true) => change(l => fn(l.components.find(c => c.id === id)!), key, rebuild);
  const placement = section('PLACEMENT · NATIVE PIXELS', inspector);
  const geometry = node('div', 'field-grid'); placement.append(geometry);
  for (const [key, title, min, max] of [['x', 'X', 0, draft.display.width - c.rect.width], ['y', 'Y', 0, draft.display.height - c.rect.height], ['width', 'Width', 32, draft.display.width], ['height', 'Height', 32, draft.display.height]] as const) {
    numeric(title, c.rect[key], min, max, geometry, n => updateComponent(c => { c.rect[key] = n; }));
  }
  const spacing = node('div', 'field-grid'); placement.append(spacing);
  numeric('Padding', c.padding, 0, 64, spacing, n => updateComponent(c => { c.padding = n; }));
  numeric('Block gap', c.gap, 0, 64, spacing, n => updateComponent(c => { c.gap = n; }));
  const borderLabel = node('label', 'check'); const border = node('input'); border.type = 'checkbox'; border.checked = c.border;
  border.onchange = () => updateComponent(c => { c.border = border.checked; }); borderLabel.append(border, document.createTextNode('Draw component border')); placement.append(borderLabel);
  const content = section('CONTENT · FLOWS FROM TOP TO BOTTOM', inspector);
  c.blocks.forEach((block, index) => {
    const updateBlock = (fn: (b: Block) => void, key = '', rebuild = true) => updateComponent(c => fn(c.blocks.find(b => b.id === block.id)!), key, rebuild);
    const card = node('div', 'block-card'); card.dataset.block = block.id; content.append(card);
    const heading = node('div', 'block-heading'); heading.append(node('strong', '', `Block ${index + 1}`)); card.append(heading);
    const move = (direction: number) => updateComponent(c => { const i = c.blocks.findIndex(b => b.id === block.id); [c.blocks[i], c.blocks[i + direction]] = [c.blocks[i + direction]!, c.blocks[i]!]; });
    action('↑', 'Move block up', heading, () => move(-1), index === 0);
    action('↓', 'Move block down', heading, () => move(1), index === c.blocks.length - 1);
    action('×', 'Remove block', heading, () => updateComponent(c => { c.blocks = c.blocks.filter(b => b.id !== block.id); }), c.blocks.length === 1);
    select('Content source', block.source.kind, [['literal', 'Text'], ['field', 'Dataset field'], ['clock', 'Time & date']], card, kind => updateBlock(b => {
      if (kind === 'literal') b.source = { kind: 'literal', text: 'Your text' };
      else if (kind === 'clock') b.source = { kind: 'clock', clock: newClock('time') };
      else if (catalog[0]) b.source = { kind: 'field', binding: newBinding(catalog[0]) };
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
    } else {
      const binding = block.source.binding;
      const updateBinding = (fn: (b: Binding) => void, key = '', rebuild = true) => updateBlock(b => { if (b.source.kind === 'field') fn(b.source.binding); }, key, rebuild);
      const key = `${binding.connection_id}/${binding.dataset_id}`;
      const dataset = catalog.find(d => `${d.connection_id}/${d.dataset_id}` === key);
      select('Dataset', key, catalog.map(d => [`${d.connection_id}/${d.dataset_id}`, `${d.connection_label ?? d.connection_id} / ${d.dataset_id}`]), card, value => {
        const d = catalog.find(d => `${d.connection_id}/${d.dataset_id}` === value)!;
        updateBlock(b => { b.source = { kind: 'field', binding: newBinding(d) }; });
      });
      select('Field', binding.field_id, dataset?.fields.map(f => [f.id, `${f.id} · ${f.kind}`]) ?? [], card, value => updateBinding(b => { b.field_id = value; b.decimals = null; }));
      numeric('Row (first row = 0)', binding.row ?? 0, 0, 511, card, n => updateBinding(b => { b.row = n; }));
      if (dataset?.fields.find(f => f.id === binding.field_id)?.kind === 'number') {
        select('Decimal places', binding.decimals == null ? '' : String(binding.decimals), [['', 'Automatic'], ...Array.from({ length: 7 }, (_, i): [string, string] => [String(i), String(i)])], card, value => updateBinding(b => { b.decimals = value === '' ? null : Number(value); }));
      }
      textField('Prefix', binding.prefix ?? '', card, value => updateBinding(b => { b.prefix = value; }, `${id}.${block.id}.prefix`, false));
      textField('Suffix', binding.suffix ?? '', card, value => updateBinding(b => { b.suffix = value; }, `${id}.${block.id}.suffix`, false));
    }
    const typography = node('div', 'field-grid'); card.append(typography);
    numeric('Font size (px)', block.font_size, 8, 256, typography, value => updateBlock(b => { b.font_size = value; }));
    select('Alignment', block.align ?? 'left', [['left', 'Left'], ['center', 'Centre'], ['right', 'Right']], typography, value => updateBlock(b => { b.align = value; }));
    select('Overflow', block.overflow ?? 'wrap', [['wrap', 'Wrap'], ['clip', 'Clip'], ['ellipsis', 'Ellipsis']], card, value => updateBlock(b => { b.overflow = value; }));
  });
  action('+ Add text block', 'Add text block', content, () => updateComponent(c => { c.blocks.push(newBlock()); }), c.blocks.length >= 32).className = 'wide-button';
  const report = node('p', 'render-report'); report.id = 'render-report'; content.append(report); renderReport();
  const remove = section('COMPONENT', inspector);
  action('Remove component', 'Remove component', remove, () => { change(l => { l.components = l.components.filter(c => c.id !== id); }); selected = null; render(); }).className = 'danger wide-button';
}
function newBinding(dataset: Dataset): Binding { return { connection_id: dataset.connection_id, dataset_id: dataset.dataset_id, field_id: dataset.fields[0]!.id, row: 0, decimals: null, prefix: '', suffix: '' }; }
// getRandomValues also works over plain HTTP on a LAN; randomUUID requires HTTPS.
function identifier(prefix: string) { return `${prefix}-${Array.from(crypto.getRandomValues(new Uint8Array(8)), n => n.toString(16).padStart(2, '0')).join('')}`; }
function newBlock(): Block { return { id: identifier('block'), source: { kind: 'literal', text: 'Your text' }, font_size: 20, align: 'left', overflow: 'wrap' }; }
function newClock(content: ClockSpec['content']): ClockSpec { return { timezone: 'Europe/Dublin', content, hour_cycle: 'twenty_four', date_format: 'short' }; }
function renderSettings(parent: HTMLElement) {
  const panel = section('DISPLAY SETTINGS', parent);
  const toggle = node('label', 'check');
  const input = node('input'); input.type = 'checkbox'; input.checked = !!draftSettings.quiet_hours;
  input.onchange = () => change(() => { draftSettings.quiet_hours = input.checked ? { timezone: 'Europe/Dublin', start: '22:00', end: '07:00' } : null; });
  toggle.append(input, document.createTextNode('Enable quiet hours')); panel.append(toggle);
  panel.append(node('p', 'render-report', 'During quiet hours the displayed frame stays still. The clock continues; the latest saved layout is rendered when quiet hours end.'));
  const quiet = draftSettings.quiet_hours;
  if (quiet) {
    select('Quiet hours timezone', quiet.timezone, zones.map(zone => [zone, zone]), panel, value => change(() => { draftSettings.quiet_hours!.timezone = value; }));
    for (const [key, title] of [['start', 'Pause at'], ['end', 'Resume at']] as const) {
      const time = node('input'); time.type = 'time'; time.value = quiet[key]; time.step = '60';
      time.onchange = () => change(() => { draftSettings.quiet_hours![key] = time.value; });
      label(title, time, panel);
    }
  }
  panel.append(node('p', 'render-report', 'Use Save to apply these settings, or Discard to restore them. Refresh display also respects quiet hours.'));
}
function addComponent(kind: 'text' | 'data' | 'clock') {
  const block = newBlock();
  const blocks = [block];
  if (kind === 'clock') {
    block.source = { kind: 'clock', clock: newClock('time') }; block.font_size = 40;
    const date = newBlock(); date.source = { kind: 'clock', clock: newClock('date') }; date.font_size = 14; blocks.push(date);
  } else if (kind === 'data') {
    if (!catalog[0]) { notice('No datasets are available. Start with a text component.'); return; }
    block.source = { kind: 'field', binding: newBinding(catalog[0]) };
  }
  const c: Component = { id: identifier('component'), rect: { x: 0, y: 0, width: 256, height: 128 }, padding: 12, gap: 8, border: false, blocks };
  for (let y = 16; y + c.rect.height <= draft.display.height; y += 8) {
    for (let x = 16; x + c.rect.width <= draft.display.width; x += 8) {
      c.rect.x = x; c.rect.y = y;
      if (validRect(c.rect, c.id)) { selected = c.id; settingsSelected = false; change(l => { l.components.push(c); }); return; }
    }
  }
  notice('No free 256 × 128 area is available. Resize or remove a component to make room.');
}

let lastPointerX = 0;
let lastPointerY = 0;
function proposedRect(state: NonNullable<typeof drag>, x: number, y: number): Rect {
  const snap = (n: number) => Math.round(n / (checked('snap') ? 8 : 1)) * (checked('snap') ? 8 : 1);
  const dx = (x - state.startX) / zoom, dy = (y - state.startY) / zoom;
  const rect = { ...state.original };
  if (state.resize) { rect.width = Math.max(32, snap(rect.width + dx)); rect.height = Math.max(32, snap(rect.height + dy)); }
  else { rect.x = snap(rect.x + dx); rect.y = snap(rect.y + dy); }
  return rect;
}
$('board').addEventListener('pointerdown', event => {
  if (!active() || event.button !== 0 || drag) return;
  const target = event.target as HTMLElement;
  const box = target.closest<HTMLElement>('[data-component]');
  if (!box) { selected = null; render(); return; }
  event.preventDefault();
  selected = box.dataset.component!;
  settingsSelected = false;
  const c = component()!;
  drag = { id: c.id, startX: event.clientX, startY: event.clientY, original: { ...c.rect }, last: { ...c.rect }, baseline: capture(), resize: !!target.closest('[data-resize]'), pointerId: event.pointerId, invalid: false };
  lastPointerX = event.clientX; lastPointerY = event.clientY;
  $('board').setPointerCapture(event.pointerId);
  document.body.classList.add('dragging');
  coalescedKey = '';
  render();
});
$('board').addEventListener('pointermove', event => {
  if (!drag || event.pointerId !== drag.pointerId) return;
  lastPointerX = event.clientX; lastPointerY = event.clientY;
  const rect = proposedRect(drag, event.clientX, event.clientY);
  drag.invalid = !validRect(rect, drag.id);
  if (!drag.invalid) { drag.last = rect; component()!.rect = rect; queuePreview(); }
  renderOverlays();
});
function endDrag(cancel = false) {
  if (!drag) return;
  const state = drag;
  drag = undefined;
  if (cancel) restore(state.baseline);
  else {
    component()!.rect = state.last;
    if (!equal(state.baseline, capture())) remember(state.baseline);
    if (state.invalid) notice('Components cannot overlap or leave the display. Restored the last valid position.');
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
  notice(); render(); queuePreview();
}
async function save() {
  if (!dirty() || saving || conflict || !connected || drag) return;
  saving = true; render(false); notice();
  try {
    const result = await api<{ revision: number; phase: string }>('/api/layout', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ base_revision: saved.revision, layout: draft, refresh_settings: draftSettings }) });
    saved = { ...saved, revision: result.revision, layout: clone(draft), refresh_settings: clone(draftSettings), phase: result.phase };
    editing = false; viewingDraft = false; selected = null; undo = []; redo = [];
    ++previewSequence; previewAbort?.abort(); window.clearTimeout(previewTimer);
    notice(result.phase === 'paused' ? 'Saved. Display updates are paused for quiet hours.' : result.phase === 'pending' ? 'Saved. A display update is pending.' : result.phase === 'refreshing' ? 'Saved. The simulator is refreshing.' : 'Saved. The display is already up to date.');
    displayNoticePending = result.phase !== 'idle';
  } catch (error) { notice(String(error), true); }
  finally { saving = false; render(); }
}
button('edit').onclick = () => {
  editing = true; viewingDraft = true; draft = clone(saved.layout); draftSettings = clone(saved.refresh_settings); settingsSelected = false; draftImage = $<HTMLImageElement>('frame').src;
  undo = []; redo = []; selected = null; conflict = false; notice(); render(); queuePreview();
};
button('discard').onclick = async () => {
  if (drag) endDrag(true);
  try {
    const current = await api<Workspace>('/api/state');
    saved = current; draft = clone(current.layout); draftSettings = clone(current.refresh_settings); settingsSelected = false; editing = false; viewingDraft = false; selected = null; conflict = false;
    ++previewSequence; previewAbort?.abort(); window.clearTimeout(previewTimer); notice(); render(); updateStatus();
  } catch (error) { notice(String(error), true); }
};
button('save').onclick = () => { void save(); };
button('view-current').onclick = () => { if (drag) endDrag(); viewingDraft = false; render(); };
button('view-draft').onclick = () => { viewingDraft = true; render(); };
button('undo').onclick = () => history('undo'); button('redo').onclick = () => history('redo');
button('add-text').onclick = () => addComponent('text'); button('add-data').onclick = () => addComponent('data'); button('add-clock').onclick = () => addComponent('clock');
button('display-settings').onclick = () => { settingsSelected = true; selected = null; render(); };
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
window.addEventListener('beforeunload', event => { if (dirty() || connectionDirty()) { event.preventDefault(); event.returnValue = ''; } });
document.addEventListener('keydown', event => {
  if ($<HTMLDialogElement>('connections-dialog').open) return;
  const typing = (event.target as HTMLElement).closest('input,textarea,select,[contenteditable=true]');
  const mod = event.ctrlKey || event.metaKey;
  if (mod && event.key.toLowerCase() === 's') { event.preventDefault(); void save(); return; }
  if (!active()) return;
  if (event.key === 'Escape') { if (drag) endDrag(true); else { selected = null; settingsSelected = false; render(); } return; }
  if (typing || drag) return;
  if (mod && event.key.toLowerCase() === 'z') { event.preventDefault(); history(event.shiftKey ? 'redo' : 'undo'); return; }
  const c = component();
  if (!c) return;
  if (event.key === 'Delete' || event.key === 'Backspace') { event.preventDefault(); const id = c.id; change(l => { l.components = l.components.filter(c => c.id !== id); }); selected = null; render(); return; }
  const delta: Record<string, [number, number]> = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
  const movement = delta[event.key];
  if (movement) {
    event.preventDefault(); const step = event.shiftKey ? 8 : 1;
    change(l => { const target = l.components.find(c => c.id === selected)!; target.rect.x += movement[0] * step; target.rect.y += movement[1] * step; }, `move.${c.id}`);
  }
});
function updateStatus() {
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
      const changedSchema = JSON.stringify(updated.map(d => [d.connection_id, d.connection_label, d.dataset_id, d.fields])) !== JSON.stringify(catalog.map(d => [d.connection_id, d.connection_label, d.dataset_id, d.fields]));
      catalog = updated;
      if (editing && viewingDraft) queuePreview();
      if (changedSchema && active() && !document.activeElement?.closest('input,textarea,select')) renderInspector();
    }
    const changed = !connected || current.revision !== saved.revision || current.frame_id !== saved.frame_id || current.displayed_revision !== saved.displayed_revision || current.phase !== saved.phase;
    connected = true;
    if (editing) {
      if (current.revision !== saved.revision && !conflict) { conflict = true; notice('A newer layout was saved in another tab. Your draft is retained. Discard to load the newer layout.', true); }
      saved.phase = current.phase; saved.displayed_revision = current.displayed_revision; saved.frame_id = current.frame_id; saved.clock_now = current.clock_now; saved.data_revision = current.data_revision;
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
async function start() {
  try {
    [saved, catalog, zones] = await Promise.all([api<Workspace>('/api/state'), api<Dataset[]>('/api/catalog'), api<string[]>('/api/timezones')]);
    draft = clone(saved.layout); draftSettings = clone(saved.refresh_settings); updateStatus(); render();
    window.setInterval(() => { void poll(); }, 1000);
  } catch (error) { notice(`Cannot load the workspace. ${String(error)} Reload after the service is available.`, true); button('edit').disabled = true; }
}
void start();

let connectorDefinitions: ConnectorDefinition[] = [];
let connectionStore: Connections = { revision: 0, connections: [], execution: 'settings_only' };
let connectionDraft: Connection | undefined;
let connectionBaseline = '';
let connectionBusy = false;
let connectionConflict = false;
let connectionIsNew = false;
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
  $('connection-properties').inert = connectionBusy;
  $('connections-list').inert = connectionBusy;
}
async function loadConnections() {
  const [catalog, store] = await Promise.all([api<{ connectors: ConnectorDefinition[] }>('/api/connectors'), api<Connections>('/api/connections')]);
  connectorDefinitions = catalog.connectors; connectionStore = store;
  document.querySelector('.connections-scope')!.textContent = store.execution === 'compiled' ? 'Fetch now updates connection data. Refresh display applies the latest saved layout and data, subject to quiet hours.' : 'Settings only in this release. Connections do not fetch data or change the sample display yet.';
  connectionDraft = undefined; connectionBaseline = ''; connectionConflict = false; connectionIsNew = false;
  renderConnections();
}
function beginConnection(connection?: Connection) {
  if (connectionDraft || connectionBusy) return;
  const definition = connectorDefinitions[0];
  if (!connection && !definition) return;
  connectionIsNew = !connection;
  connectionDraft = connection ? clone(connection) : newConnection(definition!);
  // Treat setting IDs as literal keys, including names inherited by plain objects.
  connectionDraft.settings = Object.assign(Object.create(null), connectionDraft.settings);
  connectionBaseline = connection ? JSON.stringify(connectionDraft) : '';
  connectionConflict = false; connectionNotice(); renderConnections();
}
function newConnection(definition: ConnectorDefinition): Connection {
  const settings: Connection['settings'] = Object.create(null);
  for (const field of definition.settings) if ('default' in field.input && field.input.default != null) settings[field.id] = field.input.default;
  return { id: identifier('connection'), connector_id: definition.id, label: definition.name, enabled: true, poll_interval_secs: definition.default_poll_secs, settings, secret_present: [] };
}
function renderConnections() {
  const list = $('connections-list'); list.replaceChildren();
  for (const connection of connectionStore.connections) {
    const item = node('button', `connection-item ${connectionDraft?.id === connection.id ? 'selected' : ''}`);
    item.setAttribute('aria-label', `Configure ${connection.label}`);
    item.append(node('strong', '', connection.label), node('small', '', `${connection.fetch?.status.replaceAll('_', ' ') ?? (connection.enabled ? 'Configured' : 'Disabled')} · ${connection.poll_interval_secs} s`));
    item.disabled = !!connectionDraft;
    item.onclick = () => beginConnection(connection);
    list.append(item);
    if (connection.fetch && connection.fetch.status !== 'settings_only') {
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
    const empty = node('div', 'empty-properties');
    empty.append(node('span', '', '≋'), node('h3', '', connectionStore.connections.length ? 'Choose a connection.' : 'Start with a data source.'), node('p', '', 'Keep each location or account in its own connection. The settings form comes from its connector definition.'));
    parent.append(empty); connectionButtons(); return;
  }
  const definition = connectorDefinitions.find(d => d.id === current.connector_id)!;
  const form = node('form', 'connection-form'); form.id = 'connection-form'; parent.append(form);
  const fields = node('div', 'connection-fields'); form.append(fields);
  fields.append(node('h3', '', connectionIsNew ? 'New connection' : current.label), node('p', 'connection-description', definition.description));
  if (connectionIsNew) select('Connector', definition.id, connectorDefinitions.map(d => [d.id, d.name]), fields, value => {
    const next = newConnection(connectorDefinitions.find(d => d.id === value)!); next.id = current.id;
    connectionDraft = next; renderConnections();
  });
  const labelInput = node('input'); labelInput.value = current.label; labelInput.required = true; labelInput.maxLength = 96;
  labelInput.oninput = () => { current.label = labelInput.value; connectionButtons(); };
  label('Connection name', labelInput, fields);
  const enabledLabel = node('label', 'check'); const enabled = node('input'); enabled.type = 'checkbox'; enabled.checked = current.enabled;
  enabled.onchange = () => { current.enabled = enabled.checked; connectionButtons(); }; enabledLabel.append(enabled, document.createTextNode('Enabled')); fields.append(enabledLabel);
  const interval = node('input'); interval.type = 'number'; interval.min = String(definition.min_poll_secs); interval.max = '86400'; interval.step = '1'; interval.required = true; interval.value = String(current.poll_interval_secs);
  interval.oninput = () => { current.poll_interval_secs = interval.valueAsNumber; connectionButtons(); };
  label('Polling interval (seconds)', interval, fields);
  for (const field of definition.settings) {
    const inputDefinition = field.input;
    const value = current.settings[field.id];
    let input: HTMLInputElement | HTMLSelectElement;
    if (inputDefinition.type === 'choice') {
      input = node('select');
      if (!field.required) { const option = node('option', '', 'Not set'); option.value = ''; input.append(option); }
      for (const choice of inputDefinition.options) { const option = node('option', '', choice.label); option.value = choice.value; input.append(option); }
      input.value = typeof value === 'string' ? value : '';
      input.onchange = () => { current.settings[field.id] = input.value === '' ? null : input.value; connectionButtons(); };
      input.required = field.required;
    } else {
      input = node('input');
      if (inputDefinition.type === 'boolean') {
        input.type = 'checkbox'; input.checked = value === true;
        input.onchange = () => { current.settings[field.id] = (input as HTMLInputElement).checked; connectionButtons(); };
      } else if (inputDefinition.type === 'integer') {
        input.type = 'number'; input.min = String(inputDefinition.min); input.max = String(inputDefinition.max); input.step = '1'; input.required = field.required; input.value = typeof value === 'number' ? String(value) : '';
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
        const group = node('div', 'secret-setting'); label(field.label, input, group);
        const clearLabel = node('label', 'check'); clearLabel.append(clear, document.createTextNode(`Clear ${field.label}`)); group.append(clearLabel);
        group.append(node('small', 'field-help', `${present ? 'A value is saved. ' : ''}${field.help}`)); fields.append(group);
        continue;
      } else {
        input.type = 'text'; input.maxLength = inputDefinition.max_bytes; input.required = field.required; input.value = typeof value === 'string' ? value : '';
        input.oninput = () => { current.settings[field.id] = input.value; connectionButtons(); };
      }
    }
    label(field.label, input, fields);
    if (inputDefinition.type === 'boolean') input.parentElement!.classList.add('boolean-setting');
    if (field.help) fields.append(node('small', 'field-help', field.help));
  }
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
    remove.onclick = () => { void writeConnection(true); }; actions.prepend(remove);
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
  details.textContent = status.status === 'fetching' ? 'Fetching…' : status.last_error ?? (status.last_success_at ? `Last fetched ${new Date(status.last_success_at * 1000).toLocaleTimeString()}` : 'Waiting for the first fetch.');
  if (!status.can_fetch && status.fetch_after > 0) fetch.title = `Next eligible: ${new Date(status.fetch_after * 1000).toLocaleString()}`;
}
async function refreshConnectionStatus() {
  if (!connectionDialog().open || connectionBusy) return;
  try {
    const current = await api<Connections>('/api/connections');
    if (!connectionDialog().open || connectionBusy) return;
    if (connectionDraft) {
      // Configuration and password inputs stay untouched while status changes.
      for (const container of $('connections-list').querySelectorAll<HTMLElement>('[data-fetch-id]')) {
        const connection = current.connections.find(c => c.id === container.dataset.fetchId);
        if (connection) updateFetchControl(container, connection);
      }
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
button('connections-open').onclick = async () => {
  connectionDialog().showModal(); connectionNotice('Loading connections…'); connectionBusy = true; connectionButtons();
  try { await loadConnections(); connectionNotice(); } catch (error) { connectionNotice(String(error), true); }
  finally { connectionBusy = false; connectionButtons(); }
};
button('connection-new').onclick = () => beginConnection();
function closeConnections(event?: Event) {
  if (connectionBusy || connectionDirty()) { event?.preventDefault(); connectionNotice('Save or discard your form before closing connections.'); return; }
  connectionDraft = undefined; connectionBaseline = ''; $('connection-properties').replaceChildren(); connectionDialog().close();
  if (active()) renderInspector();
}
button('connections-close').onclick = () => closeConnections();
connectionDialog().addEventListener('cancel', event => closeConnections(event));
connectionDialog().addEventListener('keydown', event => {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') {
    event.preventDefault();
    if (!connectionBusy && !connectionConflict && (connectionIsNew || connectionDirty())) $<HTMLFormElement>('connection-form')?.requestSubmit();
  }
});
window.setInterval(() => { void refreshConnectionStatus(); }, 1000);
