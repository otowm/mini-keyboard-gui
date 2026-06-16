//! Protocolo e transporte HID para o macro pad CH57x (VID 1189 / PID 8890).
//!
//! O formato dos pacotes foi portado da engenharia reversa do projeto
//! kriomant/ch57x-keyboard-tool (modelo k8890). Cada mensagem tem 64 bytes e
//! começa com `0x03`. O transporte difere por plataforma (ver `upload`):
//!   - Windows: hidapi, escreve os 64 bytes direto (o `0x03` é o report id;
//!     NÃO prefixar 0x00 — viraria 65 e o WriteFile recusa).
//!   - Linux: libusb (rusb). O usbhid não vincula a interface de config do
//!     CH57x (só tem endpoint OUT), então não há hidraw; mandamos os 64 bytes
//!     crus por interrupt OUT no endpoint da interface de config.
//!
//! Layout das mensagens:
//!   - início de bind:  03 fe (layer+1) 01 01 00 00 00 00
//!   - tecla (teclado): 03 <key_id> ((layer+1)<<4 | 1) <len> <i> <mods> <code> 00 00
//!   - tecla (mídia):   03 <key_id> ((layer+1)<<4 | 2) <low> <high> 00 00 00 00
//!   - tecla (mouse):   03 <key_id> ((layer+1)<<4 | 3) <buttons> <dx> <dy> <wheel> <mod> 00
//!   - fim de bind:     03 aa aa 00 00 00 00 00 00

use serde::{Deserialize, Serialize};

pub const VID: u16 = 0x1189;
pub const PID: u16 = 0x8890;

const MSG_LEN: usize = 64;
/// Knob actions começam após os botões; o firmware reserva 12 ids para botões.
const KNOB_BASE: u8 = 12;

// ---------------------------------------------------------------------------
// Tipos recebidos do frontend
// ---------------------------------------------------------------------------

/// Uma combinação de tecla: modificadores + (opcionalmente) uma tecla.
#[derive(Debug, Clone, Deserialize)]
pub struct Accord {
    #[serde(default)]
    pub modifiers: Vec<String>,
    /// Nome da tecla (ex: "a", "f5", "enter") ou None para accord só de modificador.
    #[serde(default)]
    pub code: Option<String>,
}

/// O que uma tecla/ação do knob faz.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Binding {
    /// Sem mapeamento — a tecla é ignorada no upload.
    None,
    /// Sequência de teclas (macro de até 5 toques).
    Keyboard { accords: Vec<Accord> },
    /// Tecla de mídia (volume, play, etc).
    Media { code: String },
    /// Clique de mouse.
    Mouse {
        #[serde(default)]
        buttons: Vec<String>,
        #[serde(default)]
        modifier: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct KnobConfig {
    pub ccw: Binding,
    pub press: Binding,
    pub cw: Binding,
}

/// Configuração completa de uma camada enviada pelo frontend.
#[derive(Debug, Clone, Deserialize)]
pub struct KeyConfig {
    #[serde(default)]
    pub layer: u8,
    /// Bindings dos botões físicos, na ordem (índice 0 = primeiro botão).
    pub buttons: Vec<Binding>,
    /// Knob opcional (este modelo tem 0 ou 1 knob).
    #[serde(default)]
    pub knob: Option<KnobConfig>,
}

// ---------------------------------------------------------------------------
// Identificação da tecla no protocolo
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum KnobAction {
    Ccw = 0,
    Press = 1,
    Cw = 2,
}

#[derive(Debug, Clone, Copy)]
enum Key {
    Button(u8),
    Knob(u8, KnobAction),
}

impl Key {
    fn id(self) -> Result<u8, String> {
        match self {
            Key::Button(n) if n >= KNOB_BASE => Err("índice de botão inválido".into()),
            Key::Button(n) => Ok(n + 1),
            Key::Knob(n, _) if n >= 3 => Err("índice de knob inválido".into()),
            Key::Knob(n, a) => Ok(KNOB_BASE + 1 + 3 * n + (a as u8)),
        }
    }
}

// ---------------------------------------------------------------------------
// Construção das mensagens
// ---------------------------------------------------------------------------

fn push_msg(out: &mut Vec<u8>, msg: &[u8]) {
    let mut buf = [0u8; MSG_LEN];
    buf[..msg.len()].copy_from_slice(msg);
    out.extend_from_slice(&buf);
}

fn modifiers_byte(names: &[String]) -> Result<u8, String> {
    let mut bits = 0u8;
    for name in names {
        let bit = match name.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => 0,
            "shift" => 1,
            "alt" | "opt" => 2,
            "win" | "cmd" | "super" | "meta" => 3,
            "rctrl" => 4,
            "rshift" => 5,
            "ralt" | "ropt" => 6,
            "rwin" | "rcmd" => 7,
            other => return Err(format!("modificador desconhecido: {other}")),
        };
        bits |= 1 << bit;
    }
    Ok(bits)
}

