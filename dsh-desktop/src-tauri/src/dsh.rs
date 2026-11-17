//! Localizar, instalar y versionar el DSH instalado en el equipo.

use std::process::{Command, Stdio};

/// Localiza el binario `dsh` en el PATH del usuario.
///
/// Una app lanzada desde el menú no hereda el PATH del shell interactivo,
/// así que probamos las ubicaciones habituales de pnpm/npm/npx.
pub fn ruta_dsh() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let candidatos = [
        format!("{home}/.local/share/pnpm/bin/dsh"),
        format!("{home}/.local/bin/dsh"),
        format!("{home}/.bun/bin/dsh"),
        "/usr/local/bin/dsh".to_string(),
        "/usr/bin/dsh".to_string(),
    ];
    for c in candidatos.iter() {
        if std::path::Path::new(c).is_file() {
            return Some(c.clone());
        }
    }

    // Ultimo recurso: confiar en el PATH. Si tampoco aparece, devolvemos
    // None para que quien llame sepa que no hay nada instalado (antes se
    // devolvia "dsh" siempre, y eso impedia detectar la ausencia).
    let salida = Command::new("sh")
        .arg("-c")
        .arg("command -v dsh")
        .env("PATH", path_ampliado())
        .output()
        .ok()?;
    if salida.status.success() {
        let ruta = String::from_utf8_lossy(&salida.stdout).trim().to_string();
        if !ruta.is_empty() {
            return Some(ruta);
        }
    }
    None
}

/// Localiza el directorio bin del Node más moderno instalado por nvm.
///
/// DSH requiere Node ≥ 22 (`node:util.parseEnv`); el Node del sistema
/// puede ser más antiguo y romper el arranque.
pub fn dir_node() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let raiz = format!("{home}/.nvm/versions/node");
    let mut versiones: Vec<String> = std::fs::read_dir(&raiz)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    versiones.sort_by(|a, b| {
        let parse = |s: &str| -> Vec<u32> {
            s.trim_start_matches('v')
                .split('.')
                .filter_map(|p| p.parse().ok())
                .collect()
        };
        parse(b).cmp(&parse(a))
    });
    for v in versiones {
        let bin = format!("{raiz}/{v}/bin");
        if std::path::Path::new(&format!("{bin}/node")).is_file() {
            return Some(bin);
        }
    }
    None
}

/// PATH ampliado para que el proceso hijo encuentre node y dsh.
pub fn path_ampliado() -> String {
    let actual = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();
    let node = dir_node().unwrap_or_default();
    format!("{node}:{home}/.local/share/pnpm/bin:{home}/.local/bin:{home}/.bun/bin:{actual}")
}

/// Version mayor minima de Node que necesita DSH (`node:util.parseEnv`).
pub const NODE_MINIMO: u32 = 22;

/// Version del `node` que usara DSH (`v24.1.0`), o `None` si no hay ninguno.
///
/// Se busca con el mismo PATH ampliado con el que se arranca DSH, asi que es
/// exactamente el Node que se va a usar: el mas moderno de nvm si lo hay.
pub fn version_node() -> Option<String> {
    let salida = Command::new("node")
        .arg("--version")
        .env("PATH", path_ampliado())
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !salida.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&salida.stdout).trim().to_string()).filter(|v| !v.is_empty())
}

/// Si la version de Node (`v24.1.0`) es suficiente para DSH.
pub fn node_suficiente(version: &str) -> bool {
    version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()
        .and_then(|m| m.parse::<u32>().ok())
        .is_some_and(|m| m >= NODE_MINIMO)
}

/// Localiza el entrypoint real `@deepseek-ai/dsh/lib/bin.js` en el store de pnpm.
///
/// El store conserva directorios de instalaciones anteriores con la misma
/// version de dsh, y `read_dir` no garantiza orden. Para no arrancar un DSH
/// obsoleto, resolvemos primero la ruta que el shim global tiene escrita:
/// es la unica que coincide con lo que el usuario ejecuta en su terminal.
pub fn entrypoint_dsh() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let global = format!("{home}/.local/share/pnpm/global");
    const BIN_JS: &str = "node_modules/@deepseek-ai/dsh/lib/bin.js";

    // 1) La referencia del shim global es la fuente de verdad.
    let shim = format!("{home}/.local/share/pnpm/bin/dsh");
    if let Ok(contenido) = std::fs::read_to_string(&shim) {
        if let Some(dir) = dir_global_del_shim(&contenido) {
            let candidato = format!("{global}/{dir}/{BIN_JS}");
            if std::path::Path::new(&candidato).is_file() {
                return Some(candidato);
            }
        }
    }

    // 2) Respaldo: el directorio modificado mas recientemente, en cualquier
    //    version del layout de pnpm (global/v10, global/v11, ...).
    let mut candidatos: Vec<(std::time::SystemTime, String)> = Vec::new();
    for layout in std::fs::read_dir(&global).ok()?.filter_map(|e| e.ok()) {
        let Ok(entradas) = std::fs::read_dir(layout.path()) else { continue };
        for entrada in entradas.filter_map(|e| e.ok()) {
            let ruta = entrada.path().join(BIN_JS);
            if ruta.is_file() {
                let t = entrada
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                candidatos.push((t, ruta.to_string_lossy().to_string()));
            }
        }
    }
    candidatos.sort_by(|a, b| b.0.cmp(&a.0));
    candidatos.into_iter().next().map(|(_, r)| r)
}

