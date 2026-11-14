-- Alertas por correo (#151.5).
--
-- Dos columnas de seguimiento en `entregas`: el instante en que se avisó de la
-- proximidad y el instante en que se avisó del incumplimiento. NULL = no se ha
-- avisado. Sin ellas, el planificador reenviaría la misma alerta cada día, y una
-- alerta que llega a diario es una alerta que nadie mira. Reproducen las columnas
-- que el sistema anterior ya tenía (`alerta_proximidad_en`,
-- `alerta_incumplimiento_en`).
ALTER TABLE entregas ADD COLUMN alerta_proximidad_en     TEXT;
ALTER TABLE entregas ADD COLUMN alerta_incumplimiento_en TEXT;

-- El correo del stakeholder externo. El esquema inicial no lo llevaba —no hacía
-- falta hasta que hubo a quién avisar—; el sistema anterior sí lo tenía, así que
-- el volcado puede traerlo. Sin esto no se puede alertar al tercero destino.
ALTER TABLE stakeholders ADD COLUMN email TEXT;