/// Constrói as mensagens de 64 bytes para um único bind (start + corpo + finish).
fn build_binding(layer: u8, key: Key, binding: &Binding, out: &mut Vec<u8>) -> Result<(), String> {
    if matches!(binding, Binding::None) {
        return Ok(());
    }
    if layer > 15 {
        return Err("camada inválida (0-15)".into());
    }
    let key_id = key.id()?;
    let lay = layer + 1;

    // Início do bind.
    push_msg(out, &[0x03, 0xfe, lay, 0x01, 0x01, 0, 0, 0, 0]);

    match binding {
        Binding::None => unreachable!(),
        Binding::Keyboard { accords } => {
            if accords.is_empty() {
                return Err("macro de teclado vazia".into());
            }
            if accords.len() > 5 {
                return Err("macro longa demais (máx 5 toques)".into());
            }
            let len = accords.len() as u8;
            // O firmware espera um toque vazio antes dos reais.
            let mut items: Vec<(u8, u8)> = vec![(0, 0)];
            for a in accords {
                let m = modifiers_byte(&a.modifiers)?;
                let c = match &a.code {
                    Some(name) => key_code(name)?,
                    None => 0,
                };
                items.push((m, c));
            }
            for (i, (m, c)) in items.into_iter().enumerate() {
                push_msg(
                    out,
                    &[0x03, key_id, (lay << 4) | 0x01, len, i as u8, m, c, 0, 0],
                );
            }
        }
        Binding::Media { code } => {
            let value = media_code(code)?;
            let [low, high] = value.to_le_bytes();
            push_msg(out, &[0x03, key_id, (lay << 4) | 0x02, low, high, 0, 0, 0, 0]);
        }
        Binding::Mouse { buttons, modifier } => {
            let mut btn = 0u8;
            for b in buttons {
                btn |= match b.to_ascii_lowercase().as_str() {
                    "left" => 1,
                    "right" => 2,
                    "middle" => 4,
                    other => return Err(format!("botão de mouse desconhecido: {other}")),
                };
            }
            if btn == 0 {
                return Err("clique de mouse precisa de ao menos um botão".into());
            }
            let m = match modifier.as_deref() {
                None => 0,
                Some("ctrl") => 1,
                Some("shift") => 2,
                Some("alt") => 4,
                Some(other) => return Err(format!("modificador de mouse inválido: {other}")),
            };
            push_msg(out, &[0x03, key_id, (lay << 4) | 0x03, btn, 0, 0, 0, m, 0]);
        }
    }

    // Fim do bind.
    push_msg(out, &[0x03, 0xaa, 0xaa, 0, 0, 0, 0, 0, 0]);
    Ok(())
}

/// Constrói o buffer completo (múltiplo de 64) para uma configuração de camada.
pub fn build_messages(config: &KeyConfig) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for (i, b) in config.buttons.iter().enumerate() {
        build_binding(config.layer, Key::Button(i as u8), b, &mut out)?;
    }
    if let Some(k) = &config.knob {
        build_binding(config.layer, Key::Knob(0, KnobAction::Ccw), &k.ccw, &mut out)?;
        build_binding(config.layer, Key::Knob(0, KnobAction::Press), &k.press, &mut out)?;
        build_binding(config.layer, Key::Knob(0, KnobAction::Cw), &k.cw, &mut out)?;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Transporte HID
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub found: bool,
    pub product: Option<String>,
    pub manufacturer: Option<String>,
    pub interface: i32,
    pub usage_page: u16,
}

/// Pontua um device_info para escolher a interface de configuração correta.
/// A interface de config costuma ser vendor-defined (usage_page 0xff00) e/ou
/// a interface de maior número (a 0 é o "teclado" que digita).
fn score(info: &hidapi::DeviceInfo) -> i32 {
    let mut s = 0;
    if info.usage_page() == 0xff00 {
        s += 1000;
    }
    s += info.interface_number();
    s
}

