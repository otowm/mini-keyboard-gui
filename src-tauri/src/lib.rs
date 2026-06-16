mod keyboard;

use keyboard::{DeviceInfo, KeyConfig};

/// Detecta se o tecladinho está conectado.
#[tauri::command]
fn detect_keyboard() -> Result<DeviceInfo, String> {
    keyboard::detect()
}

/// Envia a configuração para o teclado.
#[tauri::command]
fn upload_config(config: KeyConfig) -> Result<usize, String> {
    let frames = keyboard::build_messages(&config)?;
    keyboard::upload(&frames)
}

/// Gera o dump hex das mensagens sem enviar (dry-run / debug).
#[tauri::command]
fn preview_config(config: KeyConfig) -> Result<Vec<String>, String> {
    let frames = keyboard::build_messages(&config)?;
    Ok(keyboard::hex_preview(&frames))
}

/// Catálogo de teclas/modificadores/mídia para popular a UI.
#[tauri::command]
fn key_catalog() -> serde_json::Value {
    keyboard::catalog()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            detect_keyboard,
            upload_config,
            preview_config,
            key_catalog
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
