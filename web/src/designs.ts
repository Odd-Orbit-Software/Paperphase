import type { Component, Layout, Rect } from './model';
import { sourceBinding } from './model';
import { validElements } from './elements';

export type SavedDesign = { id: string; connector_id: string; name: string; component: Component };
export type DesignLibrary = { schema_version: number; revision: number; designs: SavedDesign[] };
export const builtinDesigns = 'paperphase.builtin';

export function bindDesign(component: Component, connection: string | undefined): Component {
  const result = structuredClone(component);
  result.rect.x = 0; result.rect.y = 0;
  for (const block of result.blocks) {
    const binding = sourceBinding(block.source);
    if (binding) binding.connection_id = connection ?? 'design.source';
  }
  return result;
}
export function compactDesign(component: Component, fontSize = 14): Component {
  const result = structuredClone(component);
  result.padding = 4; result.gap = 2; result.content_layout = 'flow';
  result.rect.x = 0; result.rect.y = 0;
  for (const block of result.blocks) {
    delete block.rect; block.font_size = fontSize;
    if (block.source.kind === 'icon') block.source.size = 24;
    block.overflow = 'wrap';
  }
  return result;
}
export function designGeometryValid(component: Component): boolean {
  const {width,height,x,y} = component.rect;
  const padding = component.padding * 2;
  const status = component.blocks.some(block => sourceBinding(block.source)) ? 18 : 0;
  return [x,y,width,height].every(Number.isInteger) && x === 0 && y === 0 && width >= 32 && height >= 32
    && width <= 4096 && height <= 4096 && width * height <= 4194304 && width > padding && height > padding + status;
}
export function designFits(component: Component, layout: Layout): Rect | null {
  if (!designGeometryValid({...component,rect:{...component.rect,x:0,y:0}}) || !validElements(component) || !component.blocks.length) return null;
  const {width,height} = component.rect;
  if (width > layout.display.width || height > layout.display.height || layout.components.length >= 64) return null;
  // Any free axis-aligned rectangle can slide to the display edge or the edge
  // of a neighbour. This finds tight placements without eight-pixel dead zones.
  const xs = [...new Set([0,...layout.components.map(c=>c.rect.x+c.rect.width)])].sort((a,b)=>a-b);
  const ys = [...new Set([0,...layout.components.map(c=>c.rect.y+c.rect.height)])].sort((a,b)=>a-b);
  for (const y of ys) for (const x of xs) {
    if (x+width>layout.display.width || y+height>layout.display.height) continue;
    if (!layout.components.some(c=>x<c.rect.x+c.rect.width&&x+width>c.rect.x&&y<c.rect.y+c.rect.height&&y+height>c.rect.y)) return {x,y,width,height};
  }
  return null;
}
export function friendlyField(id: string) { return id.replaceAll(/[-_]/g,' ').replace(/^./, c=>c.toUpperCase()); }
