import { createRenderer, loadCore, type Json, type Renderer, type WasmCore } from "../../js/polyui-web/src/index";
import { ReplaySocket } from "./replay";

declare const REPLAY: boolean;

type JsonObject = { readonly [key: string]: Json };
type Source = "typed" | "voice";

interface RecognitionAlternative {
  readonly transcript: string;
}

interface RecognitionResult {
  readonly isFinal: boolean;
  readonly length: number;
  readonly [index: number]: RecognitionAlternative | undefined;
}

interface RecognitionEvent extends Event {
  readonly resultIndex: number;
  readonly results: { readonly length: number; readonly [index: number]: RecognitionResult | undefined };
}

interface RecognitionErrorEvent extends Event {
  readonly error: string;
}

interface Recognition extends EventTarget {
  lang: string;
  interimResults: boolean;
  continuous: boolean;
  start(): void;
  stop(): void;
  abort(): void;
}

type RecognitionConstructor = new () => Recognition;

function required<T extends Element>(selector: string): T {
  const found = document.querySelector<T>(selector);
  if (!found) throw new Error(`the chat page is missing ${selector}`);
  return found;
}

const thread = required<HTMLElement>("#thread");
const welcome = required<HTMLElement>("#welcome");
const status = required<HTMLParagraphElement>("#status");
const composer = required<HTMLFormElement>("#composer");
const prompt = required<HTMLTextAreaElement>("#prompt");
const sendButton = required<HTMLButtonElement>("#send");
const micButton = required<HTMLButtonElement>("#mic");
const speakerButton = required<HTMLButtonElement>("#speaker");
const hint = required<HTMLParagraphElement>("#hint");

const locale = navigator.language || "en";
const wasmUrl = new URL("./polyui_core_bg.wasm", import.meta.url);
const endpoint = new URL("/ws", location.href);
endpoint.protocol = location.protocol === "https:" ? "wss:" : "ws:";

const loopbackHosts = ["localhost", "127.0.0.1", "[::1]"];

interface Setup {
  readonly embedOrigin: string | null;
}

let setup: Setup | null = null;

async function readSetup(): Promise<Setup> {
  if (REPLAY) return { embedOrigin: null };
  try {
    const response = await fetch(new URL("/config.json", location.href));
    if (!response.ok) return { embedOrigin: null };
    const config = parse(await response.text());
    const origin = config ? config["embed_origin"] : null;
    return { embedOrigin: typeof origin === "string" && origin !== "" ? origin : null };
  } catch {
    return { embedOrigin: null };
  }
}

function sandboxOverHttp(origin: string | null): boolean {
  if (origin === null) return false;
  try {
    const url = new URL(origin);
    return url.protocol === "http:" && loopbackHosts.includes(url.hostname);
  } catch {
    return false;
  }
}

function helloFor(current: Setup, resume: Resume | null) {
  return {
    v: 1,
    t: "hello",
    client: { name: "polyui-chat", version: "0.1.0" },
    catalogs: current.embedOrigin === null ? ["core@1", "chart@1"] : ["core@1", "chart@1", "embed@1"],
    modalities: ["screen", "voice"],
    features: ["sync", "stream", "resume"],
    locale,
    ...(resume === null ? {} : { resume }),
  };
}

type Resume = { readonly session: string; readonly token: string };

interface Line extends EventTarget {
  readonly readyState: number;
  send(data: string): void;
}

let socket: Line | null = null;
let core: WasmCore | null = null;
let renderer: Renderer | null = null;
let live = false;
let retries = 0;
let resumeWith: Resume | null = null;
let turn: HTMLElement | null = null;
let answer: HTMLParagraphElement | null = null;
let turnSurfaces: string[] = [];
let latestSurface: string | null = null;
let readAloud = false;
let listening: Recognition | null = null;
const placed = new Map<string, HTMLElement>();

