import { invoke } from "@tauri-apps/api/core";

// ---------------------------------------------------------------------------
// Modelo de dados (espelha os tipos do backend Rust)
// ---------------------------------------------------------------------------

type Binding =
  | { type: "none" }
  | { type: "keyboard"; accords: { modifiers: string[]; code: string | null }[] }
  | { type: "media"; code: string }
  | { type: "mouse"; buttons: string[]; modifier?: string | null };

const NONE: Binding = { type: "none" };

interface State {
  layer: number;
  buttons: Binding[];
  knob: { ccw: Binding; press: Binding; cw: Binding };
  /** "reversed" = uso girado: knob à esquerda, teclas 3·2·1, giro invertido. */
  orientation: "normal" | "reversed";
}

const state: State = {
  layer: 0,
  buttons: [NONE, NONE, NONE],
  knob: { ccw: NONE, press: NONE, cw: NONE },
  orientation: "reversed",
};

let catalog: {
  keys: string[];
  exclusive: string[];
  modifiers: string[];
  media: string[];
  mouse_buttons: string[];
} = { keys: [], exclusive: [], modifiers: [], media: [], mouse_buttons: [] };

let selected: string | null = null; // ex: "button:0" | "knob:ccw"

// ---------------------------------------------------------------------------
// Helpers de acesso ao binding selecionado
// ---------------------------------------------------------------------------

function getBinding(target: string): Binding {
  const [kind, key] = target.split(":");
  if (kind === "button") return state.buttons[Number(key)];
  return state.knob[key as "ccw" | "press" | "cw"];
}

function setBinding(target: string, b: Binding) {
  const [kind, key] = target.split(":");
  if (kind === "button") state.buttons[Number(key)] = b;
  else state.knob[key as "ccw" | "press" | "cw"] = b;
}

function labelFor(b: Binding): string {
  switch (b.type) {
    case "none":
      return "—";
    case "keyboard": {
      const a = b.accords[0];
      if (!a) return "—";
      const parts = [...a.modifiers.map(cap), a.code ?? ""].filter(Boolean);
      return parts.join("+") || "—";
    }
    case "media":
      return "♪ " + b.code;
    case "mouse":
      return "🖱 " + b.buttons.join("+");
  }
}

const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

/** Reconstrói o desenho do teclado de acordo com a orientação. */
function renderDevice() {
  const row = document.querySelector("#pad-row")!;
  row.innerHTML = "";
  const reversed = state.orientation === "reversed";

  // --- grupo do knob ---
  const knob = document.createElement("div");
  knob.className = "knob-wrap";
  // No modo invertido o giro físico é espelhado: o controle da esquerda (⟲)
  // dispara a ação CW do firmware, e o da direita (⟳) dispara a CCW.
  const leftAct = makeEl("knob-act", reversed ? "knob:cw" : "knob:ccw", "⟲", "Girar p/ esquerda");
  const press = makeEl("knob", "knob:press", "knob", "Apertar o knob");
  const rightAct = makeEl("knob-act", reversed ? "knob:ccw" : "knob:cw", "⟳", "Girar p/ direita");
  knob.append(leftAct, press, rightAct);

  // --- teclas ---
  const order = reversed ? ["button:2", "button:1", "button:0"] : ["button:0", "button:1", "button:2"];
  const keys = order.map((t) =>
    makeEl("key", t, String(Number(t.split(":")[1]) + 1), "")
  );

  if (reversed) row.append(knob, ...keys);
  else row.append(...keys, knob);
}

/** Cria um elemento clicável do teclado já com label/estado aplicados. */
function makeEl(cls: string, target: string, base: string, title: string): HTMLElement {
  const el = document.createElement("button");
  el.type = "button";
  el.className = cls;
  el.dataset.target = target;
  if (title) el.title = title;

  const b = getBinding(target);
  const span = document.createElement("span");
  // knob-act mantém o ícone fixo; key/knob mostram o binding
  span.textContent = cls === "knob-act" ? base : b.type === "none" ? base : labelFor(b);
  el.append(span);

  el.classList.toggle("mapped", b.type !== "none");
  el.classList.toggle("selected", target === selected);

  el.addEventListener("click", () => {
    selected = target;
    renderDevice();
    renderEditor();
  });
  return el;
}

