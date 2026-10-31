//! Línea de comandos: opciones en español y su ayuda.

use std::path::PathBuf;

use clap::builder::Styles;
use clap::{ArgAction, Parser};

use crate::formato::Formato;
use crate::imagen::LADO_MAXIMO;
use crate::lote::{self, Opciones};
use crate::presentacion::{ACENTO, NEGRITA, TENUE};

/// Texto de `--version`, con la firma del autor.
const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "\nHecho por Rubén Tur · https://github.com/Ruben-Tur"
);

const EJEMPLOS: &str = "  afinar ./fotos
  afinar ./fotos --formato webp --ancho 1600 --calidad 75
  afinar ./fotos --salida ./web/img --simular
  afinar ./fotos --json > informe.json";

const CODIGOS: &str = "  0  todo convertido o al día
  1  error general (por ejemplo, la carpeta de entrada no existe)
  2  opciones no válidas
  3  alguna imagen no se pudo convertir; el resto sí";

/// Convierte carpetas enteras de imágenes a WebP y AVIF en paralelo.
///
/// Los originales nunca se modifican: los resultados se escriben en otra
/// carpeta que replica la estructura de la de entrada.
#[derive(Debug, Parser)]
#[command(name = "afinar", version = VERSION, styles = estilos())]
#[command(help_template = plantilla(), after_help = ejemplos())]
#[command(override_usage = "afinar <ENTRADA> [OPCIONES]")]
#[command(disable_help_flag = true, disable_version_flag = true)]
pub struct Cli {
    /// Carpeta con las imágenes originales (JPG, PNG o WebP)
    #[arg(value_name = "ENTRADA", help_heading = "Argumentos")]
    pub entrada: PathBuf,

    /// Carpeta de resultados [por defecto: optimizadas]
    #[arg(short, long, value_name = "CARPETA", default_value = "optimizadas")]
    #[arg(hide_default_value = true, help_heading = "Opciones")]
    pub salida: PathBuf,

    /// Formatos separados por comas: webp, avif [por defecto: webp,avif]
    #[arg(short, long, value_name = "LISTA", value_delimiter = ',')]
    #[arg(value_enum, default_values_t = [Formato::Webp, Formato::Avif])]
    #[arg(hide_default_value = true, hide_possible_values = true)]
    #[arg(help_heading = "Opciones")]
    pub formato: Vec<Formato>,

    /// Ancho máximo en píxeles, sin ampliar nunca [por defecto: el original]
    #[arg(short, long, value_name = "PX")]
    #[arg(value_parser = clap::value_parser!(u32).range(1..=i64::from(LADO_MAXIMO)))]
    #[arg(help_heading = "Opciones")]
    pub ancho: Option<u32>,

    /// Calidad de 1 a 100: más alta, más nitidez y más peso [por defecto: 80]
    #[arg(short, long, value_name = "1-100", default_value_t = 80)]
    #[arg(value_parser = clap::value_parser!(u8).range(1..=100))]
    #[arg(hide_default_value = true, help_heading = "Opciones")]
    pub calidad: u8,

    /// Hilos de trabajo [por defecto: uno por núcleo]
    #[arg(long, value_name = "N")]
    #[arg(value_parser = clap::value_parser!(u16).range(1..=1024))]
    #[arg(help_heading = "Opciones")]
    pub hilos: Option<u16>,

    /// Vuelve a convertir aunque el resultado ya esté al día
    #[arg(long, help_heading = "Opciones")]
    pub forzar: bool,

    /// Muestra qué se haría, sin escribir nada
    #[arg(long, help_heading = "Opciones")]
    pub simular: bool,

    /// Imprime un resumen en JSON para scripts, en vez del informe
    #[arg(long, help_heading = "Opciones")]
    pub json: bool,

    /// Muestra esta ayuda
    #[arg(short = 'h', long = "help", action = ArgAction::Help)]
    #[arg(value_parser = clap::value_parser!(bool), help_heading = "Opciones")]
    pub ayuda: (),

    /// Muestra la versión
    #[arg(short = 'V', long = "version", action = ArgAction::Version)]
    #[arg(value_parser = clap::value_parser!(bool), help_heading = "Opciones")]
    pub version: (),
}

impl Cli {
    /// Traduce lo escrito en la línea de comandos a opciones del lote.
    pub fn opciones(&self) -> Opciones {
        let mut formatos = self.formato.clone();
        formatos.sort();
        formatos.dedup();
        let hilos = match self.hilos {
            Some(hilos) => usize::from(hilos),
            None => lote::hilos_por_defecto(),
        };
        Opciones {
            entrada: self.entrada.clone(),
            salida: self.salida.clone(),
            formatos,
            ancho_max: self.ancho,
            calidad: self.calidad,
            hilos,
            forzar: self.forzar,
            simular: self.simular,
        }
    }
}