function parse(text: string): JsonObject | null {
  try {
    const value = JSON.parse(text) as Json;
    return typeof value === "object" && value !== null && !Array.isArray(value) ? value : null;
  } catch {
    return null;
  }
}

function field(message: JsonObject, key: string): string {
  const value = message[key];
  return typeof value === "string" ? value : "";
}

function setStatus(state: "connecting" | "live" | "offline", label: string): void {
  status.dataset["state"] = state;
  status.textContent = label;
}

function setHint(text: string): void {
  hint.textContent = text;
}

function refreshSend(): void {
  sendButton.disabled = !live || prompt.value.trim() === "";
}

function nearBottom(): boolean {
  return thread.scrollHeight - thread.scrollTop - thread.clientHeight < 120;
}

function follow(stick: boolean): void {
  if (stick) requestAnimationFrame(() => thread.scrollTo({ top: thread.scrollHeight }));
}

function append(node: HTMLElement): void {
  const stick = nearBottom();
  welcome.hidden = true;
  thread.append(node);
  follow(stick);
}

function agentTurn(): HTMLElement {
  if (!turn) {
    turn = document.createElement("div");
    turn.className = "turn turn-agent";
    append(turn);
  }
  return turn;
}

function streamingAnswer(): HTMLParagraphElement {
  if (!answer) {
    answer = document.createElement("p");
    answer.className = "answer";
    answer.dataset["streaming"] = "";
    agentTurn().append(answer);
  }
  return answer;
}

function settleAnswer(): void {
  if (answer) {
    delete answer.dataset["streaming"];
    if (answer.textContent === "") answer.remove();
  }
  answer = null;
}

function note(text: string, tone: "info" | "error" = "info"): void {
  settleAnswer();
  const line = document.createElement("p");
  line.className = "note";
  line.dataset["tone"] = tone;
  line.textContent = text;
  agentTurn().append(line);
}

function userBubble(text: string, source: Source): void {
  const row = document.createElement("div");
  row.className = "turn turn-user";
  const bubble = document.createElement("p");
  bubble.className = "bubble";
  bubble.dataset["source"] = source;
  bubble.textContent = text;
  row.append(bubble);
  append(row);
}

function finishTurn(): void {
  settleAnswer();
  const spoken = turn ? [...turn.querySelectorAll(".answer")].map((node) => node.textContent ?? "").join(" ") : "";
  const surfaces = turnSurfaces;
  turn = null;
  turnSurfaces = [];
  if (readAloud) speak(spoken, surfaces);
}

function placeSurface(id: string): void {
  if (!renderer || placed.has(id)) return;
  settleAnswer();
  const holder = document.createElement("div");
  holder.className = "surface";
  holder.append(renderer.surface(id));
  placed.set(id, holder);
  const stick = nearBottom();
  agentTurn().append(holder);
  follow(stick);
  turnSurfaces.push(id);
}

function onServerMessage(raw: string): void {
  const message = parse(raw);
  if (!message || !renderer) return;
  const type = field(message, "t");
  switch (type) {
    case "welcome": {
      const token = field(message, "resume_token");
      if (message["resumed"] === false) {
        resumeWith = null;
        startOver();
        return;
      }
      resumeWith = token === "" ? null : { session: field(message, "session"), token };
      live = true;
      retries = 0;
      setStatus("live", REPLAY ? "Replay" : "Connected");
      if (REPLAY) setHint("This page replays one canned conversation in your browser with the real wasm core. No model is called.");
      renderer.receive(raw);
      refreshSend();
      return;
    }
    case "agent.delta": {
      const piece = field(message, "text");
      const stick = nearBottom();
      streamingAnswer().append(piece);
      follow(stick);
      return;
    }
    case "agent.done":
      finishTurn();
      return;
    case "agent.error":
      note(field(message, "message") || "The assistant could not answer. Try again.", "error");
      finishTurn();
      return;
  }
  if (!type.startsWith("ui.")) return;
  const surface = field(message, "surface");
  if ((type === "ui.open" || type === "ui.snapshot") && surface !== "") {
    placeSurface(surface);
    latestSurface = surface;
  }
  const stick = nearBottom();
  renderer.receive(raw);
  follow(stick);
  if (type === "ui.close" && latestSurface === surface) latestSurface = null;
}

