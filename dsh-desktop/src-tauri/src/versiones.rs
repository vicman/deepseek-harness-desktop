//! Comparacion de versiones segun semver.

/// Compara versiones semver (`1.2.3`, `0.2.2-rc.1`, `1.0.0+build`).
///
/// Sigue la precedencia de semver: primero `mayor.menor.parche`; a igualdad,
/// una version sin prerelease es mas nueva que cualquier prerelease
/// (`0.2.2` > `0.2.2-rc.4`), y entre prereleases se comparan los
/// identificadores uno a uno (`rc.4` > `rc.1`, numericos < alfanumericos).
pub fn comparar_versiones(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    fn partes(v: &str) -> (Vec<u64>, Option<&str>) {
        let v = v.trim().trim_start_matches('v');
        let v = v.split('+').next().unwrap_or(""); // los metadatos no cuentan
        let (nucleo, pre) = match v.split_once('-') {
            Some((n, p)) => (n, Some(p)),
            None => (v, None),
        };
        let mut nums: Vec<u64> = nucleo.split('.').map(|p| p.parse().unwrap_or(0)).collect();
        nums.resize(3, 0);
        (nums, pre)
    }

    let (na, pa) = partes(a);
    let (nb, pb) = partes(b);
    match na.cmp(&nb) {
        Ordering::Equal => {}
        otro => return otro,
    }
    match (pa, pb) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(pa), Some(pb)) => {
            let (mut ia, mut ib) = (pa.split('.'), pb.split('.'));
            loop {
                match (ia.next(), ib.next()) {
                    (None, None) => return Ordering::Equal,
                    (None, Some(_)) => return Ordering::Less,
                    (Some(_), None) => return Ordering::Greater,
                    (Some(x), Some(y)) => {
                        let orden = match (x.parse::<u64>(), y.parse::<u64>()) {
                            (Ok(x), Ok(y)) => x.cmp(&y),
                            (Ok(_), Err(_)) => Ordering::Less,
                            (Err(_), Ok(_)) => Ordering::Greater,
                            (Err(_), Err(_)) => x.cmp(y),
                        };
                        if orden != Ordering::Equal {
                            return orden;
                        }
                    }
                }
            }
        }
    }
}

pub fn es_mas_nueva(candidata: &str, actual: &str) -> bool {
    comparar_versiones(candidata, actual) == std::cmp::Ordering::Greater
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering::*;

    #[test]
    fn versiones_numericas() {
        assert_eq!(comparar_versiones("0.2.2", "0.2.0"), Greater);
        assert_eq!(comparar_versiones("0.10.0", "0.9.9"), Greater);
        assert_eq!(comparar_versiones("1.0.0", "1.0.0"), Equal);
        assert_eq!(comparar_versiones("v1.2.3", "1.2.3"), Equal);
        assert_eq!(comparar_versiones("1.2", "1.2.0"), Equal);
    }

    #[test]
    fn la_estable_gana_a_su_prerelease() {
        // El fallo de la 0.1.1: 0.2.2 y 0.2.2-rc.1 se veian iguales.
        assert!(es_mas_nueva("0.2.2", "0.2.2-rc.1"));
        assert!(!es_mas_nueva("0.2.2-rc.4", "0.2.2"));
    }

    #[test]
    fn prereleases_entre_si() {
        assert!(es_mas_nueva("0.2.2-rc.4", "0.2.2-rc.1"));
        assert!(es_mas_nueva("0.2.2-rc.10", "0.2.2-rc.9")); // numerico, no texto
        assert!(es_mas_nueva("0.2.2-rc.1", "0.2.2-alpha.3")); // alfabetico
        assert!(es_mas_nueva("0.2.2-rc.1", "0.2.0-rc.2"));
        assert!(es_mas_nueva("1.0.0-alpha.1", "1.0.0-alpha"));
        assert!(es_mas_nueva("1.0.0-alpha", "1.0.0-1")); // numerico < alfanumerico
    }

    #[test]
    fn metadatos_de_build_no_cuentan() {
        assert_eq!(comparar_versiones("1.0.0+abc", "1.0.0+xyz"), Equal);
        assert!(!es_mas_nueva("0.2.2-rc.1", "0.2.2-rc.1"));
    }
}
