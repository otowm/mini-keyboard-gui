# AGENTS.md — Mini Keyboard GUI

Guia para agentes de IA (Claude Code, Codex, etc.) trabalharem neste repositório.
Leia isto **inteiro** antes de editar — ele contém detalhes de protocolo de hardware
que não são óbvios pelo código e que, se ignorados, quebram a gravação no dispositivo.

---

## 1. O que é o projeto

App **multiplataforma** (Windows + Linux) para configurar um **macro pad CH57x**
(3 teclas mecânicas + 1 knob/encoder). Substitui o utilitário chinês original
"MINI KeyBoard.exe" (só Windows, interface ruim). Objetivo: UI melhor e funcionar
no Linux (o dono usa **CachyOS**, Arch-based).

- **Stack:** Tauri v2 — backend Rust + frontend vanilla TypeScript (sem framework).
- **HID:** crate `hidapi` (HID nativo no Windows, hidraw no Linux; sem trocar driver).
- **Estado:** funcional. Detecção, edição visual, preview de bytes e **gravação no
  hardware já funcionam** — confirmado pelo dono **no Windows e no Linux (CachyOS)**.
  No Linux a gravação usa libusb (ver §3); a detecção usa hidraw.

---

## 2. O hardware

- **USB ID:** VID `0x1189`, PID `0x8890` (chip WCH CH57x). Modelos irmãos `8840`/`8842`.
- **Layout físico:** 1 fileira → knob + 3 teclas. O dono **usa girado 180°**:
  knob à esquerda, teclas na ordem **3·2·1**, e o giro do knob fica espelhado.
  Por isso a UI tem uma orientação `reversed` (padrão) — ver §6.
- O mapeamento é gravado **na memória do próprio teclado**. Depois de gravar, ele
  funciona em qualquer computador sem software rodando.

---

## 3. Protocolo HID (CRÍTICO)

