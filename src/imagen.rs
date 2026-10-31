//! Lectura, redimensionado y codificación de una imagen.

use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;

use image::codecs::avif::AvifEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageReader};

use crate::error::ErrorArchivo;
use crate::formato::Formato;

/// Lado máximo, en píxeles, de una imagen que se acepta leer. Coincide con el
/// máximo de WebP y frena las «bombas de descompresión»: archivos pequeños
/// que declaran dimensiones enormes para agotar la memoria.
pub const LADO_MAXIMO: u32 = 16_383;

/// Memoria máxima que puede reservar el decodificador para una imagen.
pub const MEMORIA_MAXIMA: u64 = 512 * 1024 * 1024;

/// Tamaño máximo del archivo original: 100 MB.
pub const ARCHIVO_MAXIMO: u64 = 100_000_000;

/// Velocidad del codificador AVIF, de 1 (lento, algo más pequeño) a 10.
const VELOCIDAD_AVIF: u8 = 6;

/// Píxeles listos para los codificadores: RGB o, si hay transparencia de
/// verdad, RGBA, siempre con 8 bits por canal.
#[derive(Debug)]
pub struct Pixeles {
    pub datos: Vec<u8>,
    pub ancho: u32,
    pub alto: u32,
    pub alfa: bool,
}

impl Pixeles {
    /// Prepara la imagen para codificarla. El canal alfa solo se conserva si
    /// algún píxel es transparente: muchos PNG lo traen sin usarlo.
    pub fn desde(imagen: DynamicImage) -> Pixeles {
        let (ancho, alto) = (imagen.width(), imagen.height());
        let (datos, alfa) = a_ocho_bits(imagen);
        Pixeles {
            datos,
            ancho,
            alto,
            alfa,
        }
    }
}

fn a_ocho_bits(imagen: DynamicImage) -> (Vec<u8>, bool) {
    if !imagen.color().has_alpha() {
        return (imagen.into_rgb8().into_raw(), false);
    }
    let rgba = imagen.into_rgba8();
    if rgba.pixels().any(|p| p.0[3] < u8::MAX) {
        return (rgba.into_raw(), true);
    }
    let rgb = DynamicImage::ImageRgba8(rgba).into_rgb8();
    (rgb.into_raw(), false)
}

/// Lee y decodifica una imagen con límites de seguridad y aplica la
/// orientación EXIF, para que las fotos del móvil no salgan giradas.
pub fn leer(ruta: &Path) -> Result<DynamicImage, ErrorArchivo> {
    let datos = leer_limitado(ruta)?;
    let mut lector = ImageReader::new(Cursor::new(datos));
    lector.limits(limites());
    // La extensión sirve de pista; si el contenido se reconoce, manda él.
    if let Ok(formato) = image::ImageFormat::from_path(ruta) {
        lector.set_format(formato);
    }
    let lector = lector.with_guessed_format();
    let lector = lector.map_err(ErrorArchivo::Lectura)?;
    let mut decodificador = lector.into_decoder()?;
    let orientacion = decodificador.orientation()?;
    let mut imagen = DynamicImage::from_decoder(decodificador)?;
    imagen.apply_orientation(orientacion);
    Ok(imagen)
}

/// Límites del decodificador: se comprueban con la cabecera, antes de
/// reservar memoria para los píxeles.
fn limites() -> image::Limits {
    let mut limites = image::Limits::no_limits();
    limites.max_image_width = Some(LADO_MAXIMO);
    limites.max_image_height = Some(LADO_MAXIMO);
    limites.max_alloc = Some(MEMORIA_MAXIMA);
    limites
}

/// Lee el archivo entero sin pasar de `ARCHIVO_MAXIMO`, aunque crezca
/// mientras se lee.
fn leer_limitado(ruta: &Path) -> Result<Vec<u8>, ErrorArchivo> {
    let archivo = File::open(ruta).map_err(ErrorArchivo::Lectura)?;
    let mut limitado = archivo.take(ARCHIVO_MAXIMO + 1);
    let mut datos = Vec::new();
    let leido = limitado.read_to_end(&mut datos);
    leido.map_err(ErrorArchivo::Lectura)?;
    if datos.len() as u64 > ARCHIVO_MAXIMO {
        return Err(ErrorArchivo::DemasiadoGrande);
    }
    Ok(datos)
}

/// Medidas finales: si la imagen es más ancha que `ancho_max` se reduce
/// manteniendo la proporción; nunca se amplía.
pub fn dimensiones_destino(ancho: u32, alto: u32, ancho_max: Option<u32>) -> (u32, u32) {
    match ancho_max {
        Some(maximo) if ancho > maximo => {
            let (ancho, alto, maximo) = (u64::from(ancho), u64::from(alto), u64::from(maximo));
            // Redondeo al entero más cercano, sin bajar nunca de 1 px.
            let alto_nuevo = (alto * maximo + ancho / 2) / ancho;
            (maximo as u32, alto_nuevo.max(1) as u32)
        }
        _ => (ancho, alto),
    }
}

