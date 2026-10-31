//! Errores al procesar un archivo concreto. Ninguno detiene el lote: se
//! anotan en el informe y el resto de imágenes sigue su curso.

use std::io;

use image::ImageError;
use thiserror::Error;

use crate::formato::Formato;

/// Por qué no se ha podido convertir una imagen.
#[derive(Debug, Error)]
pub enum ErrorArchivo {
    /// El sistema no deja leer el archivo (permisos, se ha borrado...).
    #[error("no se pudo leer el archivo: {0}")]
    Lectura(#[source] io::Error),

    /// El archivo supera `imagen::ARCHIVO_MAXIMO`.
    #[error("ocupa más de 100 MB, el máximo admitido")]
    DemasiadoGrande,

    /// Dimensiones o memoria por encima de los límites de `imagen`.
    #[error("supera los límites de seguridad (16 383 px de lado o 512 MiB)")]
    Limites,

    /// No se reconoce como imagen o está dañada.
    #[error("no es una imagen válida o está dañada")]
    Imagen(#[source] ImageError),

    /// El codificador ha rechazado la imagen.
    #[error("no se pudo convertir a {0}: {1}")]
    Codificacion(Formato, String),

    /// No se ha podido guardar el resultado.
    #[error("no se pudo guardar el resultado: {0}")]
    Escritura(#[source] io::Error),

    /// La carpeta de destino, una vez resuelta, cae fuera de la de salida.
    #[error("el destino queda fuera de la carpeta de salida")]
    FueraDeSalida,

    /// La ruta relativa tiene tramos que podrían sacarla de su carpeta.
    #[error("ruta no válida dentro de la carpeta de entrada")]
    RutaNoValida,

    /// Otro original ya genera un resultado con el mismo nombre.
    #[error("su resultado se llamaría igual que el de «{0}»")]
    Conflicto(String),

    /// Una biblioteca ha entrado en pánico con este archivo.
    #[error("error interno inesperado con este archivo")]
    Interno,
}

impl From<ImageError> for ErrorArchivo {
    fn from(error: ImageError) -> ErrorArchivo {
        match error {
            ImageError::Limits(_) => ErrorArchivo::Limites,
            otro => ErrorArchivo::Imagen(otro),
        }
    }
}

impl ErrorArchivo {
    /// Explicación técnica de la biblioteca (en inglés), útil para depurar.
    pub fn detalle(&self) -> Option<String> {
        match self {
            ErrorArchivo::Imagen(error) => Some(error.to_string()),
            _ => None,
        }
    }
}

/// Un fallo ya convertido en texto para el informe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fallo {
    /// Mensaje en español para la persona que usa la herramienta.
    pub mensaje: String,
    /// Detalle técnico opcional (solo sale en el JSON).
    pub detalle: Option<String>,
}

impl From<&ErrorArchivo> for Fallo {
    fn from(error: &ErrorArchivo) -> Fallo {
        Fallo {
            mensaje: error.to_string(),
            detalle: error.detalle(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_mensajes_estan_en_espanol() {
        let error = ErrorArchivo::Conflicto("foto.jpg".to_string());
        let esperado = "su resultado se llamaría igual que el de «foto.jpg»";
        assert_eq!(error.to_string(), esperado);
    }

    #[test]
    fn el_fallo_guarda_el_detalle_tecnico_aparte() {
        let fallo = Fallo::from(&ErrorArchivo::Interno);
        assert_eq!(fallo.detalle, None);
        assert!(fallo.mensaje.starts_with("error interno"));
    }
}