function renderEditor() {
  const body = document.querySelector("#editor-body")!;
  const hint = document.querySelector("#editor-hint")!;
  const title = document.querySelector("#editor-title")!;

  if (!selected) {
    body.classList.add("hidden");
    hint.classList.remove("hidden");
    title.textContent = "Selecione uma tecla";
    return;
  }
  body.classList.remove("hidden");
  hint.classList.add("hidden");
  title.textContent = prettyTarget(selected);

  const b = getBinding(selected);
  const typeSel = document.querySelector<HTMLSelectElement>("#binding-type")!;
  typeSel.value = b.type;

  toggle("#grp-keyboard", b.type === "keyboard");
  toggle("#grp-media", b.type === "media");
  toggle("#grp-mouse", b.type === "mouse");

  // modificadores
  const mods = document.querySelector("#mods")!;
  const active = b.type === "keyboard" ? b.accords[0]?.modifiers ?? [] : [];
  mods.querySelectorAll<HTMLButtonElement>("button").forEach((btn) => {
    btn.classList.toggle("on", active.includes(btn.dataset.mod!));
  });

  // selects
  if (b.type === "keyboard")
    (document.querySelector("#key-input") as HTMLInputElement).value = b.accords[0]?.code ?? "";
  if (b.type === "media")
    (document.querySelector("#media-select") as HTMLSelectElement).value = b.code;
  if (b.type === "mouse")
    (document.querySelector("#mouse-select") as HTMLSelectElement).value = b.buttons[0] ?? "left";
}

function prettyTarget(t: string): string {
  const [kind, key] = t.split(":");
  if (kind === "button") return `Botão ${Number(key) + 1}`;
  if (key === "press") return "Knob (apertar)";
  // No modo invertido, o CW do firmware corresponde ao giro físico p/ esquerda.
  const reversed = state.orientation === "reversed";
  const isLeft = reversed ? key === "cw" : key === "ccw";
  return isLeft ? "Knob ← (girar p/ esquerda)" : "Knob → (girar p/ direita)";
}

const toggle = (sel: string, on: boolean) =>
  document.querySelector(sel)!.classList.toggle("hidden", !on);

// ---------------------------------------------------------------------------
// Atualização do binding a partir da UI
// ---------------------------------------------------------------------------

function updateFromEditor() {
  if (!selected) return;
  const type = (document.querySelector("#binding-type") as HTMLSelectElement).value;
  let b: Binding;

  if (type === "keyboard") {
    const modBtns = [...document.querySelectorAll<HTMLButtonElement>("#mods button.on")];
    const modifiers = modBtns.map((x) => x.dataset.mod!);
    const code = (document.querySelector("#key-input") as HTMLInputElement).value.trim() || null;
    b = { type: "keyboard", accords: [{ modifiers, code }] };
  } else if (type === "media") {
    const code = (document.querySelector("#media-select") as HTMLSelectElement).value;
    b = { type: "media", code };
  } else if (type === "mouse") {
    const btn = (document.querySelector("#mouse-select") as HTMLSelectElement).value;
    b = { type: "mouse", buttons: [btn], modifier: null };
  } else {
    b = NONE;
  }

  setBinding(selected, b);
  renderDevice();
  renderEditor();
}

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

function buildConfig() {
  return {
    config: {
      layer: state.layer,
      buttons: state.buttons,
      knob: state.knob,
    },
  };
}

function log(msg: string, isError = false) {
  const el = document.querySelector("#log")!;
  el.classList.remove("hidden");
  el.textContent = msg;
  el.classList.toggle("error", isError);
}

async function detect() {
  const status = document.querySelector("#status")!;
  const text = document.querySelector("#status-text")!;
  try {
    const info = await invoke<{
      found: boolean;
      product: string | null;
      interface: number;
    }>("detect_keyboard");
    status.className = "status " + (info.found ? "status--ok" : "status--off");
    text.textContent = info.found
      ? `${info.product ?? "Mini Keyboard"} conectado`
      : "teclado não encontrado";
  } catch (e) {
    status.className = "status status--off";
    text.textContent = "erro: " + e;
  }
}

async function preview() {
  try {
    const lines = await invoke<string[]>("preview_config", buildConfig());
    log(lines.length ? lines.join("\n") : "(nada configurado)");
  } catch (e) {
    log(String(e), true);
  }
}

