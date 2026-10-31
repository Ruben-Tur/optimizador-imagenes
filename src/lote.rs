//! Planificación y ejecución del lote en paralelo.

use std::collections::{BTreeMap, HashMap};
use std::num::NonZeroUsize;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use rayon::prelude::*;
use serde::Serialize;

use crate::error::{ErrorArchivo, Fallo};
use crate::formato::Formato;
use crate::imagen::{self, Pixeles};
use crate::rutas::{self, Original};

/// Lo que se le pide al lote.
#[derive(Debug, Clone)]
pub struct Opciones {
    /// Carpeta con los originales.
    pub entrada: PathBuf,
    /// Carpeta donde se escriben los resultados.
    pub salida: PathBuf,
    /// Formatos a generar, sin repetir.
    pub formatos: Vec<Formato>,
    /// Ancho máximo en píxeles; `None` conserva el original.
    pub ancho_max: Option<u32>,
    /// Calidad de 1 a 100.
    pub calidad: u8,
    /// Hilos de trabajo.
    pub hilos: usize,
    /// Convertir aunque el resultado esté al día.
    pub forzar: bool,
    /// Solo calcular qué se haría.
    pub simular: bool,
}

/// Un archivo generado.
#[derive(Debug, Clone)]
pub struct Conversion {
    pub formato: Formato,
    pub ruta: PathBuf,
    pub bytes: u64,
    pub ancho: u32,
    pub alto: u32,
}

/// Cómo ha acabado una imagen.
#[derive(Debug, Clone)]
pub enum Estado {
    /// Convertida a estos formatos (los que faltaban).
    Convertida(Vec<Conversion>),
    /// Todos sus resultados existían y eran posteriores al original.
    AlDia,
    /// Simulación: se convertiría a estos formatos.
    Pendiente(Vec<Formato>),
    /// No se ha podido procesar; el resto del lote ha seguido.
    Fallida(Fallo),
}

/// Resultado de una imagen.
#[derive(Debug, Clone)]
pub struct Resultado {
    /// Ruta dentro de la carpeta de entrada.
    pub relativa: PathBuf,
    /// Tamaño del original en bytes.
    pub bytes: u64,
    pub estado: Estado,
}

impl Resultado {
    fn nuevo(origen: &Original, estado: Estado) -> Resultado {
        Resultado {
            relativa: origen.relativa.clone(),
            bytes: origen.bytes,
            estado,
        }
    }
}

/// Recuento de imágenes por estado.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Cuenta {
    pub imagenes: usize,
    pub convertidas: usize,
    pub al_dia: usize,
    pub pendientes: usize,
    pub fallidas: usize,
}

/// Bytes antes y después de un formato.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Total {
    pub archivos: usize,
    pub originales: u64,
    pub nuevos: u64,
}

/// Todo lo ocurrido en el lote, ordenado por ruta.
#[derive(Debug)]
pub struct Informe {
    pub resultados: Vec<Resultado>,
    pub hilos: usize,
    pub duracion: Duration,
    pub simulado: bool,
}

impl Informe {
    /// Cuántas imágenes han acabado en cada estado.
    pub fn cuenta(&self) -> Cuenta {
        let mut cuenta = Cuenta {
            imagenes: self.resultados.len(),
            ..Cuenta::default()
        };
        for resultado in &self.resultados {
            match resultado.estado {
                Estado::Convertida(_) => cuenta.convertidas += 1,
                Estado::AlDia => cuenta.al_dia += 1,
                Estado::Pendiente(_) => cuenta.pendientes += 1,
                Estado::Fallida(_) => cuenta.fallidas += 1,
            }
        }
        cuenta
    }

    /// Bytes antes y después de cada formato, solo de lo convertido ahora.
    pub fn totales(&self) -> Vec<(Formato, Total)> {
        let mut totales: BTreeMap<Formato, Total> = BTreeMap::new();
        for resultado in &self.resultados {
            let Estado::Convertida(conversiones) = &resultado.estado else {
                continue;
            };
            for conversion in conversiones {
                let total = totales.entry(conversion.formato).or_default();
                total.archivos += 1;
                total.originales += resultado.bytes;
                total.nuevos += conversion.bytes;
            }
        }
        totales.into_iter().collect()
    }
}