Portado de [kriomant/ch57x-keyboard-tool](https://github.com/kriomant/ch57x-keyboard-tool)
(`src/keyboard/k8890.rs`). Implementado em `src-tauri/src/keyboard.rs`.

Cada mensagem tem **64 bytes**, começa com `0x03`. Um bind = início + corpo + fim:

- **início:** `03 fe <layer+1> 01 01 00 00 00 00`
- **corpo** (um destes):
  - teclado: primeiro um toque vazio `(0,0)`, depois cada accord →
    `03 <key_id> ((layer+1)<<4 | 1) <len> <i> <mods> <code> 00 00`
  - mídia: `03 <key_id> ((layer+1)<<4 | 2) <low> <high> 00 00 00 00` (code u16 LE, Consumer page)
  - mouse: `03 <key_id> ((layer+1)<<4 | 3) <buttons> <dx> <dy> <wheel> <mod> 00`
- **fim:** `03 aa aa 00 00 00 00 00 00`

**key_id:** botões = `index+1` (1-based). Knob `n`, ação: `12 + 1 + 3*n + ação`,
onde ccw=0, press=1, cw=2 → knob 0 = ids **13/14/15**. Macros: máx 5 toques.

**Bytes:** modificadores → ctrl=bit0, shift=1, alt=2, win=3, depois as variantes
direitas 4–7. Mouse modifier: ctrl=1/shift=2/alt=4. Mouse buttons: left=1/right=2/middle=4.

### ⚠️ Enquadramento do write (o transporte difere por plataforma!)

O `upload()` em `keyboard.rs` tem **dois caminhos** (`#[cfg(...)]`). O conteúdo dos
64 bytes é o mesmo; o que muda é como eles chegam ao device.

**Windows — hidapi (report HID numerado, report id `0x03`, OutputReportByteLength 64).**
O `WriteFile` exige tamanho **exato**. Portanto:

- **Escreva os 64 bytes direto** com `dev.write(chunk)` — o `0x03` já é o report id.
- **NÃO** prefixe `0x00` (vira 65 bytes → `WriteFile 0x57 ERROR_INVALID_PARAMETER`).
- **NÃO** mande um "prime" de zeros: report id `0x00` é inválido aqui. O prime de
  zeros do ch57x era artefato do transporte libusb, não do HID.

A interface de config no Windows é escolhida em `open_config_device()`: a de maior
`usage_page==0xff00` / maior `interface_number` (a interface 0 é o "teclado" que digita).

**Linux — libusb/rusb (NÃO dá pra usar hidraw).** A interface de config do CH57x
(a interface **1**, a única com endpoint **OUT**) só tem endpoint OUT, e o `usbhid`
do kernel **se recusa a vincular** interfaces HID sem um interrupt IN. Resultado: ela
fica **órfã, sem nó `/dev/hidrawN`**, e o hidapi nem a enxerga — abrir as outras
interfaces e escrever dá **EPIPE / "broken pipe"** (elas não têm endpoint OUT).
Por isso o Linux usa **libusb direto**: acha a interface com endpoint interrupt OUT,
faz `claim_interface`, e manda cada frame de 64 bytes cru (com o `0x03`) por
`write_interrupt`. Sem semântica de report id — é transferência crua no endpoint.
Não tente "consertar" isso voltando pra hidapi no Linux; o nó hidraw não existe.

---

## 4. Arquitetura e arquivos

```
src-tauri/src/keyboard.rs   Protocolo + transporte hidapi + tabelas de códigos HID.
                            build_messages(), upload(), detect(), hex_preview(), catalog().
                            Tem testes (cargo test) que conferem bytes vs. referência.
src-tauri/src/lib.rs        Comandos Tauri: detect_keyboard, upload_config,
                            preview_config, key_catalog.
src-tauri/tauri.conf.json   Config da janela (1040x660, minWidth 1000), bundle.
index.html                  Estrutura da UI (topbar, #pad-row vazio, editor, footer).
src/main.ts                 Toda a lógica do frontend (estado, render, captura, IPC).
src/styles.css              Tema dark; .key são quadrados estilo keycap (92x92).
```

**Contrato frontend → backend** (`upload_config`/`preview_config` recebem `{ config }`):

```ts
config = {
  layer: number,                       // 0..15
  buttons: Binding[],                  // por id de firmware (índice 0 = botão 1)
  knob: { ccw: Binding, press: Binding, cw: Binding } | null
}
type Binding =
  | { type: "none" }
  | { type: "keyboard", accords: { modifiers: string[], code: string|null }[] }
  | { type: "media", code: string }
  | { type: "mouse", buttons: string[], modifier?: string|null }
```

Nomes válidos de tecla/mídia/modificador vêm do comando `key_catalog` (Rust → JSON).
Teclas vão por nome (ex.: `"a"`, `"f5"`, `"f13"`, `"enter"`); aceita também `"0xNN"`.

---

## 5. Build & Run

Pré-requisitos comuns: **Rust + cargo**, **Node + npm**.

```bash
npm install                 # uma vez, deps do frontend
npm run tauri dev           # dev (abre a janela, com hot-reload do frontend)
npm run build               # typecheck (tsc) + bundle do frontend (sem app)
npm run tauri build         # gera binário/instalador em src-tauri/target/release/
cd src-tauri && cargo test  # testes do protocolo (recomendado após mexer no protocolo)
```

### Windows
Precisa das **VS Build Tools (workload C++/MSVC)** + WebView2 (vem no Win10/11).

### Linux (CachyOS / Arch) — destino atual
Dependências de sistema do Tauri v2 + libs p/ hidapi (hidraw usa libudev):

```bash
sudo pacman -S --needed \
  webkit2gtk-4.1 base-devel curl wget file openssl \
  libappindicator-gtk3 librsvg gtk3 systemd-libs
# Rust e Node: rustup + nodejs/npm (ou via pacman/paru)
```

**Regras udev** — são **DUAS**, e fazem coisas diferentes (ver §3):

```bash
# 1) hidraw: a DETECÇÃO (hidapi enumera/lê as interfaces de teclado/mouse)
echo 'KERNEL=="hidraw*", ATTRS{idVendor}=="1189", ATTRS{idProduct}=="8890", MODE="0666"' \
  | sudo tee /etc/udev/rules.d/99-ch57x-hidraw.rules
# 2) usb: a GRAVAÇÃO (libusb/rusb abre o device e fala com a interface de config)
echo 'SUBSYSTEM=="usb", ATTRS{idVendor}=="1189", ATTRS{idProduct}=="8890", MODE="0666"' \
  | sudo tee /etc/udev/rules.d/99-ch57x.rules
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Depois de aplicar, **reconecte o USB** para as novas permissões valerem.

Notas Linux:
- Sem a regra **hidraw**, `/dev/hidrawN` fica root-only e a detecção falha.
- Sem a regra **usb**, o nó usbfs (`/dev/bus/usb/BBB/DDD`) fica root-only e o
  `upload` por libusb falha com acesso negado.
- A interface de config (interface 1) **não tem nó hidraw** no Linux — é por isso
  que o write vai por libusb e não por hidapi (ver §3, "Enquadramento do write").
- Se o build do `hidapi`/`rusb` reclamar, confirme `base-devel` (compilador C),
  libudev (systemd) e `libusb` (1.0).

#### Empacotar no Linux — pegadinha do FUSE no AppImage (CachyOS/Arch)

`npm run tauri build` gera `.deb`, `.rpm` e `.AppImage`. O `.deb`/`.rpm` saem sempre.
O passo do **AppImage** chama o `linuxdeploy` — que é, ele próprio, um AppImage e
**precisa do módulo `fuse` do kernel para se montar**. No CachyOS o módulo costuma
**não estar carregado** (mesmo com `fuse2` instalado), e o build falha com:

```
failed to bundle project `failed to run linuxdeploy`
```

Fix (sem sudo, sem mexer no kernel) — manda o linuxdeploy se *extrair* em vez de montar:

```bash
APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=true npm run tauri build
```

Alternativa: `sudo modprobe fuse` (persistir em `/etc/modules-load.d/fuse.conf`).

**Onde sai:** `src-tauri/target/release/bundle/{deb,rpm,appimage}/` (fundo na árvore;
IDEs ocultam `target/`). O `.AppImage` é grande (~100 MB) porque embute o WebKitGTK;
o `.deb`/`.rpm` (~4 MB) dependem das libs do sistema.

**Rodar o AppImage gerado** também exige FUSE em runtime — sem o módulo, use
`./mini-keyboard-gui_*.AppImage --appimage-extract-and-run`.

---

## 6. Funcionalidades específicas (não remover sem entender)

- **Orientação (`state.orientation`, padrão `reversed`):** é **só apresentação**.
  `renderDevice()` desenha o knob à esquerda e as teclas 3·2·1, e no modo reversed
  troca qual ação de firmware (cw↔ccw) os controles de giro esquerdo/direito acionam.
  Os bindings continuam guardados/enviados pelo **id de firmware** — não mexa nisso
  achando que é bug. Há um checkbox "Usar invertido" na UI.

- **Teclas exclusivas F13–F24** (HID `0x68`–`0x73`): existem no padrão mas nenhum
  teclado comum tem e nenhum app usa por padrão. São a forma de ter um botão que não
  conflita com nada. O usuário liga a ação via AutoHotkey (Windows) ou
  input-remapper/xbindkeys (Linux). Estão no catálogo e nos chips da UI.

- **Captura de tecla:** botão "🎯 Capturar tecla" escuta o `keydown` e mapeia
  `KeyboardEvent.code` → nome do catálogo (`jsKeyToName` em `main.ts`), pegando os
  modificadores juntos.

---

## 7. Próximos passos / ideias em aberto

- **Camadas:** o firmware suporta layers 0–15; o knob normalmente alterna 3 perfis.
  Hoje a UI edita só a layer 0. Backend já aceita `layer`. Falta UI de camadas.
- **Macros multi-tecla:** backend já suporta até 5 accords por bind; a UI hoje edita
  só 1 accord por tecla. Expandir o editor.
- **Salvar/abrir perfis** (JSON local) e talvez exportar a config equivalente do
  ch57x-keyboard-tool (YAML).
- **Empacotar:** Linux (`.deb`/`.rpm`/`.AppImage`) já gera OK — ver a pegadinha do
  FUSE no §5. Falta gerar/validar o Windows (`.msi`).

---

## 8. Convenções

- Mensagens da UI em **pt-BR**. Código/identificadores em inglês ou pt, siga o arquivo.
- Após mexer no protocolo (`keyboard.rs`), rode `cargo test` — os testes comparam bytes
  com a referência conhecida (ex.: `Ctrl+A` → `03 01 11 01 01 01 04`).
- Frontend é vanilla TS sem framework; mantenha assim salvo pedido explícito.
- Não reintroduza o "prime" de zeros nem o prefixo `0x00` no write (ver §3).
