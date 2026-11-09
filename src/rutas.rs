//! Qué archivos se procesan, dónde se escribe cada resultado y las
//! comprobaciones que impiden tocar los originales o escribir fuera de la
//! carpeta de salida.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{self, Component, Path, PathBuf};
use std::process;
use std::time::SystemTime;

use anyhow::{bail, Context};
use walkdir::{DirEntry, WalkDir};

use crate::error::ErrorArchivo;
use crate::formato::Formato;

/// Extensiones que se leen, sin distinguir mayúsculas.
pub const EXTENSIONES: [&str; 4] = ["jpg", "jpeg", "png", "webp"];

const SALIDA_CONTIENE: &str = "la salida no puede ser la entrada ni una carpeta que la contenga";

/// Una imagen encontrada en la carpeta de entrada.
#[derive(Debug, Clone)]
pub struct Original {
    /// Ruta para leerla.
    pub ruta: PathBuf,
    /// Ruta dentro de la carpeta de entrada; se replica en la de salida.
    pub relativa: PathBuf,
    /// Tamaño en bytes.
    pub bytes: u64,
    /// Fecha de modificación, para saber si un resultado está al día.
    pub modificada: Option<SystemTime>,
}

/// Algo encontrado al recorrer la carpeta que no se puede procesar.
#[derive(Debug, Clone)]
pub struct Problema {
    /// Ruta dentro de la carpeta de entrada.
    pub relativa: PathBuf,
    /// Qué ha pasado, en español.
    pub mensaje: String,
}

/// Resultado de recorrer la carpeta de entrada.
#[derive(Debug, Default)]
pub struct Busqueda {
    /// Imágenes que se pueden procesar, en orden alfabético.
    pub originales: Vec<Original>,
    /// Carpetas o archivos que no se han podido leer.
    pub problemas: Vec<Problema>,
}

/// Las carpetas del lote ya comprobadas.
#[derive(Debug, Clone)]
pub struct Carpetas {
    /// Carpeta de salida absoluta y sin enlaces simbólicos.
    pub salida_resuelta: PathBuf,
    /// Si la salida está dentro de la entrada, su ruta relativa a ella, para
    /// no volver a leer los resultados como si fueran originales.
    pub excluir: Option<PathBuf>,
}

/// Comprueba las carpetas antes de empezar.
///
/// La salida puede estar dentro de la entrada (se excluye del recorrido),
/// pero no puede ser la misma carpeta ni contenerla: los resultados `.webp`
/// acabarían escritos encima de los originales.
pub fn validar_carpetas(entrada: &Path, salida: &Path) -> anyhow::Result<Carpetas> {
    let datos = match fs::metadata(entrada) {
        Ok(datos) => datos,
        Err(error) => bail!("no se puede abrir «{}»: {error}", entrada.display()),
    };
    if !datos.is_dir() {
        bail!("«{}» no es una carpeta", entrada.display());
    }
    let entrada_real = fs::canonicalize(entrada).context("no se pudo resolver la entrada")?;
    let salida_resuelta = absoluta(salida)?;
    if entrada_real.starts_with(&salida_resuelta) {
        bail!(SALIDA_CONTIENE);
    }
    let dentro = salida_resuelta.strip_prefix(entrada_real).ok();
    let excluir = dentro.map(Path::to_path_buf);
    Ok(Carpetas {
        salida_resuelta,
        excluir,
    })
}

/// Ruta absoluta y sin enlaces simbólicos aunque todavía no exista: se
/// resuelve el tramo que ya existe y se le añade el resto.
///
/// Si el tramo que existe es un archivo, se rechaza aquí: si no, cada
/// imagen fallaría por separado al crear su carpeta.
fn absoluta(ruta: &Path) -> anyhow::Result<PathBuf> {
    let completa = path::absolute(ruta).context("ruta de salida no válida")?;
    for ancestro in completa.ancestors() {
        if !ancestro.exists() {
            continue;
        }
        if !ancestro.is_dir() {
            let archivo = ancestro.display();
            bail!("la salida no es una carpeta: «{archivo}» es un archivo");
        }
        let resto = completa.strip_prefix(ancestro)?;
        if !resto.as_os_str().is_empty() && !es_relativa_segura(resto) {
            bail!("la salida no puede usar «..» si aún no existe");
        }
        let real = fs::canonicalize(ancestro).context("no se pudo resolver la salida")?;
        return Ok(real.join(resto));
    }
    Ok(completa)
}

