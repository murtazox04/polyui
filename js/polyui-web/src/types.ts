export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

export type Props = { readonly [key: string]: Json | undefined };

export interface A11y {
  readonly role?: string;
  readonly label?: string;
}

export interface PolyuiCore {
  receive(message: string): string;
  setValue(surface: string, path: string, value: string): string;
  fire(surface: string, component: string, prop: string): string | null;
  confirm(surface: string, actionId: string, nonce: string): string;
  say(surface: string): string;
  matchVoice(surface: string, text: string): string | null;
}

export interface CreateOp {
  readonly op: "create";
  readonly id: string;
  readonly type: string;
  readonly props?: Props;
  readonly a11y?: A11y;
  readonly value?: Json;
}

export interface UpdateOp {
  readonly op: "update";
  readonly id: string;
  readonly props?: Props;
  readonly a11y?: A11y;
  readonly value?: Json;
}

export interface ChildrenOp {
  readonly op: "children";
  readonly id: string;
  readonly children: readonly string[];
}

export interface RemoveOp {
  readonly op: "remove";
  readonly id: string;
}

export interface RootOp {
  readonly op: "root";
  readonly id: string;
}

export interface ConfirmOp {
  readonly op: "confirm";
  readonly action_id: string;
  readonly nonce: string;
  readonly title: string;
  readonly body?: string;
  readonly confirm_label?: string;
  readonly expires_ms?: number;
}

export interface HighlightOp {
  readonly op: "highlight";
  readonly id: string | null;
}

export interface CloseOp {
  readonly op: "close";
}

export type RenderOp =
  | CreateOp
  | UpdateOp
  | ChildrenOp
  | RemoveOp
  | RootOp
  | ConfirmOp
  | HighlightOp
  | CloseOp;

export interface RenderBatch {
  readonly surface: string;
  readonly ops: readonly RenderOp[];
  readonly fallback?: string;
  readonly send?: readonly (string | { readonly [key: string]: Json })[];
}

export interface Utterance {
  readonly id: string;
  readonly text: string;
}

export interface VoiceMatch {
  readonly component: string;
  readonly option?: string | null;
  readonly batch?: RenderBatch | null;
  readonly action?: Json;
}

export type ActionSource = "tap" | "voice" | "keyboard";

export interface ComponentApi {
  readonly surface: string;
  readonly id: string;
  readonly type: string;
  setValue(path: string, value: Json): void;
  fire(prop: string): void;
  safeUrl(raw: unknown): string | null;
}

export interface ComponentState {
  readonly props: Props;
  readonly a11y: A11y | undefined;
  readonly value: Json | undefined;
}

export interface ComponentView {
  readonly element: Element;
  update(state: ComponentState): void;
  setChildren?(children: readonly Element[]): void;
  destroy?(): void;
}

export type ComponentRenderer = (api: ComponentApi) => ComponentView;

export interface Labels {
  readonly cancel: string;
  readonly expired: string;
  readonly chartUnavailable: string;
}

export interface RendererOptions {
  readonly core: PolyuiCore;
  readonly send: (message: string) => void;
  readonly container?: Element;
  readonly sync?: boolean;
  readonly allowHttp?: boolean;
  readonly confirmDelayMs?: number;
  readonly labels?: Partial<Labels>;
}

export type ChartColor = string;

export interface ChartViewBox {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

export interface ChartRect {
  readonly type: "rect";
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
  readonly fill: ChartColor;
}

export interface ChartLine {
  readonly type: "line";
  readonly x1: number;
  readonly y1: number;
  readonly x2: number;
  readonly y2: number;
  readonly stroke: ChartColor;
  readonly stroke_width: number;
}

export type ChartPathCommand =
  | { readonly op: "M"; readonly x: number; readonly y: number }
  | { readonly op: "L"; readonly x: number; readonly y: number }
  | { readonly op: "A"; readonly radius: number; readonly large_arc: boolean; readonly clockwise: boolean; readonly x: number; readonly y: number }
  | { readonly op: "Z" };

export interface ChartPath {
  readonly type: "path";
  readonly commands: readonly ChartPathCommand[];
  readonly fill?: ChartColor;
  readonly fill_opacity?: number;
  readonly stroke?: ChartColor;
  readonly stroke_width?: number;
}

export interface ChartCircle {
  readonly type: "circle";
  readonly cx: number;
  readonly cy: number;
  readonly r: number;
  readonly fill: ChartColor;
}

export interface ChartText {
  readonly type: "text";
  readonly x: number;
  readonly y: number;
  readonly text: string;
  readonly anchor: "start" | "middle" | "end";
  readonly role: "title" | "axis_title" | "tick" | "legend" | "value" | "note";
  readonly color: ChartColor;
  readonly size: number;
}

export type ChartMark = ChartRect | ChartLine | ChartPath | ChartCircle | ChartText;

export type ChartHit =
  | { readonly shape: "rect"; readonly row: number; readonly label: string; readonly series?: string; readonly x: number; readonly y: number; readonly width: number; readonly height: number }
  | { readonly shape: "polygon"; readonly row: number; readonly label: string; readonly series?: string; readonly points: readonly (readonly [number, number])[] };

export interface ChartLegendItem {
  readonly label: string;
  readonly color: ChartColor;
}

export interface ChartScene {
  readonly view_box: ChartViewBox;
  readonly kind?: string;
  readonly locale?: string;
  readonly title?: string;
  readonly description?: string;
  readonly marks: readonly ChartMark[];
  readonly legend?: readonly ChartLegendItem[];
  readonly hits?: readonly ChartHit[];
  readonly skipped?: number;
}
