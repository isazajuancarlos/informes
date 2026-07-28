# SIGER en Rust + Angular

Reescritura de la arquitectura. Conviven con la versión de Python **a
propósito**: la de Python es la implementación de referencia contra la que se
mide esta, y no se retira hasta que el diferencial pase entero.

## Qué hay

| Pieza | Qué es |
|---|---|
| `siger-core` | El motor: calendario, periodicidad, vencimientos. Sin E/S, sin base de datos, sin red. |
| `siger-api` | El servidor: axum + SQLx, migraciones embebidas, sesión en cookie, y la interfaz **dentro del binario**. |
| `../angular` | La interfaz: Angular 22 + Material, sin una sola petición a un CDN. |

## Construir

```bash
./rust/construir.sh      # interfaz → empotrado → binario
```

Arrancar:

```bash
BIN=rust/target/release/siger-api
SIGER_SECRETO_SESION=$($BIN generar-secreto) $BIN
```

No hay `.env` obligatorio, no hay esquema que crear a mano y no hay migración
que recordar: la base se crea y se migra sola al arrancar.

## Cómo se sabe que el motor es correcto

No se revisó: se **midió**.

```bash
python herramientas/congelar_oraculo.py   # congela lo que produce el motor de Python
cd rust && cargo test                     # exige que el de Rust produzca lo mismo
```

El oráculo son **484 festivos día a día entre 2020 y 2045** y **8 534
vencimientos** —día nominal, día límite y tipo de ajuste— de los 80 patrones de
una base real, entre 2026 y 2030. Una sola fecha distinta pone la suite en rojo.

Ya sirvió: encontró que faltaba el festivo de Nuestra Señora del Rosario de
Chiquinquirá (9 de julio, trasladable, vigente desde 2026). Sin el diferencial
habría pasado, y habría movido las fechas límite de julio durante veinte años.

Y dejó ver algo de la versión de Python: sus festivos dependen de la **versión
instalada** de la librería `holidays`. Un cliente con una copia anterior a 2023
calcula los plazos de julio distinto, y nada se lo dice.

## Las invariantes, y dónde viven

1. **Un vencimiento tiene dos fechas.** `FechaNominal` y `FechaLimite` son tipos
   distintos: confundirlas no compila (`siger-core/src/fechas.rs`).
2. **Ante la falta de un dato, se falla.** Sin `SIGER_SECRETO_SESION` el binario
   no arranca; un país sin calendario es un error, no un calendario vacío
   (`siger-api/src/config.rs`, `siger-core/src/calendario.rs`).
3. **Ninguna ruta ni ningún filtro vienen del navegador.** Los extremos reciben
   un id y sacan el `WHERE` de `autz::Alcance`, que es el único sitio donde se
   decide qué puede ver quién (`siger-api/src/autz.rs`, `rutas.rs`).
4. **Lo opcional nunca bloquea.** La firma, la licencia y el correo se degradan;
   la entrega se registra igual.
5. **Nada sale a la red.** La interfaz no pide ni fuentes ni iconos a un CDN:
   dos de los tres perfiles se instalan sin internet.

## Lo que todavía no está

Se dice aquí y no en una lista aparte, para que no se lea como terminado:

- La **inferencia de periodicidad** (marcar fechas y que el sistema diga qué
  patrón son) sigue solo en Python: `app/services/inferir_periodicidad.py`.
- La **firma de entregas** con Quipu: en Rust son crates propios, pero el
  servicio aún no está portado.
- El **planificador en segundo plano** que crea las entregas.
- La **subida** de archivos (la descarga sí está, y con la comprobación de
  pertenencia que la versión anterior no tenía).
- La **carga de datos** existentes: falta el paso que lee la base de Python y la
  vuelca en este esquema.
