# Mini Keyboard GUI

App **multiplataforma** (Windows + Linux) para configurar o macro pad **CH57x** —
aquele teclado mecânico mini com **3 teclas + 1 knob/encoder** (chip WCH CH57x,
VID `0x1189` / PID `0x8890`, também vendido como `8840`/`8842`).

Substitui o utilitário chinês original (`MINI KeyBoard.exe`, O mapeamento é
gravado **na memória do próprio teclado** — depois de gravar, ele funciona em
qualquer computador, sem precisar do app rodando.

> Protocolo HID portado de
> [kriomant/ch57x-keyboard-tool](https://github.com/kriomant/ch57x-keyboard-tool).

---

## Funcionalidades

- 🔍 **Detecção automática** do dispositivo conectado.
- 🎹 **Editor visual** do layout (knob + 3 teclas), com orientação configurável
  (suporta usar o pad girado 180°).
- ⌨️ Três tipos de binding por tecla/ação:
  - **Teclado** — teclas + modificadores (`Ctrl`, `Shift`, `Alt`, `Win`).
  - **Mídia** — teclas de consumer/multimídia (play, volume, etc.).
  - **Mouse** — botões e roda.
- 🎯 **Captura de tecla** — clique em "Capturar tecla" e pressione a combinação.
- 🎛️ **Knob/encoder** — ações independentes para girar à esquerda (ccw),
  pressionar e girar à direita (cw).
- 🧩 **Teclas F13–F24** disponíveis — ideais para atalhos que não conflitam com
  nada (ative via AutoHotkey no Windows ou input-remapper/xbindkeys no Linux).
- 👁️ **Preview dos bytes** HID que serão enviados, antes de gravar.
- 💾 **Gravação no hardware** — confirmada funcionando no **Windows** e no
  **Linux (CachyOS/Arch)**.

---

## Stack

- **[Tauri v2](https://tauri.app/)** — backend **Rust** + frontend **vanilla
  TypeScript** (sem framework), empacotado com **Vite**.
- **HID:** crate [`hidapi`](https://crates.io/crates/hidapi) no Windows; no Linux
  a gravação usa **libusb/`rusb`** direto (ver [AGENTS.md](./AGENTS.md) §3 para o
  porquê) e a detecção usa hidraw.

---

## Instalar (usuário final)

### Windows

Baixe e rode um dos instaladores gerados (ou pegue na aba *Releases* do repo):

- **`mini-keyboard-gui_x.y.z_x64_en-US.msi`** — instalador MSI.
- **`mini-keyboard-gui_x.y.z_x64-setup.exe`** — instalador NSIS.

Não precisa instalar drivers: o CH57x usa o driver HID nativo do Windows.

### Linux (CachyOS / Arch e derivados)

Instale o `.deb`/`.rpm`/`.AppImage` gerado pelo build e aplique as **regras
udev** (são duas — uma para detecção via hidraw, outra para gravação via
libusb). Os comandos completos estão em [AGENTS.md](./AGENTS.md) §5. Resumo:

```bash
# detecção (hidraw) + gravação (usb)
echo 'KERNEL=="hidraw*", ATTRS{idVendor}=="1189", ATTRS{idProduct}=="8890", MODE="0666"' \
  | sudo tee /etc/udev/rules.d/99-ch57x-hidraw.rules
echo 'SUBSYSTEM=="usb", ATTRS{idVendor}=="1189", ATTRS{idProduct}=="8890", MODE="0666"' \
  | sudo tee /etc/udev/rules.d/99-ch57x.rules
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Depois **reconecte o USB** para as permissões valerem.

---

## Desenvolvimento

Pré-requisitos: **Rust + cargo** e **Node + npm**.

```bash
npm install            # deps do frontend (uma vez)
npm run tauri dev      # abre a janela com hot-reload do frontend
npm run build          # typecheck (tsc) + bundle do frontend
npm run tauri build    # gera binário + instaladores
cd src-tauri && cargo test   # testes do protocolo HID
```

### Build de release

```bash
npm run tauri build
```

**Windows** → `src-tauri/target/release/`:
- `mini-keyboard-gui.exe`
- `bundle/msi/mini-keyboard-gui_x.y.z_x64_en-US.msi`
- `bundle/nsis/mini-keyboard-gui_x.y.z_x64-setup.exe`

Requer **VS Build Tools (workload C++/MSVC)** + WebView2 (já vem no Win10/11).

**Linux** → `src-tauri/target/release/bundle/{deb,rpm,appimage}/`.

> ⚠️ No CachyOS/Arch o passo do AppImage pode falhar por falta do módulo `fuse`.
> Contorne com:
> ```bash
> APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=true npm run tauri build
> ```
> Detalhes em [AGENTS.md](./AGENTS.md) §5.

---

## Estrutura do projeto

```
src-tauri/src/keyboard.rs   Protocolo HID + transporte + tabelas de códigos.
src-tauri/src/lib.rs        Comandos Tauri (detect, upload, preview, catalog).
src-tauri/tauri.conf.json   Config da janela e do bundle.
index.html                  Estrutura da UI.
src/main.ts                 Lógica do frontend (estado, render, captura, IPC).
src/styles.css              Tema dark.
```

Documentação técnica completa — protocolo HID byte a byte, arquitetura, contrato
frontend↔backend e pegadinhas de plataforma — está em **[AGENTS.md](./AGENTS.md)**.

---

## Créditos

- Protocolo HID baseado em
  [kriomant/ch57x-keyboard-tool](https://github.com/kriomant/ch57x-keyboard-tool).
- Construído com [Tauri](https://tauri.app/).
