-- Esquema inicial. Va DENTRO del binario (`sqlx::migrate!`), y se aplica sola al
-- arrancar.
--
-- No es comodidad: en el sistema anterior el esquema y la migración eran dos
-- pasos que alguien tenía que recordar en el orden correcto, y el día que se
-- invirtieron el instalador sembró datos sobre tablas que no existían, terminó
-- con código 0 y anunció unas credenciales que no servían. Un binario que migra
-- su propia base no puede equivocarse de orden porque no hay dos pasos.

CREATE TABLE IF NOT EXISTS usuarios (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    nombre          TEXT    NOT NULL,
    email           TEXT    NOT NULL UNIQUE,
    clave_hash      TEXT    NOT NULL,
    rol             TEXT    NOT NULL CHECK (rol IN ('ADMIN','OPERADOR','SUPERVISOR','TERCERO')),
    activo          INTEGER NOT NULL DEFAULT 1,
    stakeholder_id  INTEGER REFERENCES stakeholders(id)
);

CREATE TABLE IF NOT EXISTS stakeholders (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    nombre  TEXT    NOT NULL,
    externo INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS informes (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    nombre          TEXT    NOT NULL UNIQUE,
    descripcion     TEXT,
    -- La regla, en el mismo formato que ya está guardado en la base anterior:
    -- `compat::ReglaJson` la lee y la traduce al entrar. Migrar el formato y
    -- migrar el motor a la vez habría hecho imposible saber cuál de los dos
    -- falló.
    recurrencia     TEXT    NOT NULL DEFAULT '{}',
    activo          INTEGER NOT NULL DEFAULT 1
);

-- Quién participa y con qué papel: la ÚNICA fuente de verdad del flujo. Una
-- parte es interna o externa, nunca las dos, y el CHECK lo garantiza.
CREATE TABLE IF NOT EXISTS participacion (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    informe_id      INTEGER NOT NULL REFERENCES informes(id) ON DELETE CASCADE,
    usuario_id      INTEGER REFERENCES usuarios(id),
    stakeholder_id  INTEGER REFERENCES stakeholders(id),
    rol             TEXT    NOT NULL CHECK (rol IN ('ORIGEN','DESTINO','USUARIO','SUPERVISA','EJECUTA')),
    es_principal    INTEGER NOT NULL DEFAULT 1,
    orden           INTEGER NOT NULL DEFAULT 0,
    CHECK ((usuario_id IS NOT NULL AND stakeholder_id IS NULL)
        OR (usuario_id IS NULL AND stakeholder_id IS NOT NULL))
);

-- Dos índices únicos PARCIALES, no uno compuesto: con una columna en NULL, un
-- UNIQUE no restringe nada porque en SQL NULL != NULL, y dos filas idénticas se
-- consideran distintas.
CREATE UNIQUE INDEX IF NOT EXISTS uq_participacion_interna
    ON participacion (informe_id, usuario_id, rol) WHERE usuario_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS uq_participacion_externa
    ON participacion (informe_id, stakeholder_id, rol) WHERE stakeholder_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS entregas (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    informe_id          INTEGER NOT NULL REFERENCES informes(id) ON DELETE CASCADE,
    usuario_id          INTEGER REFERENCES usuarios(id),

    -- Las dos fechas. `nominal` es la identidad de la entrega y por eso es
    -- parte de la clave única: el informe anual de 2026 vence el 1 de enero, se
    -- entregue cuando se entregue.
    fecha_nominal       TEXT    NOT NULL,
    fecha_limite        TEXT    NOT NULL,
    tipo_ajuste         TEXT    NOT NULL DEFAULT 'NINGUNO'
                        CHECK (tipo_ajuste IN ('NINGUNO','ANTICIPADO','GRACIA','OMITIR')),

    estado              TEXT    NOT NULL DEFAULT 'PENDIENTE'
                        CHECK (estado IN ('PENDIENTE','CUMPLIDO','EXTEMPORANEO','INCUMPLIDO')),
    fecha_entrega_real  TEXT,

    -- El NOMBRE del archivo, no su ruta. La ruta la construye el servidor desde
    -- el id de la entrega, así que ninguna cadena que venga del navegador puede
    -- salir del almacén ni entrar en él.
    archivo_nombre      TEXT,

    -- Firma post-cuántica de los campos auditables. NULL es legítimo: la firma
    -- es opcional y su ausencia nunca impide registrar la entrega.
    firma               TEXT,

    UNIQUE (informe_id, fecha_nominal)
);

CREATE INDEX IF NOT EXISTS ix_entregas_usuario ON entregas (usuario_id);
CREATE INDEX IF NOT EXISTS ix_entregas_limite  ON entregas (fecha_limite);

CREATE TABLE IF NOT EXISTS auditoria (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    momento   TEXT    NOT NULL,
    evento    TEXT    NOT NULL,
    email     TEXT,
    detalle   TEXT
);