/// Recorre la carpeta de entrada en orden alfabético sin seguir enlaces
/// simbólicos, de modo que nunca se lee nada de fuera de ella.
pub fn buscar_imagenes(entrada: &Path, excluir: Option<&Path>) -> Busqueda {
    let mut busqueda = Busqueda::default();
    let recorrido = WalkDir::new(entrada)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| !es_excluida(e.path(), entrada, excluir));
    for elemento in recorrido {
        match elemento {
            Ok(elemento) => busqueda.anotar(&elemento, entrada),
            Err(error) => busqueda.anotar_error(&error, entrada),
        }
    }
    busqueda
}

impl Busqueda {
    fn anotar(&mut self, elemento: &DirEntry, entrada: &Path) {
        // Como no se siguen los enlaces simbólicos, un enlace nunca cuenta
        // como archivo normal y se ignora.
        if !elemento.file_type().is_file() || !es_imagen(elemento.path()) {
            return;
        }
        let ruta = elemento.path();
        let relativa = ruta.strip_prefix(entrada).unwrap_or(ruta);
        let relativa = relativa.to_path_buf();
        if !es_relativa_segura(&relativa) {
            self.problema(relativa, ErrorArchivo::RutaNoValida.to_string());
            return;
        }
        let datos = match elemento.metadata() {
            Ok(datos) => datos,
            Err(error) => {
                self.problema(relativa, format!("no se pudo leer: {error}"));
                return;
            }
        };
        let original = Original {
            ruta: ruta.to_path_buf(),
            relativa,
            bytes: datos.len(),
            modificada: datos.modified().ok(),
        };
        self.originales.push(original);
    }

    fn anotar_error(&mut self, error: &walkdir::Error, entrada: &Path) {
        let ruta = error.path().unwrap_or(entrada);
        let relativa = ruta.strip_prefix(entrada).unwrap_or(ruta);
        let relativa = relativa.to_path_buf();
        let motivo = match error.io_error() {
            Some(io) => io.to_string(),
            None => error.to_string(),
        };
        self.problema(relativa, format!("no se pudo abrir: {motivo}"));
    }

    fn problema(&mut self, relativa: PathBuf, texto: String) {
        let problema = Problema {
            relativa,
            mensaje: texto,
        };
        self.problemas.push(problema);
    }
}

fn es_excluida(ruta: &Path, entrada: &Path, excluir: Option<&Path>) -> bool {
    match (excluir, ruta.strip_prefix(entrada)) {
        (Some(excluir), Ok(relativa)) => relativa == excluir,
        _ => false,
    }
}

/// ¿Tiene una extensión que sabemos leer?
pub fn es_imagen(ruta: &Path) -> bool {
    let Some(extension) = ruta.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    let extension = extension.to_ascii_lowercase();
    EXTENSIONES.contains(&extension.as_str())
}

/// Una ruta relativa solo es segura si todos sus tramos son nombres
/// normales: nada de `..`, raíces ni unidades que la saquen de su carpeta.
pub fn es_relativa_segura(ruta: &Path) -> bool {
    let mut vacia = true;
    for tramo in ruta.components() {
        if !matches!(tramo, Component::Normal(_)) {
            return false;
        }
        vacia = false;
    }
    !vacia
}

/// Dónde se escribe un resultado: la misma ruta relativa dentro de la
/// carpeta de salida, con la extensión del formato.
pub fn ruta_salida(salida: &Path, relativa: &Path, formato: Formato) -> PathBuf {
    let destino = salida.join(relativa);
    destino.with_extension(formato.extension())
}

/// Ruta relativa con «/» como separador en todos los sistemas, para
/// mostrarla en el informe y en el JSON.
pub fn legible(ruta: &Path) -> String {
    let mut texto = String::new();
    for tramo in ruta.components() {
        if !texto.is_empty() {
            texto.push('/');
        }
        texto.push_str(&tramo.as_os_str().to_string_lossy());
    }
    if texto.is_empty() {
        texto.push('.');
    }
    texto
}