/// Trabajo pendiente de una imagen: qué formatos le faltan.
#[derive(Debug)]
struct Tarea {
    origen: Original,
    faltan: Vec<Formato>,
}

/// Lo que hay que hacer, calculado antes de convertir nada.
#[derive(Debug, Default)]
pub struct Plan {
    tareas: Vec<Tarea>,
    resueltos: Vec<Resultado>,
    salida_resuelta: PathBuf,
}

impl Plan {
    /// Imágenes encontradas, incluidas las que ya están al día o fallan.
    pub fn total(&self) -> usize {
        self.tareas.len() + self.resueltos.len()
    }

    /// Imágenes que hay que convertir (o que se convertirían al simular).
    pub fn por_convertir(&self) -> usize {
        self.tareas.len()
    }

    /// Lo que ya se sabe antes de convertir: al día, conflictos, carpetas
    /// ilegibles.
    pub fn resueltos(&self) -> &[Resultado] {
        &self.resueltos
    }

    /// Ruta relativa más larga, en caracteres, para alinear el informe.
    pub fn nombre_mas_largo(&self) -> usize {
        let tareas = self.tareas.iter().map(|t| &t.origen.relativa);
        let resueltos = self.resueltos.iter().map(|r| &r.relativa);
        let mut largo = 0;
        for ruta in tareas.chain(resueltos) {
            largo = largo.max(rutas::legible(ruta).chars().count());
        }
        largo
    }

    fn anotar(&mut self, resultado: Resultado) {
        self.resueltos.push(resultado);
    }
}

/// Un hilo por núcleo lógico.
pub fn hilos_por_defecto() -> usize {
    thread::available_parallelism().map_or(1, NonZeroUsize::get)
}

/// Comprueba las carpetas, busca las imágenes y decide qué hay que hacer
/// con cada una. No escribe nada.
pub fn planificar(opciones: &Opciones) -> anyhow::Result<Plan> {
    let carpetas = rutas::validar_carpetas(&opciones.entrada, &opciones.salida)?;
    let excluir = carpetas.excluir.as_deref();
    let busqueda = rutas::buscar_imagenes(&opciones.entrada, excluir);
    let mut plan = Plan {
        salida_resuelta: carpetas.salida_resuelta,
        ..Plan::default()
    };
    for problema in busqueda.problemas {
        let fallo = Fallo {
            mensaje: problema.mensaje,
            detalle: None,
        };
        let resultado = Resultado {
            relativa: problema.relativa,
            bytes: 0,
            estado: Estado::Fallida(fallo),
        };
        plan.anotar(resultado);
    }
    let mut nombres: HashMap<String, PathBuf> = HashMap::new();
    for origen in busqueda.originales {
        let clave = clave_de_salida(&origen.relativa);
        if let Some(previo) = nombres.get(&clave) {
            let error = ErrorArchivo::Conflicto(rutas::legible(previo));
            let estado = Estado::Fallida(Fallo::from(&error));
            plan.anotar(Resultado::nuevo(&origen, estado));
            continue;
        }
        nombres.insert(clave, origen.relativa.clone());
        let faltan = pendientes(&origen, opciones);
        if faltan.is_empty() {
            plan.anotar(Resultado::nuevo(&origen, Estado::AlDia));
        } else {
            plan.tareas.push(Tarea { origen, faltan });
        }
    }
    Ok(plan)
}

/// Dos originales que solo se diferencian en la extensión (`foto.jpg` y
/// `foto.png`) darían el mismo resultado. Se compara sin distinguir
/// mayúsculas porque Windows y macOS tampoco lo hacen.
fn clave_de_salida(relativa: &Path) -> String {
    let sin_extension = relativa.with_extension("");
    rutas::legible(&sin_extension).to_lowercase()
}