function markStale(): void {
  for (const holder of placed.values()) holder.dataset["stale"] = "";
  placed.clear();
  latestSurface = null;
}

function scheduleReconnect(): void {
  const delay = Math.min(15000, 1000 * 2 ** retries);
  retries += 1;
  setStatus("offline", `Offline, retrying in ${Math.round(delay / 1000)} s`);
  setTimeout(() => void connect(), delay);
}

function openLine(current: Setup, resume: Resume | null): Line {
  const ws: Line = REPLAY ? new ReplaySocket() : new WebSocket(endpoint);
  socket = ws;
  ws.addEventListener("open", () => ws.send(JSON.stringify(helloFor(current, resume))));
  ws.addEventListener("message", (event) => {
    if (socket === ws && event instanceof MessageEvent && typeof event.data === "string") onServerMessage(event.data);
  });
  ws.addEventListener("close", () => {
    if (socket !== ws) return;
    live = false;
    refreshSend();
    settleAnswer();
    turn = null;
    if (resumeWith !== null && core !== null) {
      scheduleReconnect();
      return;
    }
    teardown();
    scheduleReconnect();
  });
  return ws;
}

function teardown(): void {
  markStale();
  core?.free();
  core = null;
  renderer = null;
}

function startOver(): void {
  const old = socket;
  socket = null;
  if (old instanceof WebSocket) old.close();
  teardown();
  void connect();
}

async function connect(): Promise<void> {
  setStatus("connecting", "Connecting");
  setup ??= await readSetup();
  const current = setup;
  if (resumeWith !== null && core !== null && renderer !== null) {
    openLine(current, resumeWith);
    return;
  }
  const hello = helloFor(current, null);
  let next: WasmCore;
  try {
    next = await loadCore(hello, wasmUrl);
  } catch (error) {
    setStatus("offline", "Could not load the PolyUI core");
    setHint(error instanceof Error ? error.message : String(error));
    return;
  }
  core = next;
  renderer = createRenderer({
    core: next,
    send: (message) => {
      if (socket !== null && socket.readyState === WebSocket.OPEN) socket.send(message);
    },
    ...(current.embedOrigin === null ? {} : { embedOrigin: current.embedOrigin }),
    allowHttp: sandboxOverHttp(current.embedOrigin),
  });
  openLine(current, null);
}

function sendSay(text: string, source: Source): void {
  const words = text.trim();
  if (!live || !socket || words === "") return;
  userBubble(words, source);
  socket.send(JSON.stringify({ v: 1, t: "user.say", text: words }));
  turn = null;
  turnSurfaces = [];
  streamingAnswer();
}

function speechSupported(): boolean {
  return "speechSynthesis" in window && typeof SpeechSynthesisUtterance === "function";
}

function stopSpeaking(): void {
  if (!speechSupported()) return;
  speechSynthesis.cancel();
  if (renderer) for (const id of placed.keys()) renderer.highlight(id, null);
}

function speak(text: string, surfaces: readonly string[]): void {
  if (!speechSupported() || !renderer) return;
  const view = renderer;
  stopSpeaking();
  const queue: { text: string; surface: string | null; id: string | null }[] = [];
  if (text.trim() !== "") queue.push({ text, surface: null, id: null });
  for (const surface of surfaces) {
    if (!placed.has(surface)) continue;
    for (const utterance of view.say(surface)) queue.push({ text: utterance.text, surface, id: utterance.id });
  }
  queue.forEach((item, index) => {
    const utterance = new SpeechSynthesisUtterance(item.text);
    utterance.lang = locale;
    utterance.addEventListener("start", () => {
      if (item.surface !== null) view.highlight(item.surface, item.id);
    });
    const clear = (): void => {
      const following = queue[index + 1];
      if (item.surface !== null && following?.surface !== item.surface) view.highlight(item.surface, null);
    };
    utterance.addEventListener("end", clear);
    utterance.addEventListener("error", clear);
    speechSynthesis.speak(utterance);
  });
}

