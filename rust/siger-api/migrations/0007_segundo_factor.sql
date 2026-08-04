-- El segundo factor: TOTP y sus códigos de recuperación (#212).
--
-- El prerrequisito ya estaba puesto en 0006: sin el freno al ensayo, seis
-- dígitos con tolerancia de tres ventanas se prueban hasta acertar y el segundo
-- factor no añade nada.

-- El secreto compartido, CIFRADO. En claro es exactamente igual de útil para
-- quien copie el archivo que para su dueño: con él se generan los códigos, así
-- que un secreto en claro convierte el segundo factor en un adorno frente al
-- único atacante que importa aquí — el que ya se llevó la base.
--
-- Se envuelve con `guaca::reposo` (Argon2id -> AEAD -> padding Padmé), el mismo
-- camino que las entregas de #128, y por tanto **solo hay 2FA si hay
-- SIGER_CLAVE_REPOSO**. Eso no es una limitación que se descubra tarde: el alta
-- del segundo factor lo dice y se niega a empezar, en vez de guardar el secreto
-- en claro «mientras tanto».
ALTER TABLE usuarios ADD COLUMN totp_secreto_cifrado BLOB;

-- Cuándo se ACTIVÓ, en RFC 3339 como el resto de la base. NULL significa «no
-- tiene segundo factor», y es lo único que decide si el login pide código.
--
-- Es una columna aparte del secreto a propósito: entre generar el secreto y
-- comprobar que la persona sabe leerlo hay un paso —teclear un código— y en ese
-- intervalo el secreto EXISTE pero el factor NO está activo. Con una sola
-- columna, quien cerrara la pantalla del QR a medias se quedaría fuera de su
-- cuenta sin haber llegado a enrolar nada.
ALTER TABLE usuarios ADD COLUMN totp_confirmado_en TEXT;

-- El último contador de 30 s que se dio por bueno. Es TODO el antirreplay: un
-- código capturado y reenviado cae en un contador que ya no avanza.
--
-- NULL = todavía no se usó ninguno, que no es lo mismo que 0: con 0 el primer
-- código válido de la historia (contador ~59 millones) entraría igual, pero la
-- distinción importa el día que alguien lea la columna para saber si la cuenta
-- llegó a usarse.
ALTER TABLE usuarios ADD COLUMN totp_ultimo_contador INTEGER;

-- Los códigos de recuperación, HASHEADOS con el mismo Argon2 que las
-- contraseñas. En claro serían una contraseña maestra escrita en la puerta: diez
-- cadenas que saltan el segundo factor, guardadas junto a lo que protegen.
--
-- Uno por fila y no una lista en una columna, porque cada uno se gasta por
-- separado: `usado_en` no nulo es un código consumido, y borrar la fila perdería
-- la única prueba de que alguien entró por ahí.
CREATE TABLE IF NOT EXISTS codigo_recuperacion (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    usuario_id INTEGER NOT NULL REFERENCES usuarios(id) ON DELETE CASCADE,
    hash       TEXT    NOT NULL,
    creado_en  TEXT    NOT NULL,
    usado_en   TEXT
);

-- La consulta es siempre «los códigos SIN gastar de este usuario».
CREATE INDEX IF NOT EXISTS idx_codigo_recuperacion_usuario
    ON codigo_recuperacion (usuario_id, usado_en);
