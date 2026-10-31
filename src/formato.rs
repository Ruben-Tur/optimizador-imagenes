//! Formatos de salida.

use std::fmt;

use clap::ValueEnum;

/// Formatos a los que se puede convertir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Formato {
    /// WebP con pérdida (libwebp)
    Webp,
    /// AVIF (rav1e)
    Avif,
}

impl Formato {
    /// Extensión del archivo resultante, que también es su nombre corto.
    pub fn extension(self) -> &'static str {
        match self {
            Formato::Webp => "webp",
            Formato::Avif => "avif",
        }
    }
}

impl fmt::Display for Formato {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.extension())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombre_y_extension_coinciden() {
        assert_eq!(Formato::Webp.to_string(), "webp");
        assert_eq!(Formato::Avif.extension(), "avif");
    }

    #[test]
    fn el_orden_pone_webp_primero() {
        assert!(Formato::Webp < Formato::Avif);
    }
}