#[cfg(not(target_os = "linux"))]
fn open_config_device(api: &hidapi::HidApi) -> Result<hidapi::HidDevice, String> {
    let best = api
        .device_list()
        .filter(|i| i.vendor_id() == VID && i.product_id() == PID)
        .max_by_key(|i| score(i))
        .ok_or_else(|| {
            "Tecladinho (1189:8890) não encontrado. Conecte-o e tente de novo.".to_string()
        })?;
    api.open_path(best.path())
        .map_err(|e| format!("falha ao abrir o device: {e}"))
}

/// Lê o device conectado e devolve infos básicas (sem enviar nada).
pub fn detect() -> Result<DeviceInfo, String> {
    let api = hidapi::HidApi::new().map_err(|e| format!("hidapi: {e}"))?;
    match api
        .device_list()
        .filter(|i| i.vendor_id() == VID && i.product_id() == PID)
        .max_by_key(|i| score(i))
    {
        Some(info) => Ok(DeviceInfo {
            found: true,
            product: info.product_string().map(|s| s.to_string()),
            manufacturer: info.manufacturer_string().map(|s| s.to_string()),
            interface: info.interface_number(),
            usage_page: info.usage_page(),
        }),
        None => Ok(DeviceInfo {
            found: false,
            product: None,
            manufacturer: None,
            interface: -1,
            usage_page: 0,
        }),
    }
}

/// Envia o buffer de mensagens para o teclado.
///
/// No Windows usamos hidapi: cada frame de 64 bytes já começa com o report id
/// `0x03` (report numerado) e é escrito direto — o WriteFile exige tamanho ==
/// OutputReportByteLength, então NÃO se prefixa 0x00 (viraria 65 e falha).
#[cfg(not(target_os = "linux"))]
pub fn upload(frames: &[u8]) -> Result<usize, String> {
    if frames.is_empty() {
        return Err("nada para enviar (nenhum bind configurado)".into());
    }
    let api = hidapi::HidApi::new().map_err(|e| format!("hidapi: {e}"))?;
    let dev = open_config_device(&api)?;

    let mut sent = 0usize;
    for chunk in frames.chunks(MSG_LEN) {
        dev.write(chunk).map_err(|e| format!("write falhou: {e}"))?;
        sent += 1;
    }
    Ok(sent)
}

/// Envia o buffer de mensagens para o teclado (Linux, via libusb).
///
/// A interface de config do CH57x só tem endpoint OUT, então o usbhid não a
/// vincula e não existe nó hidraw para o hidapi abrir. Acessamos a interface
/// diretamente por libusb e mandamos cada frame de 64 bytes cru (incluindo o
/// `0x03` inicial) por interrupt OUT — sem semântica de report id de hidraw.
#[cfg(target_os = "linux")]
pub fn upload(frames: &[u8]) -> Result<usize, String> {
    use std::time::Duration;

    if frames.is_empty() {
        return Err("nada para enviar (nenhum bind configurado)".into());
    }

    let handle = rusb::open_device_with_vid_pid(VID, PID).ok_or_else(|| {
        "Tecladinho (1189:8890) não encontrado. Conecte-o e tente de novo.".to_string()
    })?;

    // Localiza a interface de config: a que tem um endpoint interrupt OUT.
    let device = handle.device();
    let config = device
        .active_config_descriptor()
        .map_err(|e| format!("não consegui ler a config USB: {e}"))?;
    let mut target: Option<(u8, u8)> = None; // (interface, endpoint OUT)
    for iface in config.interfaces() {
        for desc in iface.descriptors() {
            for ep in desc.endpoint_descriptors() {
                if ep.direction() == rusb::Direction::Out
                    && ep.transfer_type() == rusb::TransferType::Interrupt
                {
                    target = Some((desc.interface_number(), ep.address()));
                }
            }
        }
    }
    let (iface, endpoint) = target.ok_or_else(|| {
        "interface de config (endpoint OUT) não encontrada no device".to_string()
    })?;

    // A interface de config costuma estar sem driver no Linux; ainda assim
    // pedimos auto-detach por segurança antes de reivindicá-la.
    let _ = handle.set_auto_detach_kernel_driver(true);
    handle
        .claim_interface(iface)
        .map_err(|e| format!("falha ao reivindicar a interface {iface}: {e}"))?;

    let mut sent = 0usize;
    let mut result = Ok(());
    for chunk in frames.chunks(MSG_LEN) {
        if let Err(e) = handle.write_interrupt(endpoint, chunk, Duration::from_millis(1000)) {
            result = Err(format!("write falhou: {e}"));
            break;
        }
        sent += 1;
    }

    let _ = handle.release_interface(iface);
    result.map(|_| sent)
}

