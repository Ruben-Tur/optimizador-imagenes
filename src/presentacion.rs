//! Cómo se muestran las cifras y los colores en la terminal.
//!
//! Las cifras usan coma decimal y un espacio antes de la unidad, como se
//! escriben en español: «3,80 MB», «-89 %», «11,3 s».

use std::time::Duration;

use anstyle::{Color, RgbColor, Style};

/// Color de acento de la marca: salmón, #f0a58f.
pub const ACENTO: Style = rgb(0xf0, 0xa5, 0x8f);
/// Ahorros conseguidos.
pub const VERDE: Style = rgb(0x8f, 0xd1, 0x9e);
/// Errores.
pub const ROJO: Style = rgb(0xf2, 0x6d, 0x6d);
/// Texto secundario.
pub const TENUE: Style = Style::new().dimmed();
/// Énfasis.
pub const NEGRITA: Style = Style::new().bold();

const UNIDADES: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

const fn rgb(r: u8, g: u8, b: u8) -> Style {
    Style::new().fg_color(Some(Color::Rgb(RgbColor(r, g, b))))
}

/// Envuelve un texto en un estilo. `anstream` quita los códigos de color si
/// la salida no es una terminal o si está definida la variable `NO_COLOR`.
pub fn pinta(estilo: Style, texto: &str) -> String {
    format!("{estilo}{texto}{estilo:#}")
}

/// Tamaño legible con tres cifras significativas (unidades de 1000 bytes):
/// «512 B», «412 KB», «3,80 MB», «1,02 GB».
pub fn bytes(cantidad: u64) -> String {
    if cantidad < 1000 {
        return format!("{cantidad} B");
    }
    let mut valor = cantidad as f64;
    let mut indice = 0;
    // Se cambia de unidad antes de llegar a 1000 para no mostrar «1000 KB».
    while valor >= 999.5 && indice < UNIDADES.len() - 1 {
        valor /= 1000.0;
        indice += 1;
    }
    let cifra = if valor < 9.995 {
        format!("{valor:.2}")
    } else if valor < 99.95 {
        format!("{valor:.1}")
    } else {
        format!("{valor:.0}")
    };
    let cifra = cifra.replace('.', ",");
    let unidad = UNIDADES[indice];
    format!("{cifra} {unidad}")
}

/// Variación de tamaño en tanto por ciento: negativa si se ha ahorrado.
pub fn porcentaje(original: u64, nuevo: u64) -> f64 {
    if original == 0 {
        return 0.0;
    }
    (nuevo as f64 - original as f64) * 100.0 / original as f64
}

/// La variación redondeada para leerla: «-89 %», «+12 %», «0 %».
pub fn variacion(original: u64, nuevo: u64) -> String {
    let redondeada = porcentaje(original, nuevo).round() as i64;
    if redondeada > 0 {
        format!("+{redondeada} %")
    } else {
        format!("{redondeada} %")
    }
}

/// Tiempo transcurrido: «11,3 s» o, a partir del minuto, «2 min 05 s».
pub fn duracion(tiempo: Duration) -> String {
    if tiempo.as_secs() < 60 {
        let segundos = tiempo.as_secs_f64();
        let cifra = format!("{segundos:.1}").replace('.', ",");
        return format!("{cifra} s");
    }
    let total = tiempo.as_secs();
    let minutos = total / 60;
    let resto = total % 60;
    format!("{minutos} min {resto:02} s")
}

/// Ajusta un texto a `ancho` columnas: rellena con espacios o, si no cabe,
/// lo recorta por la izquierda con «…» para que se siga viendo el archivo.
pub fn ajustar(texto: &str, ancho: usize) -> String {
    let largo = texto.chars().count();
    if largo <= ancho {
        let relleno = " ".repeat(ancho - largo);
        return format!("{texto}{relleno}");
    }
    let sobran = largo + 1 - ancho;
    let cola: String = texto.chars().skip(sobran).collect();
    format!("…{cola}")
}

/// Sustituye los caracteres de control (por ejemplo, una secuencia de escape
/// escondida en el nombre de un archivo) para que no manipulen la terminal.
pub fn seguro(texto: &str) -> String {
    let mut limpio = String::with_capacity(texto.len());
    for c in texto.chars() {
        let peligroso = c.is_control() || es_bidi(c);
        limpio.push(if peligroso { '?' } else { c });
    }
    limpio
}

/// Marcas de dirección del texto (U+202A–U+202E y U+2066–U+2069). No son de
/// control, pero dan la vuelta a lo que sigue y pueden disfrazar un nombre:
/// `foto` + U+202E + `gpj.exe` se vería como `fotoexe.jpg`.
fn es_bidi(c: char) -> bool {
    matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// «1 imagen», «3 imágenes».
pub fn plural(cantidad: usize, uno: &str, varios: &str) -> String {
    if cantidad == 1 {
        format!("1 {uno}")
    } else {
        format!("{cantidad} {varios}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_con_tres_cifras_y_coma_decimal() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(999), "999 B");
        assert_eq!(bytes(1_000), "1,00 KB");
        assert_eq!(bytes(412_000), "412 KB");
        assert_eq!(bytes(3_800_000), "3,80 MB");
        assert_eq!(bytes(96_400_000), "96,4 MB");
        assert_eq!(bytes(1_020_000_000), "1,02 GB");
    }

    #[test]
    fn bytes_nunca_muestra_mil_de_una_unidad() {
        assert_eq!(bytes(999_499), "999 KB");
        assert_eq!(bytes(999_999), "1,00 MB");
    }

    #[test]
    fn variacion_con_signo_y_redondeo() {
        assert_eq!(variacion(1_000, 110), "-89 %");
        assert_eq!(variacion(1_000, 1_120), "+12 %");
        assert_eq!(variacion(1_000, 1_000), "0 %");
        assert_eq!(variacion(1_000, 996), "0 %");
        assert_eq!(variacion(0, 5), "0 %");
    }

    #[test]
    fn duracion_en_segundos_y_minutos() {
        assert_eq!(duracion(Duration::from_millis(11_300)), "11,3 s");
        assert_eq!(duracion(Duration::from_millis(40)), "0,0 s");
        assert_eq!(duracion(Duration::from_secs(125)), "2 min 05 s");
    }

    #[test]
    fn ajustar_rellena_o_recorta_por_la_izquierda() {
        assert_eq!(ajustar("foto.jpg", 10), "foto.jpg  ");
        let recortado = ajustar("carpeta/larga/foto.jpg", 10);
        assert_eq!(recortado, "…/foto.jpg");
        assert_eq!(recortado.chars().count(), 10);
    }

    #[test]
    fn seguro_neutraliza_secuencias_de_escape() {
        assert_eq!(seguro("foto\u{1b}[31m.jpg"), "foto?[31m.jpg");
        assert_eq!(seguro("año-2025.png"), "año-2025.png");
        assert_eq!(seguro("foto\u{202E}gpj.exe"), "foto?gpj.exe");
        assert_eq!(seguro("a\u{2066}b\u{2069}"), "a?b?");
    }

    #[test]
    fn plural_en_espanol() {
        assert_eq!(plural(1, "imagen", "imágenes"), "1 imagen");
        assert_eq!(plural(3, "imagen", "imágenes"), "3 imágenes");
    }
}
