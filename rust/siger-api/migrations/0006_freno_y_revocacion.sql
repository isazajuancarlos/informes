-- El freno al ensayo de contraseñas y la revocación selectiva de sesión (#194).
--
-- Las dos piezas viven en la misma migración porque son la misma puerta: sin
-- freno, un segundo factor es teatro (quien ya tiene la contraseña prueba los
-- 10^6 códigos); sin `token_version`, cambiar la contraseña no echa a quien ya
-- entró con la vieja.

-- Cada intento de acceso, acertado o no. Se guarda el CORREO tecleado y no el
-- `usuario_id`: si solo se anotaran las cuentas que existen, recibir el freno
-- diría que el correo existe — justo lo que el login evita dando el mismo error
-- para todo.
CREATE TABLE IF NOT EXISTS intento_acceso (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    email   TEXT    NOT NULL,
    origen  TEXT    NOT NULL,
    -- RFC 3339, que es el formato que `guaca::freno` sabe leer y el mismo que ya
    -- usa `auditoria`. Dos formatos de fecha en la misma base es cómo se llega a
    -- comparar cadenas que no son comparables.
    momento TEXT    NOT NULL,
    exito   INTEGER NOT NULL CHECK (exito IN (0, 1))
);

-- La consulta del freno es siempre por la pareja completa, ordenada por momento:
-- el índice la cubre entera. Sin él, cada intento de acceso recorre la tabla.
CREATE INDEX IF NOT EXISTS idx_intento_acceso_pareja
    ON intento_acceso (email, origen, momento);

-- Revocación selectiva (M3). El número viaja DENTRO del token firmado y se
-- compara contra la base en cada petición —en la misma relectura del usuario que
-- ya se hacía para el rol—, así que subirlo invalida de golpe todas las sesiones
-- de esa persona sin guardar estado de sesión en el servidor.
--
-- Empieza en 1 y no en 0 como defensa en profundidad: `sesion::verificar` exige
-- los tres campos y rechaza un token que no los traiga, pero si algún día alguien
-- relaja ese parseo, un campo ausente valdría 0 — y un 0 que casa con el defecto
-- de la columna es un token viejo pasando por bueno.
ALTER TABLE usuarios ADD COLUMN token_version INTEGER NOT NULL DEFAULT 1;