/// Colores de la ayuda, con el acento de la marca.
fn estilos() -> Styles {
    Styles::styled()
        .header(ACENTO.bold())
        .usage(ACENTO.bold())
        .literal(NEGRITA)
        .placeholder(TENUE)
}

/// Plantilla de la ayuda con el encabezado «Uso» en español (el resto de
/// encabezados salen de `help_heading`).
fn plantilla() -> String {
    let uso = ACENTO.bold();
    let mut texto = String::from("{about-with-newline}\n");
    texto.push_str(&format!("{uso}Uso:{uso:#} {{usage}}\n\n"));
    texto.push_str("{all-args}{after-help}");
    texto
}

/// Ejemplos y códigos de salida al final de la ayuda.
fn ejemplos() -> String {
    let t = ACENTO.bold();
    let ejemplos = format!("{t}Ejemplos:{t:#}\n{EJEMPLOS}");
    format!("{ejemplos}\n\n{t}Códigos de salida:{t:#}\n{CODIGOS}")
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;
    use clap::CommandFactory;

    use super::*;

    fn leer(argumentos: &[&str]) -> Result<Opciones, clap::Error> {
        let mut todos = vec!["afinar"];
        todos.extend_from_slice(argumentos);
        Cli::try_parse_from(todos).map(|cli| cli.opciones())
    }

    #[test]
    fn la_definicion_es_coherente() {
        Cli::command().debug_assert();
    }

    #[test]
    fn valores_por_defecto() {
        let opciones = leer(&["fotos"]).unwrap();
        assert_eq!(opciones.entrada, PathBuf::from("fotos"));
        assert_eq!(opciones.salida, PathBuf::from("optimizadas"));
        assert_eq!(opciones.formatos, [Formato::Webp, Formato::Avif]);
        assert_eq!(opciones.ancho_max, None);
        assert_eq!(opciones.calidad, 80);
        assert!(opciones.hilos >= 1);
        assert!(!opciones.forzar);
        assert!(!opciones.simular);
    }

    #[test]
    fn formatos_separados_por_comas_y_sin_repetir() {
        let opciones = leer(&["fotos", "--formato", "avif,webp,avif"]).unwrap();
        assert_eq!(opciones.formatos, [Formato::Webp, Formato::Avif]);
        let solo_webp = leer(&["fotos", "-f", "webp"]).unwrap();
        assert_eq!(solo_webp.formatos, [Formato::Webp]);
    }

    #[test]
    fn opciones_cortas_y_largas() {
        let argumentos = ["fotos", "-a", "1600", "-c", "70", "-s", "web"];
        let opciones = leer(&argumentos).unwrap();
        assert_eq!(opciones.ancho_max, Some(1600));
        assert_eq!(opciones.calidad, 70);
        assert_eq!(opciones.salida, PathBuf::from("web"));
        let banderas = leer(&["fotos", "--forzar", "--simular", "--hilos", "2"]).unwrap();
        assert!(banderas.forzar && banderas.simular);
        assert_eq!(banderas.hilos, 2);
    }

    #[test]
    fn rechaza_valores_fuera_de_rango() {
        assert!(leer(&["fotos", "--calidad", "0"]).is_err());
        assert!(leer(&["fotos", "--calidad", "101"]).is_err());
        assert!(leer(&["fotos", "--ancho", "0"]).is_err());
        assert!(leer(&["fotos", "--ancho", "20000"]).is_err());
        assert!(leer(&["fotos", "--hilos", "0"]).is_err());
        assert!(leer(&["fotos", "--formato", "gif"]).is_err());
        assert!(leer(&[]).is_err());
    }

    #[test]
    fn ayuda_en_espanol() {
        let error = leer(&["--help"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::DisplayHelp);
        let ayuda = error.to_string();
        for texto in ["Uso:", "Argumentos:", "Opciones:", "Ejemplos:"] {
            assert!(ayuda.contains(texto), "falta «{texto}» en la ayuda");
        }
        assert!(ayuda.contains("--calidad <1-100>"));
    }

    #[test]
    fn version_con_firma() {
        let error = leer(&["--version"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::DisplayVersion);
        assert!(error.to_string().contains("Hecho por Rubén Tur"));
    }
}