function recognitionConstructor(): RecognitionConstructor | null {
  const candidate: unknown = Reflect.get(window, "SpeechRecognition") ?? Reflect.get(window, "webkitSpeechRecognition");
  return typeof candidate === "function" ? (candidate as RecognitionConstructor) : null;
}

function handleSpoken(text: string): void {
  const words = text.trim();
  if (words === "") return;
  const surface = latestSurface;
  const match = surface !== null && renderer ? renderer.matchVoice(surface, words) : null;
  if (match) {
    userBubble(words, "voice");
    setHint("Answered on the card above.");
    return;
  }
  setHint("");
  sendSay(words, "voice");
}

function listenErrorText(code: string): string {
  if (code === "not-allowed" || code === "service-not-allowed") return "Microphone access is blocked. Allow it in the browser to talk.";
  if (code === "no-speech") return "Nothing was heard. Tap the microphone and try again.";
  if (code === "audio-capture") return "No microphone was found.";
  if (code === "network") return "Speech recognition needs a network connection.";
  return "Voice input stopped. Try again.";
}

function setUpMicrophone(): void {
  const Recognizer = recognitionConstructor();
  if (!Recognizer) {
    micButton.disabled = true;
    micButton.title = "Voice input is not available in this browser";
    return;
  }
  micButton.addEventListener("click", () => {
    if (listening) {
      listening.stop();
      return;
    }
    stopSpeaking();
    const recognition = new Recognizer();
    recognition.lang = locale;
    recognition.interimResults = true;
    recognition.continuous = false;
    let heard = "";
    recognition.addEventListener("result", (event) => {
      const result = event as RecognitionEvent;
      let interim = "";
      for (let index = result.resultIndex; index < result.results.length; index += 1) {
        const entry = result.results[index];
        const transcript = entry?.[0]?.transcript ?? "";
        if (entry?.isFinal) heard += transcript;
        else interim += transcript;
      }
      setHint(interim === "" ? heard : interim);
    });
    recognition.addEventListener("error", (event) => {
      heard = "";
      setHint(listenErrorText((event as RecognitionErrorEvent).error));
    });
    recognition.addEventListener("end", () => {
      listening = null;
      micButton.setAttribute("aria-pressed", "false");
      micButton.setAttribute("aria-label", "Speak");
      if (heard !== "") handleSpoken(heard);
    });
    listening = recognition;
    micButton.setAttribute("aria-pressed", "true");
    micButton.setAttribute("aria-label", "Stop listening");
    setHint("Listening");
    recognition.start();
  });
}

function setUpSpeaker(): void {
  if (!speechSupported()) {
    speakerButton.disabled = true;
    speakerButton.title = "Reading aloud is not available in this browser";
    return;
  }
  speakerButton.addEventListener("click", () => {
    readAloud = !readAloud;
    speakerButton.setAttribute("aria-pressed", String(readAloud));
    speakerButton.setAttribute("aria-label", readAloud ? "Stop reading answers aloud" : "Read answers aloud");
    if (!readAloud) stopSpeaking();
    else if (latestSurface !== null) speak("", [latestSurface]);
  });
}

composer.addEventListener("submit", (event) => {
  event.preventDefault();
  sendSay(prompt.value, "typed");
  prompt.value = "";
  refreshSend();
});

prompt.addEventListener("input", refreshSend);
prompt.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
    event.preventDefault();
    composer.requestSubmit();
  }
});

for (const button of document.querySelectorAll<HTMLButtonElement>(".suggestion")) {
  button.addEventListener("click", () => sendSay(button.textContent ?? "", "typed"));
}

setUpMicrophone();
setUpSpeaker();
refreshSend();
void connect();