/// Extrae del shim de pnpm el directorio global al que apunta, como
/// `v11/<hash>`. Si aparecen varios, se queda con el de hash mas largo (los
/// cortos son restos de instalaciones antiguas).
pub fn dir_global_del_shim(contenido: &str) -> Option<String> {
    let mut mejor: Option<(String, String)> = None;
    for parte in contenido.split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '/') {
        for resto in parte.split("/global/").skip(1) {
            let mut seg = resto.split('/');
            let (Some(layout), Some(hash)) = (seg.next(), seg.next()) else { continue };
            let es_layout = layout.len() > 1
                && layout.starts_with('v')
                && layout[1..].chars().all(|c| c.is_ascii_digit());
            if !es_layout || hash.len() <= 16 {
                continue;
            }
            if mejor.as_ref().is_none_or(|(_, h)| hash.len() > h.len()) {
                mejor = Some((layout.to_string(), hash.to_string()));
            }
        }
    }
    mejor.map(|(l, h)| format!("{l}/{h}"))
}

/// Version de DSH instalada, venga de pnpm o de npm.
///
/// Con pnpm se lee del entrypoint del store. Con npm (u otro gestor) el
/// comando `dsh` es un enlace simbolico a `lib/bin.js`: se resuelve el enlace
/// y se sube hasta el `package.json` del paquete.
pub fn version_dsh() -> Option<String> {
    if let Some(v) = entrypoint_dsh().and_then(|e| version_instalada(&e)) {
        return Some(v);
    }
    let real = std::fs::canonicalize(ruta_dsh()?).ok()?;
    version_del_paquete(&real)
}

/// Sube desde `ruta` hasta encontrar el `package.json` de `@deepseek-ai/dsh`
/// y devuelve su version.
pub fn version_del_paquete(ruta: &std::path::Path) -> Option<String> {
    for dir in ruta.ancestors().skip(1) {
        let Ok(texto) = std::fs::read_to_string(dir.join("package.json")) else { continue };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&texto) else { continue };
        if json.get("name").and_then(|n| n.as_str()) == Some("@deepseek-ai/dsh") {
            return json.get("version")?.as_str().map(|s| s.to_string());
        }
    }
    None
}

/// Version de DSH que hay instalada, leida de su package.json.
pub fn version_instalada(entrypoint: &str) -> Option<String> {
    // <...>/node_modules/@deepseek-ai/dsh/lib/bin.js
    //   -> subimos a la raiz del paquete y leemos package.json
    let paquete = std::path::Path::new(entrypoint)
        .parent()? // lib/
        .parent()? // raiz del paquete
        .join("package.json");
    let texto = std::fs::read_to_string(paquete).ok()?;
    let json: serde_json::Value = serde_json::from_str(&texto).ok()?;
    json.get("version")?.as_str().map(|s| s.to_string())
}

/// Version estable mas reciente publicada en npm.
///
/// Se consulta el registro directamente para no depender de que `npm` este en
/// el PATH. Devuelve `None` si no hay red o el registro no responde: en ese
/// caso la aplicacion sigue arrancando con lo que haya instalado.
pub fn version_publicada() -> Option<String> {
    let dir = "registry.npmjs.org:443";
    let respuesta = https_get(dir, "/@deepseek-ai%2Fdsh/latest")?;
    let json: serde_json::Value = serde_json::from_str(&respuesta).ok()?;
    json.get("version")?.as_str().map(|s| s.to_string())
}

/// Peticion HTTPS GET minima y sin dependencias externas.
///
/// Solo se usa para consultar el registro de npm; si algo falla devuelve
/// `None` y la aplicacion continua sin avisar de actualizaciones.
pub fn https_get(dir: &str, ruta: &str) -> Option<String> {
    // Resolucion DNS + conexion TLS mediante `curl`, presente en cualquier
    // escritorio Linux. Asi no arrastramos un cliente TLS al binario.
    let salida = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "8",
            "-H",
            "Accept: application/json",
            &format!("https://{dir}{ruta}"),
        ])
        .stdin(Stdio::null())
        .output()
        .ok()?;

    if !salida.status.success() {
        return None;
    }
    String::from_utf8(salida.stdout).ok()
}

/// Instala DSH globalmente si no esta presente.
///
/// Se ejecuta solo cuando `entrypoint_dsh()` y `ruta_dsh()` no encuentran
/// nada: en ese caso la aplicacion no tendria backend con el que arrancar.
/// Se usa pnpm si esta disponible (es el gestor que DSH documenta) y si no,
/// npm. Devuelve `true` si la instalacion termino bien.
pub fn instalar_dsh_si_falta() -> bool {
    if entrypoint_dsh().is_some() || ruta_dsh().is_some() {
        return true; // ya esta instalado
    }
    // Basta con que el entrypoint aparezca: es la comprobacion que realmente
    // importa, no el codigo de salida del gestor.
    ejecutar_instalacion(false, || entrypoint_dsh().is_some())
}