/// Reduce la imagen al ancho máximo con un filtro Lanczos (el más nítido de
/// los que ofrece `image`), o la devuelve tal cual si ya cabe.
pub fn redimensionar(imagen: DynamicImage, ancho_max: Option<u32>) -> DynamicImage {
    let (ancho, alto) = dimensiones_destino(imagen.width(), imagen.height(), ancho_max);
    if ancho == imagen.width() {
        return imagen;
    }
    imagen.resize_exact(ancho, alto, FilterType::Lanczos3)
}

/// Codifica los píxeles en el formato pedido con una calidad de 1 a 100.
pub fn codificar(p: &Pixeles, formato: Formato, calidad: u8) -> Result<Vec<u8>, ErrorArchivo> {
    match formato {
        Formato::Webp => a_webp(p, calidad),
        Formato::Avif => a_avif(p, calidad),
    }
}

fn a_webp(p: &Pixeles, calidad: u8) -> Result<Vec<u8>, ErrorArchivo> {
    let codificador = if p.alfa {
        webp::Encoder::from_rgba(&p.datos, p.ancho, p.alto)
    } else {
        webp::Encoder::from_rgb(&p.datos, p.ancho, p.alto)
    };
    match codificador.encode_simple(false, f32::from(calidad)) {
        Ok(memoria) => Ok(memoria.to_vec()),
        Err(error) => {
            let detalle = format!("{error:?}");
            Err(ErrorArchivo::Codificacion(Formato::Webp, detalle))
        }
    }
}

fn a_avif(p: &Pixeles, calidad: u8) -> Result<Vec<u8>, ErrorArchivo> {
    let color = if p.alfa {
        ExtendedColorType::Rgba8
    } else {
        ExtendedColorType::Rgb8
    };
    let mut salida = Vec::new();
    let codificador = AvifEncoder::new_with_speed_quality(&mut salida, VELOCIDAD_AVIF, calidad);
    let escrito = codificador.write_image(&p.datos, p.ancho, p.alto, color);
    match escrito {
        Ok(()) => Ok(salida),
        Err(error) => {
            let detalle = error.to_string();
            Err(ErrorArchivo::Codificacion(Formato::Avif, detalle))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use image::{Rgb, RgbImage, Rgba, RgbaImage};
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn reduce_manteniendo_la_proporcion() {
        assert_eq!(dimensiones_destino(4000, 3000, Some(1600)), (1600, 1200));
        assert_eq!(dimensiones_destino(1000, 667, Some(500)), (500, 334));
        assert_eq!(dimensiones_destino(1601, 1, Some(1600)), (1600, 1));
    }

    #[test]
    fn nunca_amplia() {
        assert_eq!(dimensiones_destino(800, 600, Some(1600)), (800, 600));
        assert_eq!(dimensiones_destino(1600, 900, Some(1600)), (1600, 900));
        assert_eq!(dimensiones_destino(5000, 10, None), (5000, 10));
    }

    #[test]
    fn descarta_el_alfa_que_no_se_usa() {
        let opaca = RgbaImage::from_pixel(4, 3, Rgba([10, 20, 30, 255]));
        let pixeles = Pixeles::desde(DynamicImage::ImageRgba8(opaca));
        assert!(!pixeles.alfa);
        assert_eq!(pixeles.datos.len(), 4 * 3 * 3);

        let mut transparente = RgbaImage::from_pixel(4, 3, Rgba([10, 20, 30, 255]));
        transparente.put_pixel(0, 0, Rgba([0, 0, 0, 0]));
        let pixeles = Pixeles::desde(DynamicImage::ImageRgba8(transparente));
        assert!(pixeles.alfa);
        assert_eq!(pixeles.datos.len(), 4 * 3 * 4);
    }

    #[test]
    fn rechaza_dimensiones_por_encima_del_limite() {
        let dir = TempDir::new().unwrap();
        let ruta = dir.path().join("bomba.png");
        let enorme = RgbImage::new(LADO_MAXIMO + 1, 1);
        enorme.save(&ruta).unwrap();
        let resultado = leer(&ruta);
        assert!(matches!(resultado, Err(ErrorArchivo::Limites)));
    }

    #[test]
    fn un_archivo_danado_es_un_error_no_un_panico() {
        let dir = TempDir::new().unwrap();
        let ruta = dir.path().join("rota.jpg");
        fs::write(&ruta, b"esto no es un JPEG").unwrap();
        let resultado = leer(&ruta);
        assert!(matches!(resultado, Err(ErrorArchivo::Imagen(_))));
    }

    #[test]
    fn codifica_webp_y_avif_validos() {
        let imagen = RgbImage::from_pixel(32, 24, Rgb([240, 165, 143]));
        let pixeles = Pixeles::desde(DynamicImage::ImageRgb8(imagen));
        let webp = codificar(&pixeles, Formato::Webp, 80).unwrap();
        assert!(webp.starts_with(b"RIFF"));
        assert_eq!(webp.get(8..12), Some(b"WEBP".as_slice()));
        let avif = codificar(&pixeles, Formato::Avif, 80).unwrap();
        assert_eq!(avif.get(4..12), Some(b"ftypavif".as_slice()));
    }
}