/// Un resultado está al día si existe y no es anterior al original.
pub fn esta_al_dia(destino: &Path, original: Option<SystemTime>) -> bool {
    let Some(fecha_original) = original else {
        return false;
    };
    let fecha = fs::metadata(destino).and_then(|m| m.modified());
    fecha.is_ok_and(|fecha| fecha >= fecha_original)
}

/// Escribe un resultado comprobando antes que su carpeta, ya resuelta, sigue
/// dentro de la de salida: un enlace simbólico podría sacarlo de ella.
pub fn escribir_dentro(
    salida_resuelta: &Path,
    destino: &Path,
    datos: &[u8],
) -> Result<(), ErrorArchivo> {
    let carpeta = destino.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(carpeta).map_err(ErrorArchivo::Escritura)?;
    let real = fs::canonicalize(carpeta).map_err(ErrorArchivo::Escritura)?;
    if !real.starts_with(salida_resuelta) {
        return Err(ErrorArchivo::FueraDeSalida);
    }
    escribir_atomico(destino, datos).map_err(ErrorArchivo::Escritura)
}

/// Escribe en un archivo temporal de la misma carpeta y lo renombra al
/// final, así nunca queda un resultado a medias con el nombre definitivo
/// que la siguiente ejecución daría por bueno.
pub fn escribir_atomico(destino: &Path, datos: &[u8]) -> io::Result<()> {
    let temporal = ruta_temporal(destino);
    let escrito = fs::write(&temporal, datos).and_then(|()| fs::rename(&temporal, destino));
    if escrito.is_err() {
        // El error que importa es el primero; el temporal se borra si existe.
        let _ = fs::remove_file(temporal);
    }
    escrito
}