/// Ejecuta `pnpm add -g @deepseek-ai/dsh` (con npm como respaldo) hasta que
/// `listo()` confirme el resultado. Sirve para instalar y para actualizar:
/// sin version explicita, el gestor instala la etiqueta `latest`.
///
/// Con `npm_primero` se prueba npm antes que pnpm: al actualizar una copia
/// instalada con npm hay que usar npm, o quedarian dos copias distintas.
pub fn ejecutar_instalacion(npm_primero: bool, listo: impl Fn() -> bool) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    let path = path_ampliado();

    // pnpm y npm son scripts de Node, asi que necesitan encontrar `node` en
    // el PATH: se lo pasamos ampliado. Por defecto preferimos pnpm (el gestor
    // que DSH documenta) y dejamos npm como respaldo.
    let pnpm = [
        format!("{home}/.local/share/pnpm/bin/pnpm add -g @deepseek-ai/dsh"),
        format!("{home}/.local/share/pnpm/bin/pnpm add -g @deepseek-ai/dsh --force"),
    ];
    let npm = "npm install -g @deepseek-ai/dsh".to_string();
    let ordenes: Vec<String> = if npm_primero {
        std::iter::once(npm).chain(pnpm).collect()
    } else {
        pnpm.into_iter().chain(std::iter::once(npm)).collect()
    };

    for orden in ordenes {
        let salida = Command::new("sh")
            .arg("-c")
            .arg(&orden)
            .env("PATH", &path)
            .stdin(Stdio::null())
            .output();

        if salida.map(|o| o.status.success()).unwrap_or(false) && listo() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_de_node() {
        assert!(node_suficiente("v24.1.0"));
        assert!(node_suficiente("v22.0.0"));
        assert!(node_suficiente("22.11.0\n"));
        assert!(!node_suficiente("v18.19.1"));
        assert!(!node_suficiente("basura"));
        assert!(!node_suficiente(""));
    }

    #[test]
    fn shim_de_pnpm() {
        let largo = "499c6f07a790dd7f679cefbbb26ef27a4ada938b7769be3aeb15c04209a68b61";
        let shim = format!(
            "#!/bin/sh\nexec node \"/home/u/.local/share/pnpm/global/v11/1696dd-18dc9d51a32a5016-0/node_modules/x\" \
             \"/home/u/.local/share/pnpm/global/v11/{largo}/node_modules/@deepseek-ai/dsh/lib/bin.js\" \"$@\"\n"
        );
        assert_eq!(dir_global_del_shim(&shim), Some(format!("v11/{largo}")));
    }

    #[test]
    fn shim_real_con_rutas_relativas() {
        // Formato real del shim de pnpm 11 (cmd-shim): rutas con $basedir/..
        let h = "499c6f07a790dd7f679cefbbb26ef27a4ada938b7769be3aeb15c04209a68b61";
        let shim = format!(
            "#!/bin/sh\n# cmd-shim-target=/home/u/.local/share/pnpm/global/v11/{h}/node_modules/@deepseek-ai/dsh/lib/bin.js\n\
             exec \"$basedir/node\" \"$basedir/../global/v11/{h}/node_modules/@deepseek-ai/dsh/lib/bin.js\" \"$@\"\n"
        );
        assert_eq!(dir_global_del_shim(&shim), Some(format!("v11/{h}")));
    }

    #[test]
    fn shim_de_otro_layout_de_pnpm() {
        let shim = "exec node \"/h/.local/share/pnpm/global/v10/abcdef0123456789abcd/node_modules/@deepseek-ai/dsh/lib/bin.js\"";
        assert_eq!(dir_global_del_shim(shim), Some("v10/abcdef0123456789abcd".into()));
        assert_eq!(dir_global_del_shim("#!/bin/sh\nexec dsh"), None);
    }

    #[test]
    fn version_desde_la_ruta_de_npm() {
        let raiz = std::env::temp_dir().join(format!("dsh-test-{}", std::process::id()));
        let paquete = raiz.join("lib/node_modules/@deepseek-ai/dsh");
        std::fs::create_dir_all(paquete.join("lib")).unwrap();
        std::fs::write(
            paquete.join("package.json"),
            r#"{"name":"@deepseek-ai/dsh","version":"0.2.2-rc.1"}"#,
        )
        .unwrap();
        // Un package.json ajeno mas arriba no debe confundir la busqueda.
        std::fs::write(raiz.join("package.json"), r#"{"name":"otro","version":"9.9.9"}"#).unwrap();

        let bin = paquete.join("lib/bin.js");
        assert_eq!(version_del_paquete(&bin), Some("0.2.2-rc.1".into()));
        assert_eq!(version_del_paquete(&raiz.join("nada/bin.js")), None);
        let _ = std::fs::remove_dir_all(&raiz);
    }
}