/// Dump hexadecimal das mensagens (para preview/debug antes de enviar).
pub fn hex_preview(frames: &[u8]) -> Vec<String> {
    frames
        .chunks(MSG_LEN)
        .map(|c| {
            // só mostra até o último byte não-zero, pra ficar legível
            let end = c.iter().rposition(|&b| b != 0).map(|p| p + 1).unwrap_or(0);
            c[..end]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tabelas de códigos HID
// ---------------------------------------------------------------------------

/// Resolve o nome de uma tecla para seu HID usage code.
/// Aceita nomes conhecidos, caractere único, ou "0xNN" / número cru.
fn key_code(name: &str) -> Result<u8, String> {
    let n = name.trim().to_ascii_lowercase();
    if let Some(hex) = n.strip_prefix("0x") {
        return u8::from_str_radix(hex, 16).map_err(|_| format!("código inválido: {name}"));
    }
    let code = match n.as_str() {
        "a" => 0x04, "b" => 0x05, "c" => 0x06, "d" => 0x07, "e" => 0x08,
        "f" => 0x09, "g" => 0x0a, "h" => 0x0b, "i" => 0x0c, "j" => 0x0d,
        "k" => 0x0e, "l" => 0x0f, "m" => 0x10, "n" => 0x11, "o" => 0x12,
        "p" => 0x13, "q" => 0x14, "r" => 0x15, "s" => 0x16, "t" => 0x17,
        "u" => 0x18, "v" => 0x19, "w" => 0x1a, "x" => 0x1b, "y" => 0x1c,
        "z" => 0x1d,
        "1" => 0x1e, "2" => 0x1f, "3" => 0x20, "4" => 0x21, "5" => 0x22,
        "6" => 0x23, "7" => 0x24, "8" => 0x25, "9" => 0x26, "0" => 0x27,
        "enter" | "return" => 0x28,
        "escape" | "esc" => 0x29,
        "backspace" => 0x2a,
        "tab" => 0x2b,
        "space" => 0x2c,
        "minus" => 0x2d,
        "equal" => 0x2e,
        "leftbracket" => 0x2f,
        "rightbracket" => 0x30,
        "backslash" => 0x31,
        "semicolon" => 0x33,
        "quote" => 0x34,
        "grave" => 0x35,
        "comma" => 0x36,
        "dot" | "period" => 0x37,
        "slash" => 0x38,
        "capslock" => 0x39,
        "f1" => 0x3a, "f2" => 0x3b, "f3" => 0x3c, "f4" => 0x3d, "f5" => 0x3e,
        "f6" => 0x3f, "f7" => 0x40, "f8" => 0x41, "f9" => 0x42, "f10" => 0x43,
        "f11" => 0x44, "f12" => 0x45,
        "printscreen" => 0x46,
        "scrolllock" => 0x47,
        "pause" => 0x48,
        "insert" => 0x49,
        "home" => 0x4a,
        "pageup" => 0x4b,
        "delete" | "del" => 0x4c,
        "end" => 0x4d,
        "pagedown" => 0x4e,
        "right" => 0x4f,
        "left" => 0x50,
        "down" => 0x51,
        "up" => 0x52,
        "numlock" => 0x53,
        "numpadslash" => 0x54,
        "numpadasterisk" => 0x55,
        "numpadminus" => 0x56,
        "numpadplus" => 0x57,
        "numpadenter" => 0x58,
        "numpad1" => 0x59, "numpad2" => 0x5a, "numpad3" => 0x5b, "numpad4" => 0x5c,
        "numpad5" => 0x5d, "numpad6" => 0x5e, "numpad7" => 0x5f, "numpad8" => 0x60,
        "numpad9" => 0x61, "numpad0" => 0x62, "numpaddot" => 0x63,
        "application" | "menu" => 0x65,
        // Teclas "exclusivas": existem no padrão HID mas não em teclados comuns.
        "f13" => 0x68, "f14" => 0x69, "f15" => 0x6a, "f16" => 0x6b,
        "f17" => 0x6c, "f18" => 0x6d, "f19" => 0x6e, "f20" => 0x6f,
        "f21" => 0x70, "f22" => 0x71, "f23" => 0x72, "f24" => 0x73,
        _ => return Err(format!("tecla desconhecida: {name}")),
    };
    Ok(code)
}

/// Resolve o nome de uma tecla de mídia para seu Consumer usage code (u16).
fn media_code(name: &str) -> Result<u16, String> {
    let code = match name.trim().to_ascii_lowercase().as_str() {
        "next" => 0xb5,
        "previous" | "prev" => 0xb6,
        "stop" => 0xb7,
        "play" | "playpause" => 0xcd,
        "mute" => 0xe2,
        "volumeup" | "volup" => 0xe9,
        "volumedown" | "voldown" => 0xea,
        "favorites" => 0x182,
        "calculator" => 0x192,
        "screenlock" => 0x19e,
        _ => return Err(format!("tecla de mídia desconhecida: {name}")),
    };
    Ok(code)
}

/// Lista de nomes válidos, para popular dropdowns no frontend.
pub fn catalog() -> serde_json::Value {
    serde_json::json!({
        "keys": [
            "a","b","c","d","e","f","g","h","i","j","k","l","m","n","o","p","q","r","s","t","u","v","w","x","y","z",
            "1","2","3","4","5","6","7","8","9","0",
            "enter","escape","backspace","tab","space","minus","equal","leftbracket","rightbracket","backslash",
            "semicolon","quote","grave","comma","dot","slash","capslock",
            "f1","f2","f3","f4","f5","f6","f7","f8","f9","f10","f11","f12",
            "printscreen","scrolllock","pause","insert","home","pageup","delete","end","pagedown",
            "right","left","down","up","application",
            "f13","f14","f15","f16","f17","f18","f19","f20","f21","f22","f23","f24"
        ],
        "exclusive": ["f13","f14","f15","f16","f17","f18","f19","f20","f21","f22","f23","f24"],
        "modifiers": ["ctrl","shift","alt","win"],
        "media": ["next","previous","stop","play","mute","volumeup","volumedown","favorites","calculator","screenlock"],
        "mouse_buttons": ["left","right","middle"]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kb(mods: &[&str], code: &str) -> Binding {
        Binding::Keyboard {
            accords: vec![Accord {
                modifiers: mods.iter().map(|s| s.to_string()).collect(),
                code: Some(code.to_string()),
            }],
        }
    }

    #[test]
    fn ctrl_a_matches_reference() {
        let mut out = Vec::new();
        build_binding(0, Key::Button(0), &kb(&["ctrl"], "a"), &mut out).unwrap();
        // start, empty, ctrl+a, finish => 4 frames
        assert_eq!(out.len(), 4 * MSG_LEN);
        assert_eq!(&out[0..5], &[0x03, 0xfe, 0x01, 0x01, 0x01]);
        // frame 2 (empty key)
        assert_eq!(&out[MSG_LEN..MSG_LEN + 4], &[0x03, 0x01, 0x11, 0x01]);
        // frame 3: ctrl+a -> mods 0x01, code 0x04
        assert_eq!(
            &out[2 * MSG_LEN..2 * MSG_LEN + 7],
            &[0x03, 0x01, 0x11, 0x01, 0x01, 0x01, 0x04]
        );
        // finish
        assert_eq!(&out[3 * MSG_LEN..3 * MSG_LEN + 3], &[0x03, 0xaa, 0xaa]);
    }

    #[test]
    fn media_volume_up() {
        let mut out = Vec::new();
        build_binding(0, Key::Button(1), &Binding::Media { code: "volumeup".into() }, &mut out)
            .unwrap();
        assert_eq!(out.len(), 3 * MSG_LEN);
        assert_eq!(&out[MSG_LEN..MSG_LEN + 5], &[0x03, 0x02, 0x12, 0xe9, 0x00]);
    }

    #[test]
    fn knob_ids() {
        assert_eq!(Key::Knob(0, KnobAction::Ccw).id().unwrap(), 13);
        assert_eq!(Key::Knob(0, KnobAction::Press).id().unwrap(), 14);
        assert_eq!(Key::Knob(0, KnobAction::Cw).id().unwrap(), 15);
    }

    #[test]
    fn none_emits_nothing() {
        let mut out = Vec::new();
        build_binding(0, Key::Button(0), &Binding::None, &mut out).unwrap();
        assert!(out.is_empty());
    }
}
