//! Hex ↔ bytes. Una sola copia, y hay motivo.
//!
//! `firma.rs` y `licencia.rs` tenían cada uno la suya, idénticas, y las dos
//! traían el mismo defecto: medir la longitud en BYTES y después cortar por
//! índices de `&str`.
//!
//! ```ignore
//! (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
//! ```
//!
//! Con un carácter multibyte en la posición justa, el corte cae DENTRO del
//! carácter y el proceso entra en pánico. No es teórico: `de_hex("€a")` revienta
//! con *«end byte index 2 is not a char boundary»*, y ese valor llega de una
//! variable de entorno que alguien teclea o pega —una tilde de más al copiar una
//! clave de un correo, un guion tipográfico—. En el camino de la licencia se
//! ejecuta en CADA consulta, así que convertía el «no se puede verificar» que el
//! módulo promete en una petición muerta.
//!
//! Lo cazó una revisión de seguridad. Se arregla una vez y en un sitio: dos
//! copias de una función que toca material criptográfico es exactamente cómo se
//! arregla una y se olvida la otra.

/// Hex → bytes. `None` si no es hex par y legible. Itera sobre BYTES, así que
/// ninguna entrada, venga como venga, puede hacerla entrar en pánico.
pub fn de_hex(s: &str) -> Option<Vec<u8>> {
    let b = s.trim().as_bytes();
    if b.is_empty() || b.len() % 2 != 0 {
        return None;
    }
    b.chunks(2)
        .map(|par| u8::from_str_radix(std::str::from_utf8(par).ok()?, 16).ok())
        .collect()
}

/// Bytes → hex en minúsculas.
pub fn a_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ida_y_vuelta() {
        let bytes: Vec<u8> = (0u8..=255).collect();
        assert_eq!(de_hex(&a_hex(&bytes)).unwrap(), bytes);
        assert_eq!(de_hex(" 00ff ").unwrap(), vec![0x00, 0xff]);
        // Mayúsculas también: una clave copiada de otra herramienta puede venir así.
        assert_eq!(de_hex("00FF").unwrap(), vec![0x00, 0xff]);
    }

    /// El defecto que originó este módulo: ninguna cadena, por rara que sea,
    /// puede hacer que esto entre en pánico. Si esta prueba se cae, se cae con
    /// un pánico y no con un `assert`, que es justo la señal.
    ///
    /// **Los casos multibyte llevan longitud PAR en bytes a propósito.** Con
    /// longitud impar mueren en el `len() % 2` y no llegan a cortarse nunca, así
    /// que probarían la guarda de paridad y no el corte — que es lo que
    /// reventaba. Lo señaló una re-revisión: la primera versión de esta lista
    /// tenía tres casos que se paraban antes de tiempo.
    #[test]
    fn ninguna_entrada_entra_en_panico() {
        // Los que mueren pronto, en la guarda de paridad o por vacíos.
        for s in ["", " ", "z", "abc", "0", "ñ", "—"] {
            assert!(de_hex(s).is_none(), "{s:?} no debería decodificar");
        }

        // Los que LLEGAN al troceado, que es donde estaba el pánico. Se
        // comprueba que la longitud es par antes de usarlos: si un día alguien
        // edita la lista y mete uno impar, este aserto lo dice en vez de dejar
        // el caso sin ejercitar.
        for s in [
            "zz",             // 2 bytes, no-hex
            "€a",             // 4 bytes: el corte cae dentro del €
            "añb",            // 4 bytes: multibyte en medio
            "00€a",           // 6 bytes
            "\u{1F600}",      // 4 bytes: un carácter, dos trozos partidos
            "——",             // 6 bytes
            "ññ",             // 4 bytes
        ] {
            assert_eq!(s.len() % 2, 0, "{s:?} tiene que ser par para probar el troceado");
            assert!(de_hex(s).is_none(), "{s:?} no es hex y no debería decodificar");
        }
    }

    /// Y la mitad que evita que lo anterior pase por la razón equivocada: que
    /// `de_hex` no devuelva `None` para TODO. Con eso la prueba de arriba sería
    /// verde y la función, inútil.
    #[test]
    fn el_hex_de_verdad_sigue_decodificando() {
        assert_eq!(de_hex("00").unwrap(), vec![0x00]);
        assert_eq!(de_hex("deadbeef").unwrap(), vec![0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(de_hex(&a_hex(&[1, 2, 3])).unwrap(), vec![1, 2, 3]);
    }
}
