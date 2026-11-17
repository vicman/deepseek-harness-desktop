//! Aviso de versiones nuevas de DSH mediante notificaciones de escritorio.
//!
//! La app solo instala si el usuario lo pide desde la notificacion; nunca en
//! segundo plano sin permiso.

use std::process::{Command, Stdio};
use std::time::Duration;

use tauri::Manager;

use crate::backend::parar_backend;
use crate::dsh::{ejecutar_instalacion, entrypoint_dsh, version_dsh, version_publicada};
use crate::textos::texto;
use crate::versiones::es_mas_nueva;

/// Cada cuanto se vuelve a consultar npm con la app abierta.
const INTERVALO_REVISION: Duration = Duration::from_secs(6 * 60 * 60);

/// Muestra una notificacion de escritorio con `notify-send`.
///
/// Con `acciones` (pares `(nombre, etiqueta)`), la notificacion lleva botones
/// y la llamada BLOQUEA hasta que el usuario pulsa uno o la descarta; devuelve
/// el nombre de la accion pulsada. Si el `notify-send` del sistema (o el
/// servidor de notificaciones) no admite acciones, se muestra
/// `cuerpo_sin_botones` como aviso simple. Si no hay `notify-send`, no pasa
/// nada: la app sigue funcionando igual.
fn notificar(
    titulo: &str,
    cuerpo: &str,
    acciones: &[(&str, &str)],
    cuerpo_sin_botones: &str,
) -> Option<String> {
    // El .deb instala el icono como `dsh-desktop`; `instalar.sh`, como `dsh`.
    let icono = if std::path::Path::new("/usr/share/icons/hicolor/128x128/apps/dsh-desktop.png").is_file() {
        "--icon=dsh-desktop"
    } else {
        "--icon=dsh"
    };
    let base = ["--app-name=DeepSeek Harness", icono];

    if !acciones.is_empty() {
        let salida = Command::new("notify-send")
            .args(base)
            .args(acciones.iter().map(|(n, e)| format!("--action={n}={e}")))
            .args([titulo, cuerpo])
            .stdin(Stdio::null())
            .output();
        if let Ok(o) = salida {
            if o.status.success() {
                // notify-send imprime el nombre de la accion pulsada.
                let pulsada = String::from_utf8_lossy(&o.stdout).trim().to_string();
                return (!pulsada.is_empty()).then_some(pulsada);
            }
        }
        // Sin soporte de acciones: caemos al aviso simple.
    }

    let _ = Command::new("notify-send")
        .args(base)
        .args([titulo, cuerpo_sin_botones])
        .stdin(Stdio::null())
        .output();
    None
}

/// Archivo donde se guarda la version que el usuario decidio omitir.
fn ruta_version_omitida() -> Option<std::path::PathBuf> {
    let datos = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|d| !d.is_empty())
        .or_else(|| std::env::var("HOME").ok().map(|h| format!("{h}/.local/share")))?;
    Some(std::path::Path::new(&datos).join("com.vicmandev.dsh.desktop/version-omitida"))
}

fn version_omitida() -> Option<String> {
    let texto = std::fs::read_to_string(ruta_version_omitida()?).ok()?;
    Some(texto.trim().to_string()).filter(|v| !v.is_empty())
}

fn omitir_version(version: &str) {
    if let Some(ruta) = ruta_version_omitida() {
        if let Some(dir) = ruta.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(ruta, version);
    }
}

/// Revisa al arrancar y luego cada `INTERVALO_REVISION` mientras la app siga
/// abierta, para que quien la deja abierta dias tambien se entere.
///
/// Si el usuario descarta la notificacion, se le vuelve a recordar en la
/// siguiente revision; si eligio «Omitir», no se le vuelve a avisar de esa
/// version. Bloquea: se llama desde un hilo propio.
pub fn vigilar_actualizaciones(app: tauri::AppHandle) {
    loop {
        avisar_de_actualizacion(&app);
        std::thread::sleep(INTERVALO_REVISION);
    }
}

/// Avisa si hay una version mas nueva de DSH.
///
/// El aviso principal es una notificacion de escritorio: la pantalla de carga
/// desaparece en cuanto arranca el servidor, asi que un aviso solo ahi se
/// perdia. La notificacion ofrece «Actualizar» y «Omitir esta version»; tras
/// actualizar ofrece «Reiniciar ahora».
fn avisar_de_actualizacion(app: &tauri::AppHandle) {
    let Some(instalada) = version_dsh() else { return };
    let Some(publicada) = version_publicada() else { return }; // sin red: no molestamos

    if !es_mas_nueva(&publicada, &instalada) {
        return;
    }
    // El usuario ya dijo que esta version no le interesa. Una posterior
    // vuelve a avisar, porque el archivo guarda una version concreta.
    if version_omitida().as_deref() == Some(publicada.as_str()) {
        return;
    }

    // Tambien en la pantalla de carga, por si todavia esta visible. Si ya se
    // navego a la GUI, `dshAviso` no existe alli y el eval no hace nada.
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.eval(format!(
            "window.dshAviso && window.dshAviso('avisoVersion', {{publicada: '{publicada}'}});"
        ));
    }

    let datos = [("instalada", instalada.as_str()), ("publicada", publicada.as_str())];
    let t = |clave: &str| texto(&format!("notificacion.{clave}"), &datos);

    let (boton, omitir) = (t("boton"), t("omitir"));
    let acciones = [("actualizar", boton.as_str()), ("omitir", omitir.as_str())];
    match notificar(&t("titulo"), &t("cuerpo"), &acciones, &t("manual")).as_deref() {
        Some("actualizar") => {}
        Some("omitir") => {
            omitir_version(&publicada);
            return;
        }
        _ => return, // descartada: se recordara en la proxima revision
    }

    notificar(&t("titulo"), &t("actualizando"), &[], &t("actualizando"));
    // Si la copia actual no es de pnpm, se instalo con npm: se actualiza con
    // npm para no dejar dos copias distintas.
    let npm_primero = entrypoint_dsh().is_none();
    // Lista cuando la version instalada (resuelta de nuevo, pues el gestor
    // la mueve) ya no es mas vieja que la publicada.
    let ok = ejecutar_instalacion(npm_primero, || {
        version_dsh().map(|v| !es_mas_nueva(&publicada, &v)).unwrap_or(false)
    });
    if !ok {
        notificar(&t("titulo"), &t("fallo"), &[], &t("fallo"));
        return;
    }

    let reiniciar = t("reiniciar");
    let pulsada = notificar(&t("titulo"), &t("listo"), &[("reiniciar", &reiniciar)], &t("listo"));
    if pulsada.as_deref() == Some("reiniciar") {
        // Se para el servidor a mano: al reiniciar no siempre llega el evento
        // de ventana destruida que lo termina normalmente.
        parar_backend(app);
        app.restart();
    }
}