/// `.foto.webp.<pid>.tmp`: oculto, único por proceso y sin extensión de
/// imagen, para que nunca se confunda con un original.
fn ruta_temporal(destino: &Path) -> PathBuf {
    let mut nombre = OsString::from(".");
    nombre.push(destino.file_name().unwrap_or_default());
    nombre.push(format!(".{}.tmp", process::id()));
    destino.with_file_name(nombre)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn la_salida_replica_la_ruta_con_otra_extension() {
        let salida = ruta_salida(
            Path::new("optimizadas"),
            Path::new("viajes/playa.jpg"),
            Formato::Avif,
        );
        assert_eq!(salida, Path::new("optimizadas/viajes/playa.avif"));
    }

    #[test]
    fn solo_cambia_la_ultima_extension() {
        let salida = ruta_salida(Path::new("s"), Path::new("foto.v2.png"), Formato::Webp);
        assert_eq!(salida, Path::new("s/foto.v2.webp"));
    }

    #[test]
    fn rutas_relativas_seguras() {
        assert!(es_relativa_segura(Path::new("viajes/playa.jpg")));
        assert!(!es_relativa_segura(Path::new("../fuera.jpg")));
        assert!(!es_relativa_segura(Path::new("viajes/../../fuera.jpg")));
        assert!(!es_relativa_segura(Path::new("/etc/foto.jpg")));
        assert!(!es_relativa_segura(Path::new("")));
    }

    #[test]
    fn extensiones_admitidas_sin_distinguir_mayusculas() {
        assert!(es_imagen(Path::new("FOTO.JPG")));
        assert!(es_imagen(Path::new("logo.WebP")));
        assert!(es_imagen(Path::new("plano.jpeg")));
        assert!(!es_imagen(Path::new("animacion.gif")));
        assert!(!es_imagen(Path::new("LEEME")));
    }

    #[test]
    fn legible_usa_barras_normales() {
        let ruta = Path::new("blog").join("2025").join("tortilla.jpg");
        assert_eq!(legible(&ruta), "blog/2025/tortilla.jpg");
        assert_eq!(legible(Path::new("")), ".");
    }

    #[test]
    fn rechaza_salida_igual_o_que_contiene_la_entrada() {
        let dir = TempDir::new().unwrap();
        let entrada = dir.path().join("fotos");
        fs::create_dir(&entrada).unwrap();
        assert!(validar_carpetas(&entrada, &entrada).is_err());
        assert!(validar_carpetas(&entrada, dir.path()).is_err());
    }

    #[test]
    fn salida_dentro_de_la_entrada_se_excluye() {
        let dir = TempDir::new().unwrap();
        let entrada = dir.path().join("fotos");
        fs::create_dir(&entrada).unwrap();
        let carpetas = validar_carpetas(&entrada, &entrada.join("web")).unwrap();
        assert_eq!(carpetas.excluir, Some(PathBuf::from("web")));
        let fuera = validar_carpetas(&entrada, &dir.path().join("web")).unwrap();
        assert_eq!(fuera.excluir, None);
    }

    #[test]
    fn la_salida_no_puede_ser_un_archivo() {
        let dir = TempDir::new().unwrap();
        let entrada = dir.path().join("fotos");
        fs::create_dir(&entrada).unwrap();
        let archivo = dir.path().join("notas.txt");
        fs::write(&archivo, b"x").unwrap();
        assert!(validar_carpetas(&entrada, &archivo).is_err());
        assert!(validar_carpetas(&entrada, &archivo.join("web")).is_err());
    }

    #[test]
    fn la_entrada_tiene_que_ser_una_carpeta() {
        let dir = TempDir::new().unwrap();
        let archivo = dir.path().join("foto.jpg");
        fs::write(&archivo, b"x").unwrap();
        let salida = dir.path().join("salida");
        assert!(validar_carpetas(&archivo, &salida).is_err());
        let no_existe = dir.path().join("no-existe");
        assert!(validar_carpetas(&no_existe, &salida).is_err());
    }

    #[test]
    fn al_dia_segun_la_fecha_de_modificacion() {
        let dir = TempDir::new().unwrap();
        let destino = dir.path().join("foto.webp");
        fs::write(&destino, b"x").unwrap();
        let futuro = SystemTime::now() + Duration::from_secs(3600);
        assert!(esta_al_dia(&destino, Some(SystemTime::UNIX_EPOCH)));
        assert!(!esta_al_dia(&destino, Some(futuro)));
        assert!(!esta_al_dia(&destino, None));
        assert!(!esta_al_dia(&dir.path().join("otra.webp"), None));
    }

    #[test]
    fn busca_en_orden_y_respeta_la_exclusion() {
        let dir = TempDir::new().unwrap();
        let raiz = dir.path();
        fs::create_dir_all(raiz.join("v")).unwrap();
        fs::create_dir_all(raiz.join("web")).unwrap();
        for nombre in ["b.png", "a.jpg", "x.txt", "v/c.webp", "web/a.webp"] {
            fs::write(raiz.join(nombre), b"x").unwrap();
        }
        let busqueda = buscar_imagenes(raiz, Some(Path::new("web")));
        let mut nombres = Vec::new();
        for original in &busqueda.originales {
            nombres.push(legible(&original.relativa));
        }
        assert_eq!(nombres, ["a.jpg", "b.png", "v/c.webp"]);
        assert!(busqueda.problemas.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn no_sigue_enlaces_simbolicos() {
        let dir = TempDir::new().unwrap();
        let entrada = dir.path().join("fotos");
        fs::create_dir(&entrada).unwrap();
        let secreto = dir.path().join("secreto.jpg");
        fs::write(&secreto, b"x").unwrap();
        std::os::unix::fs::symlink(secreto, entrada.join("enlace.jpg")).unwrap();
        let busqueda = buscar_imagenes(&entrada, None);
        assert!(busqueda.originales.is_empty());
    }

    #[test]
    fn escribe_de_forma_atomica_y_sin_temporales() {
        let dir = TempDir::new().unwrap();
        let salida = fs::canonicalize(dir.path()).unwrap();
        let destino = salida.join("viajes").join("playa.webp");
        escribir_dentro(&salida, &destino, b"datos").unwrap();
        assert_eq!(fs::read(destino).unwrap(), b"datos");
        let restos = fs::read_dir(salida.join("viajes")).unwrap().count();
        assert_eq!(restos, 1);
    }

    #[test]
    fn no_escribe_fuera_de_la_salida() {
        let dir = TempDir::new().unwrap();
        let raiz = fs::canonicalize(dir.path()).unwrap();
        let salida = raiz.join("salida");
        fs::create_dir(&salida).unwrap();
        let destino = raiz.join("otra").join("foto.webp");
        let resultado = escribir_dentro(&salida, &destino, b"datos");
        assert!(matches!(resultado, Err(ErrorArchivo::FueraDeSalida)));
    }
}
