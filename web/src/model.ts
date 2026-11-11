export type Rect = { x: number; y: number; width: number; height: number };
export type Binding = { connection_id: string; dataset_id: string; field_id: string; row: number; decimals: number | null; prefix: string; suffix: string };
export type ClockSpec = { timezone: string; content: 'time' | 'date' | 'date_time'; hour_cycle: 'twenty_four' | 'twelve'; date_format: 'long' | 'short' | 'iso' | 'weekday' };
export type Source = { kind: 'literal'; text: string } | { kind: 'field'; binding: Binding } | { kind: 'clock'; clock: ClockSpec };
export type Block = { id: string; source: Source; font_size: number; align: string; overflow: string; rect?: Rect };
export type Component = { id: string; rect: Rect; padding: number; gap: number; border: boolean; content_layout?: 'flow' | 'positioned'; blocks: Block[] };
export type Layout = { schema_version: number; display: { id: string; width: number; height: number }; components: Component[] };
export type BlockPlacement = { id: string; rect: Rect | null };
