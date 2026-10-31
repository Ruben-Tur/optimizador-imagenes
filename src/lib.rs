//! `afinar`: convierte carpetas enteras de imágenes a WebP y AVIF en paralelo.
//!
//! La lógica vive en esta biblioteca para poder probarla; `main.rs` solo lee
//! la línea de comandos y elige cómo contar el resultado.

pub mod cli;
pub mod error;
pub mod formato;
pub mod imagen;
pub mod informe;
pub mod lote;
pub mod presentacion;
pub mod rutas;
