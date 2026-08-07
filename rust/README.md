# SIGER en Rust

**El producto, entero.** El cutover fue el 2026-07-28: la versión de Python se
retiró a la historia de git una vez el diferencial pasó completo, y hoy SIGER es
un solo binario con la interfaz dentro. De aquella etapa solo sobrevive lo que
sirve al cliente y no a la aplicación: `sembrar_demo.py`, y el
`verificar_clausulado.py` de la raíz.

## Qué hay

| Pieza | Qué es |
|---|---|
| `siger-core` | El motor: calendario, periodicidad, vencimientos. Sin E/S, sin base de datos, sin red. |
| `siger-api` | El servidor: axum + SQLx, migraciones embebidas, sesión en cookie, y **el panel**: `maud` + htmx, compilado por `rustc` junto al resto. |
| `siger-api/estaticos/` | Los cuatro activos que se sirven tal cual, VENDORIZADOS y versionados: htmx, `boi.js`, `comportamientos.js` y `panel.css`. Sin una sola petición a un CDN. |

## Construir

```bash
./rust/construir.sh      # binario, y la pantalla dibujada en un navegador
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

Los corpus se congelaron con el motor de Python **antes** del cutover y viajan
versionados en el repositorio (`siger-core/tests/oraculo/` y los
`oraculo_*.json`). Ya no hay guion que los regenere, y es a propósito: un oráculo
que se puede rehacer deja de ser un oráculo.

```bash
cd rust && cargo test -p siger-core       # exige que el de Rust produzca lo mismo
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

**Nada de la versión de Python.** El port quedó completo el 2026-08-03, cuando
entró la licencia offline, que era lo último que faltaba.

Eso no significa que no haya trabajo por hacer: hay fichas de mejora abiertas
—cosas que el sistema hace y podría hacer mejor—, y viven como tareas y PR, no
aquí. Esta sección solo responde a «¿le falta algo respecto a lo que había?», y la
respuesta es que no. **Antes de dar por pendiente cualquier cosa, mira los PR
abiertos y las ramas** (`gh pr list --state open`,
`git branch -r --no-merged origin/main`): «no está en `main`» y «no está hecho»
no son lo mismo. Hasta el 2026-08-03 el trabajo terminado esperaba turno de merge
los **lunes**; ese régimen se acabó —ahora se mergea cuando está listo, y el
límite es el cupo de minutos de Actions, no el calendario—. Al 2026-08-04 los dos
comandos salen vacíos: no hay nada esperando.

Lo que sí está, y este documento daba por pendiente hasta el 2026-08-01:
la inferencia de periodicidad (`siger-core/src/inferencia.rs`), la firma de
entregas (`siger-api/src/firma.rs`), el planificador en segundo plano
(`planificador.rs`), la subida de archivos (`POST /api/entregas/{id}/archivo`,
con tope de cuerpo y comprobación de pertenencia) y la carga de la base anterior
(`siger-api importar <origen> <destino>`). Además, posteriores al texto original:
el **cifrado en reposo** de las entregas, el **freno** al ensayo de contraseñas
con revocación de sesiones, `respaldar`/`restaurar` con copia verificada, la
**administración de cuentas** y la **verificación en dos pasos** (TOTP, con sus
tres pantallas y el QR dibujado por el propio binario en `qr.rs`, sin ninguna
dependencia nueva). Y la **licencia offline** (`licencia.rs`), que cierra el port:
archivo firmado verificado en local, sin red, con los diez campos del Anexo A del
EULA — y que **no bloquea nada nunca**, ni ausente, ni vencida, ni alterada, ni con
exceso de usuarios.
