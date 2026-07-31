# CLAUDE.md

Guía para Claude Code en este repositorio. **Verificado contra el código, no
citado de memoria. Si algo aquí no cuadra con lo que ves, gana el repositorio —
y actualiza este archivo.**

## El producto

Se llama **SIGER** — *Sistema Integral de Gestión de Informes*. La carpeta se
llama `informes`: el repositorio no es el producto.

Gestiona obligaciones de entrega de informes: define cada obligación (quién la
produce, quién la recibe, cada cuánto), genera los vencimientos, avisa, recibe la
entrega, la firma y deja rastro auditable. Cuatro roles (ADMIN, OPERADOR,
SUPERVISOR, TERCERO) con control de acceso por alcance.

## Rust puro (cutover 2026-07-28)

SIGER **se migró entero a Rust** (estándar técnico de Xiliux) y el Python quedó
en la historia de git. Hoy es **un solo binario** con la interfaz **Angular
embebida** dentro del ejecutable y **SQLite** como base. No queda `app/`,
`main.py`, NiceGUI, Docker ni pytest: si lees documentación que los mencione,
está desactualizada.

Superviven, y a propósito, dos scripts de Python que **sirven al cliente**, no a
la app: `rust/sembrar_demo.py` (siembra la base de demostración) y
`verificar_clausulado.py` (regenera/verifica el deliverable legal
`MODELO_LICENCIA_INFORMES.md`).

### Estructura

```
rust/
  siger-core/   # dominio puro: el motor, sin I/O
    calendario.rs recurrencia.rs fechas.rs inferencia.rs patrones.rs compat.rs
    tests/oraculo.rs, oraculo_inferencia.rs  # oráculos diferenciales (ver abajo)
  siger-api/    # binario axum + Angular embebido (rust-embed) + SQLite (SQLx)
    main.rs rutas.rs db.rs config.rs sesion.rs autz.rs
    acceso.rs origen.rs  # freno del login: las dos consultas y de dónde viene quien pide
    participacion.rs informes.rs flujo.rs estadisticas.rs
    alertas.rs planificador.rs firma.rs importar.rs estaticos.rs respaldo.rs
    migrations/  # 0001_inicial … 0006_freno_y_revocacion (sqlx::migrate!)
angular/        # la interfaz; se construye e incrusta en el binario
```

## Comandos

```bash
cd rust
cargo build --release            # produce target/release/siger-api (Angular embebido)
cargo test  --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings   # el CI usa -D warnings

cd ../angular && npm ci && npx ng build                  # reconstruir la interfaz
```

MSRV 1.85 (evitar API que exija más; p. ej. `% 2 != 0`, no `is_multiple_of`).
El CI (`.github/workflows/ci.yml`) tiene dos trabajos: `rust` (test + clippy) y
`frontend` (ng build).

Subcomandos del binario: `importar <origen> <destino>` (vuelca una base del
Python anterior), `alertas` (una pasada, para un timer), `generar-secreto`,
`generar-claves-firma`, `hashear` (Argon2, mismos parámetros que el acceso).
Configuración por entorno en `config.rs`; **el binario no arranca si falta un
secreto** (no hay defectos peligrosos). Plantilla en `.env.example`.

## Invariantes que explican casi todo

1. **El flujo es lo único que el producto modela con columnas.** Quién produce,
   quién recibe, quién consume vive en **una sola tabla** de participación
   (`participacion.rs`). Lo demás —campos de la ficha, reglas— es configuración
   del cliente y vive como datos, no como esquema.
