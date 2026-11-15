import type { Component } from './model';
import { validElements } from './elements';

export function fitsArea(item: Component, area: Component): boolean {
  return !item.rotation && item.rect.width <= area.rect.width && item.rect.height <= area.rect.height;
}
export function validRotation(area: Component): boolean {
  const r = area.rotation;
  if (!r) return validElements(area);
  return area.blocks.length === 0 && area.padding === 0 && area.gap === 0 && !area.border
    && (area.content_layout ?? 'flow') === 'flow' && r.members.length >= 1 && r.members.length <= 8
    && Number.isInteger(r.interval_secs) && r.interval_secs >= 5 && r.interval_secs <= 86400
    && new Set(r.members.map(item => item.id)).size === r.members.length
    && (!r.paused_member || r.members.some(item => item.id === r.paused_member))
    && r.members.every(item => fitsArea(item, area) && item.rect.x === 0 && item.rect.y === 0 && validElements(item));
}
export function localItem(item: Component, id: string): Component {
  const copy = structuredClone(item); copy.id = id; copy.rect.x = 0; copy.rect.y = 0; return copy;
}
export function rotatingArea(item: Component, memberId: string): Component {
  return { id:item.id, label:`${item.label || 'Component'} in turns`, rect:{...item.rect}, padding:0, gap:0,
    border:false, content_layout:'flow', blocks:[], refresh_hint:'automatic',
    rotation:{interval_secs:300,paused:false,members:[localItem(item,memberId)]} };
}
