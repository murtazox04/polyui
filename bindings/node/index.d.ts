export type Json = Record<string, unknown>
export type Payload = Json | string
export type Operation = 'show' | 'update' | 'close'
export type Resolver = (user: unknown, source: string, key: string) => unknown
export type IdFactory = () => string

export interface Problem {
  at: string
  message: string
  field?: string
  list?: string
  index?: number
  id?: string
  prop?: string
  path?: string
}

export interface SealedAction {
  action_id: string
  component: string
  prop: string
  action: string
  args: Json
  with: string[]
  requires_confirm: boolean
}

export interface Component {
  id: string
  type: string
  props?: Json
  children?: string[]
  alt?: string
  say?: string
  voice?: { ordinal?: number; synonyms?: string[] }
}

export interface PropSpec {
  kind: string
  required?: boolean
  default?: unknown
  bindable?: boolean
  description?: string
  min?: number
  max?: number
  values?: string[]
  schemes?: string[]
  of?: PropSpec
  fields?: Record<string, PropSpec>
}

export interface ActionSpec {
  args?: Record<string, PropSpec>
  requires_confirm?: boolean
  description?: string
}

export interface SurfaceJSON {
  id: string | null
  root: string
  components: Component[]
  data: Json
}

export interface Tool {
  name: string
  description: string
  input_schema: Json
  strict?: boolean
}

export interface ValidatorOptions {
  actions?: Record<string, ActionSpec>
  refSources?: Record<string, string | PropSpec>
  resolver?: Resolver
  allowHttp?: boolean
}

export interface PrepareOptions {
  user?: unknown
  surfaceId?: string
  declared?: string[]
  locale?: string
  nextId?: IdFactory
}

export interface ChartOptions {
  locale?: string
  width?: number
  height?: number
  dark?: boolean
}

export interface PatchParts {
  upsert?: Component[]
  remove?: string[]
  root?: string
}

export declare class PolyUIError extends Error {
  constructor(message: string, details?: Json)
}

export declare class ValidationError extends PolyUIError {
  readonly code: 'POLYUI_VALIDATION'
  problems: Problem[]
  operation: Operation
}

export declare class CatalogError extends PolyUIError {
  readonly code: 'POLYUI_CATALOG'
  path: string
}

export declare class ChartError extends PolyUIError {
  readonly code: 'POLYUI_CHART'
}

export declare class ProtocolError extends PolyUIError {
  readonly code: 'POLYUI_PROTOCOL'
}

export declare class RefRefusedError extends PolyUIError {
  readonly code: 'POLYUI_REF_REFUSED'
  constructor(message?: string, details?: Json)
}

export declare class Catalogs {
  private constructor()
  static builtin(): Catalogs
  static empty(): Catalogs
  withCatalog(catalog: Json | string): Catalogs
  restrict(declared: string[]): Catalogs
  negotiate(declared: string[]): string[]
  get tags(): string[]
  typeNames(): string[]
  has(typeName: string): boolean
}

export declare class Surface {
  private constructor()
  static fromJSON(value: SurfaceJSON | Json): Surface
  toJSON(): SurfaceJSON
  get id(): string | null
  get root(): string
  get components(): Component[]
  get data(): Json
  get(id: string): Component | null
  apply(update: Update): Surface
  equals(other: Surface): boolean
}

export declare class Update {
  private constructor()
  toJSON(): Json
  get surface(): string
  get upsert(): Component[]
  get remove(): string[]
  get root(): string | null
  get data(): Json[]
}

export declare class Prepared {
  private constructor()
  get surfaceId(): string
  get model(): Surface
  get client(): Surface
  get messages(): Json[]
  get summary(): string
  get actions(): SealedAction[]
  get seq(): number
  get touched(): string[]
}

export declare class Validator {
  constructor(catalogs: Catalogs, options?: ValidatorOptions)
  get catalogs(): Catalogs
  get refSources(): string[]
  showUi(payload: Payload): Surface
  updateUi(payload: Payload, current: Surface): Update
  closeUi(payload: Payload): string
  resolveRefs(surface: Surface, user?: unknown): Surface
  sealActions(surface: Surface, nextId?: IdFactory): { surface: Surface; actions: SealedAction[] }
  compileCharts(surface: Surface, options?: Omit<ChartOptions, 'dark'>): Surface
  downgrade(surface: Surface, declared: string[], locale?: string): Surface
  summary(surface: Surface, surfaceId?: string, locale?: string): string
  fallback(surface: Surface, locale?: string): string
  prepareShow(payload: Payload, options?: PrepareOptions): Prepared
  prepareUpdate(
    payload: Payload,
    model: Surface,
    client: Surface,
    seq: number,
    options?: Omit<PrepareOptions, 'surfaceId'>,
  ): Prepared
  toolDefinitions(declared?: string[], strict?: boolean): Tool[]
}

export declare class Client {
  constructor(hello: Json | string, catalogs?: Catalogs, allowHttp?: boolean)
  get locale(): string
  get session(): string | null
  get surfaces(): string[]
  data(surface: string): unknown | null
  fallback(surface: string): string | null
  receive(message: Json | string): Json
  setValue(surface: string, path: string, value: unknown): Json
  fire(surface: string, component: string, prop: string, detail?: Json): Json | null
  confirm(surface: string, actionId: string, nonce: string): Json | null
  say(surface: string): Array<{ id: string; text: string }>
  matchVoice(surface: string, text: string): Json | null
}

export declare function toolDefinitions(
  catalogs: Catalogs,
  actions?: Record<string, ActionSpec>,
  strict?: boolean,
): Tool[]
export declare function openMessage(
  surfaceId: string,
  surface: Surface,
  seq?: number,
  fallback?: string,
): Json
export declare function patchMessage(surfaceId: string, seq: number, parts?: PatchParts): Json
export declare function closeMessage(surfaceId: string, seq: number): Json
export declare function closedSummary(surfaceId: string): string
export declare function compileChart(props: Json, options?: Omit<ChartOptions, 'dark'>): Json
export declare function chartSvg(props: Json, options?: ChartOptions): string
export declare function describeChart(props: Json, locale?: string): string
