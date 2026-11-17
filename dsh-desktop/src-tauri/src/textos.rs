//! Idioma y textos de la aplicacion.
//!
//! Todos los textos viven en `ui/textos.json`, compartido con la pantalla de
//! carga: Rust lo incrusta en el binario y lo inyecta en el HTML del splash,
//! asi cada texto se traduce una sola vez.

use std::sync::OnceLock;

use serde_json::Value;

/// Contenido de `ui/textos.json`, incrustado al compilar.
pub const TEXTOS_JSON: &str = include_str!("../../ui/textos.json");

/// Comando de instalacion de DSH; sustituye el marcador `{orden}`.
pub const ORDEN_INSTALAR: &str = "pnpm add -g @deepseek-ai/dsh";

fn textos() -> &'static Value {
    static TEXTOS: OnceLock<Value> = OnceLock::new();
    TEXTOS.get_or_init(|| serde_json::from_str(TEXTOS_JSON).unwrap_or(Value::Null))
}

/// Texto `clave` (con puntos para anidar: `notificacion.titulo`) en el idioma
/// del equipo, con los marcadores `{nombre}` sustituidos por `datos`.
/// Si falta en ese idioma se usa el espanol, y si falta tambien, la clave.
pub fn texto(clave: &str, datos: &[(&str, &str)]) -> String {
    texto_en(idioma_del_equipo(), clave, datos)
}

pub fn texto_en(idioma: &str, clave: &str, datos: &[(&str, &str)]) -> String {
    let buscar = |idioma: &str| {
        clave
            .split('.')
            .try_fold(textos().get(idioma)?, |v, parte| v.get(parte))
            .and_then(|v| v.as_str())
    };
    let mut s = buscar(idioma).or_else(|| buscar("es")).unwrap_or(clave).to_string();
    s = s.replace("{orden}", ORDEN_INSTALAR);
    for (nombre, valor) in datos {
        s = s.replace(&format!("{{{nombre}}}"), valor);
    }
    s
}

/// Idioma de la interfaz segun el equipo: `es`, `en` o `pt`.
///
/// Se leen las variables de entorno del sistema (las mismas que usa el
/// escritorio). Espanol, ingles y portugues; cualquier otro idioma cae al
/// espanol, que es el idioma por defecto de la aplicacion.
pub fn idioma_del_equipo() -> &'static str {
    // LC_ALL manda sobre LC_MESSAGES, y este sobre LANG.
    for var in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        // Cualquier otro idioma: se sigue probando la variable siguiente.
        if let Some(idioma) = idioma_de(&std::env::var(var).unwrap_or_default()) {
            return idioma;
        }
    }
    // Sin coincidencia, espanol por defecto.
    "es"
}

/// Idioma soportado de un valor tipo locale, o `None` si no es es/en/pt.
/// Formatos: "es_CO.UTF-8", "en_US", "pt_BR.utf8", "es_CO:es".
pub fn idioma_de(valor: &str) -> Option<&'static str> {
    let valor = valor.to_lowercase();
    let base = valor
        .split(['.', ':', '@'])
        .next()
        .unwrap_or("")
        .split(['_', '-'])
        .next()
        .unwrap_or("");
    match base {
        "es" => Some("es"),
        "en" => Some("en"),
        "pt" => Some("pt"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idiomas() {
        assert_eq!(idioma_de("es_CO.UTF-8"), Some("es"));
        assert_eq!(idioma_de("en_US"), Some("en"));
        assert_eq!(idioma_de("pt_BR.utf8"), Some("pt"));
        assert_eq!(idioma_de("es_CO:es"), Some("es"));
        assert_eq!(idioma_de("EN-gb"), Some("en"));
        assert_eq!(idioma_de("fr_FR.UTF-8"), None);
        assert_eq!(idioma_de("C"), None);
        assert_eq!(idioma_de(""), None);
    }

    /// Claves de un objeto JSON, aplanadas con puntos.
    fn claves(v: &Value, prefijo: &str, salida: &mut Vec<String>) {
        if let Some(obj) = v.as_object() {
            for (k, hijo) in obj {
                let ruta = if prefijo.is_empty() { k.clone() } else { format!("{prefijo}.{k}") };
                if hijo.is_object() {
                    claves(hijo, &ruta, salida);
                } else {
                    salida.push(ruta);
                }
            }
        }
    }

    #[test]
    fn los_tres_idiomas_tienen_las_mismas_claves() {
        let json: Value = serde_json::from_str(TEXTOS_JSON).expect("textos.json no es JSON valido");
        let mut base = Vec::new();
        claves(&json["es"], "", &mut base);
        base.sort();
        assert!(!base.is_empty());
        for idioma in ["en", "pt"] {
            let mut otras = Vec::new();
            claves(&json[idioma], "", &mut otras);
            otras.sort();
            assert_eq!(base, otras, "claves distintas en '{idioma}'");
        }
    }

    #[test]
    fn sustituye_marcadores() {
        let t = texto_en("es", "notificacion.cuerpo", &[("instalada", "0.2.0"), ("publicada", "0.2.2")]);
        assert_eq!(t, "Instalada: 0.2.0\nNueva: 0.2.2");
        let t = texto_en("en", "notificacion.fallo", &[]);
        assert!(t.ends_with(ORDEN_INSTALAR), "{t}");
    }

    #[test]
    fn idioma_o_clave_desconocidos() {
        assert_eq!(texto_en("fr", "notificacion.boton", &[]), "Actualizar");
        assert_eq!(texto_en("es", "no.existe", &[]), "no.existe");
    }
}
