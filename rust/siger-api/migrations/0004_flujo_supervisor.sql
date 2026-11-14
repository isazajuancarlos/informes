-- Flujo del supervisor (#163): notificaciones internas, hilo de comentarios por
-- entrega y estado de la justificación de un incumplimiento.
--
-- El sistema anterior tenía `notificaciones_app` (3924 filas) y
-- `comentarios_entrega`; se portan con la misma intención. La mensajería es
-- INTERNA (in-app) y, si el correo está configurado, se envía además una copia.

CREATE TABLE IF NOT EXISTS notificaciones (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    usuario_id  INTEGER NOT NULL REFERENCES usuarios(id) ON DELETE CASCADE,
    tipo        TEXT    NOT NULL,
    titulo      TEXT    NOT NULL,
    mensaje     TEXT    NOT NULL,
    -- A qué entrega se refiere, para poder abrirla desde la notificación.
    entrega_id  INTEGER REFERENCES entregas(id) ON DELETE CASCADE,
    leida       INTEGER NOT NULL DEFAULT 0,
    momento     TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_notif_usuario ON notificaciones (usuario_id, leida);

-- El hilo de una entrega: la justificación que escribe el operador y la
-- retroalimentación del supervisor son comentarios del mismo hilo, con su autor.
CREATE TABLE IF NOT EXISTS comentarios (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    entrega_id  INTEGER NOT NULL REFERENCES entregas(id) ON DELETE CASCADE,
    autor_id    INTEGER REFERENCES usuarios(id),
    texto       TEXT    NOT NULL,
    momento     TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_coment_entrega ON comentarios (entrega_id);

-- Estado de la justificación de un incumplimiento. NULL = sin justificar;
-- PENDIENTE = el operador justificó y espera revisión; ACEPTADA / RECHAZADA = el
-- supervisor ya revisó. Es un CHECK cerrado a esos tres valores explícitos.
ALTER TABLE entregas ADD COLUMN justificacion_estado TEXT
    CHECK (justificacion_estado IN ('PENDIENTE', 'ACEPTADA', 'RECHAZADA'));