/// Formatos cuyo resultado falta o es anterior al original.
fn pendientes(origen: &Original, opciones: &Opciones) -> Vec<Formato> {
    let mut faltan = Vec::new();
    for &formato in &opciones.formatos {
        let destino = rutas::ruta_salida(&opciones.salida, &origen.relativa, formato);
        if opciones.forzar || !rutas::esta_al_dia(&destino, origen.modificada) {
            faltan.push(formato);
        }
    }
    faltan
}

/// Ejecuta el plan repartiendo las imágenes entre `opciones.hilos` hilos.
///
/// `avisar` se llama desde los hilos de trabajo cada vez que termina una
/// imagen, para ir mostrando el progreso.
pub fn ejecutar(
    plan: Plan,
    opciones: &Opciones,
    avisar: &(dyn Fn(&Resultado) + Sync),
) -> anyhow::Result<Informe> {
    let inicio = Instant::now();
    let mut resultados = plan.resueltos;
    if opciones.simular {
        for tarea in plan.tareas {
            let estado = Estado::Pendiente(tarea.faltan);
            resultados.push(Resultado::nuevo(&tarea.origen, estado));
        }
    } else {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(opciones.hilos)
            .build()
            .context("no se pudieron preparar los hilos de trabajo")?;
        let salida = plan.salida_resuelta.as_path();
        let tareas = &plan.tareas;
        let hechos: Vec<Resultado> = pool.install(|| {
            tareas
                .par_iter()
                .map(|tarea| procesar_a_salvo(tarea, opciones, salida))
                .inspect(avisar)
                .collect()
        });
        resultados.extend(hechos);
    }
    resultados.sort_by_key(|r| r.relativa.clone());
    Ok(Informe {
        resultados,
        hilos: opciones.hilos,
        duracion: inicio.elapsed(),
        simulado: opciones.simular,
    })
}

/// Procesa una imagen sin que un fallo, ni siquiera un pánico dentro de una
/// biblioteca, pueda detener el resto del lote.
fn procesar_a_salvo(tarea: &Tarea, opciones: &Opciones, salida: &Path) -> Resultado {
    let trabajo = || procesar(tarea, opciones, salida);
    let estado = match panic::catch_unwind(AssertUnwindSafe(trabajo)) {
        Ok(Ok(conversiones)) => Estado::Convertida(conversiones),
        Ok(Err(error)) => Estado::Fallida(Fallo::from(&error)),
        Err(_) => Estado::Fallida(Fallo::from(&ErrorArchivo::Interno)),
    };
    Resultado::nuevo(&tarea.origen, estado)
}

/// Lee, redimensiona y codifica en memoria todos los formatos; solo si todo
/// ha ido bien escribe los archivos, para no dejar resultados a medias.
fn procesar(
    tarea: &Tarea,
    opciones: &Opciones,
    salida_resuelta: &Path,
) -> Result<Vec<Conversion>, ErrorArchivo> {
    let origen = &tarea.origen;
    if origen.bytes > imagen::ARCHIVO_MAXIMO {
        return Err(ErrorArchivo::DemasiadoGrande);
    }
    let foto = imagen::leer(&origen.ruta)?;
    let foto = imagen::redimensionar(foto, opciones.ancho_max);
    let pixeles = Pixeles::desde(foto);
    let mut codificados = Vec::with_capacity(tarea.faltan.len());
    for &formato in &tarea.faltan {
        let datos = imagen::codificar(&pixeles, formato, opciones.calidad)?;
        codificados.push((formato, datos));
    }
    let mut conversiones = Vec::with_capacity(codificados.len());
    for (formato, datos) in codificados {
        let destino = rutas::ruta_salida(&opciones.salida, &origen.relativa, formato);
        rutas::escribir_dentro(salida_resuelta, &destino, &datos)?;
        let conversion = Conversion {
            formato,
            ruta: destino,
            bytes: datos.len() as u64,
            ancho: pixeles.ancho,
            alto: pixeles.alto,
        };
        conversiones.push(conversion);
    }
    Ok(conversiones)
}