async function upload() {
  try {
    const n = await invoke<number>("upload_config", buildConfig());
    log(`✅ Gravado! ${n} mensagens enviadas ao teclado.`);
  } catch (e) {
    log("❌ " + e, true);
  }
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

function fillSelect(sel: string, items: string[], withEmpty = false) {
  const el = document.querySelector<HTMLSelectElement>(sel)!;
  el.innerHTML = "";
  if (withEmpty) el.append(new Option("(nenhuma)", ""));
  for (const it of items) el.append(new Option(it, it));
}

function fillDatalist(sel: string, items: string[]) {
  const el = document.querySelector<HTMLDataListElement>(sel)!;
  el.innerHTML = "";
  for (const it of items) el.append(new Option(it));
}

/// Traduz um KeyboardEvent (tecla física) para o nome usado no catálogo.
function jsKeyToName(e: KeyboardEvent): string | null {
  const c = e.code;
  let m: RegExpMatchArray | null;
  if ((m = c.match(/^Key([A-Z])$/))) return m[1].toLowerCase();
  if ((m = c.match(/^Digit(\d)$/))) return m[1];
  if ((m = c.match(/^F(\d{1,2})$/))) return "f" + m[1];
  if ((m = c.match(/^Numpad(\d)$/))) return "numpad" + m[1];
  const map: Record<string, string> = {
    Enter: "enter", Escape: "escape", Backspace: "backspace", Tab: "tab", Space: "space",
    Minus: "minus", Equal: "equal", BracketLeft: "leftbracket", BracketRight: "rightbracket",
    Backslash: "backslash", Semicolon: "semicolon", Quote: "quote", Backquote: "grave",
    Comma: "comma", Period: "dot", Slash: "slash", CapsLock: "capslock",
    ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right",
    Home: "home", End: "end", PageUp: "pageup", PageDown: "pagedown",
    Insert: "insert", Delete: "delete", PrintScreen: "printscreen", ScrollLock: "scrolllock",
    Pause: "pause", NumLock: "numlock", ContextMenu: "application",
    NumpadDivide: "numpadslash", NumpadMultiply: "numpadasterisk", NumpadSubtract: "numpadminus",
    NumpadAdd: "numpadplus", NumpadEnter: "numpadenter", NumpadDecimal: "numpaddot",
  };
  if (map[c]) return map[c];
  // teclas que são só modificadores: ignora (esperamos a tecla "de verdade")
  return null;
}

let capturing = false;
function setCaptureUI(on: boolean) {
  capturing = on;
  const btn = document.querySelector("#capture-btn")!;
  btn.textContent = on ? "pressione uma tecla…" : "🎯 Capturar tecla";
  btn.classList.toggle("recording", on);
}

window.addEventListener("DOMContentLoaded", async () => {
  catalog = await invoke("key_catalog");

  fillDatalist("#key-list", catalog.keys);
  fillSelect("#media-select", catalog.media);

  // chips das teclas exclusivas (F13–F24)
  const exq = document.querySelector("#exclusive-chips")!;
  for (const k of catalog.exclusive) {
    const chip = document.createElement("button");
    chip.type = "button";
    chip.textContent = k.toUpperCase();
    chip.addEventListener("click", () => {
      (document.querySelector("#key-input") as HTMLInputElement).value = k;
      updateFromEditor();
    });
    exq.append(chip);
  }

  // captura por tecla física
  document.querySelector("#capture-btn")!.addEventListener("click", () => setCaptureUI(!capturing));
  window.addEventListener("keydown", (e) => {
    if (!capturing) return;
    const name = jsKeyToName(e);
    if (!name) return; // ignora modificadores puros até vir uma tecla real
    e.preventDefault();
    const mods = [
      [e.ctrlKey, "ctrl"], [e.shiftKey, "shift"], [e.altKey, "alt"], [e.metaKey, "win"],
    ].filter(([on]) => on).map(([, n]) => n as string);
    document.querySelectorAll<HTMLButtonElement>("#mods button").forEach((b) =>
      b.classList.toggle("on", mods.includes(b.dataset.mod!))
    );
    (document.querySelector("#key-input") as HTMLInputElement).value = name;
    setCaptureUI(false);
    updateFromEditor();
  });

  // botões de modificador
  const mods = document.querySelector("#mods")!;
  for (const m of catalog.modifiers) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.dataset.mod = m;
    btn.textContent = cap(m);
    btn.addEventListener("click", () => {
      btn.classList.toggle("on");
      updateFromEditor();
    });
    mods.append(btn);
  }

  // alternar orientação (normal / invertido)
  const orient = document.querySelector<HTMLInputElement>("#orient-toggle")!;
  orient.checked = state.orientation === "reversed";
  orient.addEventListener("change", () => {
    state.orientation = orient.checked ? "reversed" : "normal";
    renderDevice();
    renderEditor();
  });

  // editor
  document.querySelector("#binding-type")!.addEventListener("change", updateFromEditor);
  document.querySelector("#key-input")!.addEventListener("input", updateFromEditor);
  document.querySelector("#media-select")!.addEventListener("change", updateFromEditor);
  document.querySelector("#mouse-select")!.addEventListener("change", updateFromEditor);

  // ações
  document.querySelector("#preview")!.addEventListener("click", preview);
  document.querySelector("#upload")!.addEventListener("click", upload);
  document.querySelector("#refresh")!.addEventListener("click", detect);

  renderDevice();
  renderEditor();
  detect();
});
