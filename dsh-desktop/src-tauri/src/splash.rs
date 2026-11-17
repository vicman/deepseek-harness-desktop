//! Pantalla de carga: servidor local y recursos de `ui/`.

use std::io::Write;
use std::net::TcpListener;

/// Sirve la pantalla de carga en `127.0.0.1`, en su propio puerto.
///
/// Devuelve el puerto. El HTML queda embebido en el binario para que la
/// ventana pueda nacer ya en `127.0.0.1` (mismo sitio que la GUI) y la
/// cookie `SameSite=Strict` no se bloquee al navegar despues.
pub fn servidor_splash() -> u16 {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(l) => l,
        Err(_) => return 0,
    };
    let puerto = listener.local_addr().map(|a| a.port()).unwrap_or(0);

    std::thread::spawn(move || {
        for flujo in listener.incoming() {
            let mut s = match flujo {
                Ok(s) => s,
                Err(_) => continue,
            };
            // Leemos la peticion para saber que recurso se pide.
            // Solo UNA lectura: si se leyera dos veces, la primera consumiria
            // la peticion y la segunda devolveria 0 bytes, de modo que
            // siempre se serviria el HTML y nunca el logotipo.
            let mut buf = [0u8; 2048];
            let n = std::io::Read::read(&mut s, &mut buf).unwrap_or(0);
            let peticion = String::from_utf8_lossy(&buf[..n]).to_string();

            // El logotipo se sirve como archivo aparte, no incrustado en el
            // HTML: un base64 grande obliga al navegador a decodificarlo
            // antes de pintar y dejaba un hueco visible sin el logo.
            let (tipo, cuerpo): (&str, Vec<u8>) = if peticion.contains("logo-splash.png") {
                ("image/png", cargar_logo())
            } else {
                ("text/html; charset=utf-8", cargar_splash().into_bytes())
            };

            let cabecera = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {tipo}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
                cuerpo.len()
            );
            let _ = std::io::Write::write_all(&mut s, cabecera.as_bytes());
            let _ = std::io::Write::write_all(&mut s, &cuerpo);
            let _ = s.flush();
        }
    });

    puerto
}

/// Candidatos en disco para un recurso de `ui/` (splash, logotipo...).
///
/// Solo se buscan junto al binario y en el arbol del proyecto (ejecucion
/// desde `target/debug` o `target/release`), para poder ajustar el diseno sin
/// recompilar. Una app instalada usa siempre la copia incrustada: antes se
/// miraba tambien en `~/.local/share` y `/usr/share`, y una copia vieja que
/// quedo de una instalacion anterior tapaba a la buena.
pub fn rutas_ui(nombre: &str) -> Vec<std::path::PathBuf> {
    let Ok(exe) = std::env::current_exe() else { return Vec::new() };
    let Some(dir) = exe.parent() else { return Vec::new() };
    // Desde target/release hay que subir tres niveles hasta la raiz del
    // proyecto (cuatro desde target/debug/deps, donde corren los tests).
    ["ui", "../ui", "../../ui", "../../../ui", "../../../../ui"]
        .iter()
        .map(|sub| dir.join(sub).join(nombre))
        .collect()
}

/// Copias de respaldo incrustadas en el binario, por si faltan los archivos
/// (por ejemplo en un AppImage, que no instala nada en `/usr/share`).
const SPLASH_INCRUSTADO: &str = include_str!("../../ui/splash.html");
const LOGO_INCRUSTADO: &[u8] = include_bytes!("../../ui/logo-splash.png");

/// Marcador de `splash.html` que se sustituye por `ui/textos.json`.
const MARCADOR_TEXTOS: &str = "/*TEXTOS*/null";

/// Carga la pantalla de carga desde `ui/splash.html`, con los textos ya
/// inyectados.
///
/// Se prefiere el archivo en disco (asi se puede ajustar el diseno sin
/// recompilar) y si no esta se usa la copia incrustada.
pub fn cargar_splash() -> String {
    // Una copia en disco sin el marcador es de una version anterior: se
    // descarta, porque sin textos la pagina quedaria a medias.
    let html = rutas_ui("splash.html")
        .iter()
        .filter_map(|r| std::fs::read_to_string(r).ok())
        .find(|t| t.contains(MARCADOR_TEXTOS))
        .unwrap_or_else(|| SPLASH_INCRUSTADO.to_string());
    html.replace(MARCADOR_TEXTOS, crate::textos::TEXTOS_JSON.trim())
}

/// Carga el logotipo de la pantalla de carga como bytes PNG.
///
/// Se sirve como recurso aparte en lugar de incrustarlo en el HTML: un base64
/// grande obliga al navegador a decodificarlo antes de pintar, y ese hueco se
/// veia como un destello claro sin el logo.
pub fn cargar_logo() -> Vec<u8> {
    rutas_ui("logo-splash.png")
        .iter()
        .find_map(|r| std::fs::read(r).ok())
        .unwrap_or_else(|| LOGO_INCRUSTADO.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_splash_lleva_los_textos_inyectados() {
        assert!(SPLASH_INCRUSTADO.contains(MARCADOR_TEXTOS), "falta el marcador en splash.html");
        let html = cargar_splash();
        assert!(!html.contains(MARCADOR_TEXTOS));
        assert!(html.contains("\"notificacion\""));
    }
}