2. **Lo opcional nunca puede impedir cumplir (modo degradado, #71).** La firma
   sin clave, el correo sin SMTP, la alerta que falla: todo degrada con gracia.
   Un bloqueo convertiría a Xiliux en la causa del incumplimiento del cliente
   ante su ente de control. Verificado por inyección de fallo en `alertas.rs`
   (tests `degradado`): un SMTP caído no marca, no pierde y no propaga.
3. **Ante la ausencia de un dato, se falla ruidosamente; nunca se sustituye por un
   defecto.** `config.rs` no trae valores por defecto para secretos.

## Acceso y alcance (`autz.rs`)

Roles `Admin | Operador | Supervisor | Tercero`. El **alcance** de lo que cada
quien ve es un valor explícito: `Todas` (admin/supervisión), `DeUsuario(id)`,
`DeStakeholder(id)`, `Ninguna` («no le corresponde nada» es un valor legítimo, no
un hueco). Las consultas filtran por alcance; no hay «filtro opcional».

## La puerta: freno y revocación (`acceso.rs`, `origen.rs`, `sesion.rs`)

La cookie es `id|version|caducidad|firma` (HMAC-SHA256, `HttpOnly`,
`SameSite=Strict`; `Secure` lo añade el proxy). `version` es el `token_version`
del usuario: viaja firmado y se compara en la misma relectura que ya se hacía
para el rol, así que **subir el entero cierra de golpe todas las sesiones de esa
persona** sin guardar estado de sesión en el servidor.

El freno al ensayo de contraseñas cuenta por pareja **(correo, origen)**: cinco
fallos sin castigo, luego 15 s doblando con techo en 5 min, y un acierto lo borra.
**No bloquea la cuenta** — un bloqueo deja fuera al usuario legítimo, que es peor
que el ataque que evita. La REGLA vive en `guaca::freno`, compartida con medico;
aquí solo están las dos consultas. El origen es la dirección del socket salvo que
se declare `SIGER_TRAS_PROXY=1`, y si entonces no llega `X-Real-IP` **el acceso
falla**: rellenarlo con la dirección del proxy degradaría el freno a contar solo
por cuenta.

Las tres respuestas de rechazo —correo inexistente, cuenta desactivada, clave
mala— cuestan **lo mismo**: cuando no hay usuario se verifica contra un hash
señuelo. Sin eso, el mensaje es idéntico y el cronómetro dice qué correos existen.

## Firma de entregas (`firma.rs`, Quipu, opcional)

Firma post-cuántica (ML-DSA-87 + Ed25519) de los campos auditables **dentro de la
misma transacción** que fija estado, fecha y archivo, vía el contenedor de Quipu
(`encode_signed`/`decode_verified`). Sin `QUIPU_SIGNING_KEY`, la entrega se
registra igual, sin firma — la ausencia de firma nunca impide cumplir.

## El motor y sus oráculos (`siger-core`)

`recurrencia.rs` genera los vencimientos; `inferencia.rs` va del calendario a la
regla (`inferir`) y ofrece las lecturas posibles de una selección
(`interpretaciones`). **No se revisan: se MIDEN** contra corpus dorados que
produjo el motor de Python antes del cutover, congelados y vendorizados:
`tests/oraculo/{festivos_co,vencimientos}.json` (motor) y
`tests/oraculo_inferencia.json` + `tests/oraculo_interpretaciones.json`. Una sola
fecha distinta pone el test rojo.

## Licencia offline (`licencia.rs`)

Archivo firmado por el licenciante, verificado **en local** y sin red, con la
misma criptografía que sella las entregas. Los campos son los diez del Anexo A
del EULA, ni uno más: si el Anexo cambia, el módulo cambia con él.

**No bloquea nada. Nunca** — ni ausente, ni con el mantenimiento vencido, ni
alterada, ni con exceso de usuarios (invariante 2). Da visibilidad y evidencia;
el remedio ante el exceso es contractual. `veredicto` devuelve `Estado`, no
`Result`: no hay forma de que un llamante la convierta en un camino de error.

Dos detalles que parecen descuidos y no lo son:

- **`mantenimiento_hasta` NO es la vigencia.** El derecho de uso es perpetuo
  (3.1, 3.2, 7.6). Pasada la fecha se dice «mantenimiento vencido», nunca
  «licencia vencida»; y no lleva color de alarma, que solo merece `Invalida`.
- **El veredicto NO valida contra las listas del Anexo A; la emisión sí.** Un
  binario viejo que rechazara una edición nueva pintaría de rojo una licencia
  legítima y bien firmada. Quien dice si el documento es auténtico es la firma.

El par de claves del licenciante es DISTINTO del de las entregas
(`generar-claves-licencia`), y `INFORMES_LICENCIA_SK` no se acepta por argumento
—quedaría en el historial del intérprete y a la vista de `ps`—. Despliegue y
emisión, en `DESPLIEGUE.md`.

## Documentos de negocio (no se editan sin pedirlo)

`EULA_BORRADOR.md`, `LICENCIAMIENTO.md`, `MODELO_LICENCIA_INFORMES.md`,
`CONTRATO_SERVICIOS_DESARROLLO.md`, `PRECIOS_PORTAFOLIO.md`, `ANCLAJE_PRECIOS.md`,
los análisis de minutas: trabajo jurídico de Juan. `verificar_clausulado.py`
comprueba que EULA y modelo de licencia no se contradigan. Despliegue en
`DESPLIEGUE.md`.
