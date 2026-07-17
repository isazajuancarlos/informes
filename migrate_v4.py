"""
Migración segura v3 → v4: agrega tablas y columnas nuevas sin destruir datos.

Uso:
    python migrate_v4.py

Nota: ejecuta init_db() primero para garantizar que las tablas base existen,
luego corre cada sentencia SQL en su propia transacción para que un fallo
no aborte las sentencias siguientes.
"""
import sys
import os
sys.path.append(os.path.dirname(os.path.abspath(__file__)))

from app.core.database import engine
from sqlalchemy import text


MIGRACIONES = [
    # Nuevas tablas independientes (sin FK externas)
    """CREATE TABLE IF NOT EXISTS configuracion_sistema (
        clave VARCHAR(100) PRIMARY KEY,
        valor TEXT,
        descripcion VARCHAR(300)
    )""",
    """CREATE TABLE IF NOT EXISTS unidades_organizacionales (
        id SERIAL PRIMARY KEY,
        nombre VARCHAR(200) NOT NULL,
        codigo VARCHAR(50) UNIQUE,
        descripcion TEXT,
        padre_id INTEGER REFERENCES unidades_organizacionales(id),
        nivel INTEGER DEFAULT 1,
        activo BOOLEAN DEFAULT TRUE
    )""",
    # Tablas que dependen de usuarios (ya existe tras init_db)
    """CREATE TABLE IF NOT EXISTS delegaciones (
        id SERIAL PRIMARY KEY,
        delegante_id INTEGER NOT NULL REFERENCES usuarios(id),
        delegado_id INTEGER NOT NULL REFERENCES usuarios(id),
        fecha_inicio TIMESTAMP NOT NULL,
        fecha_fin TIMESTAMP NOT NULL,
        motivo TEXT,
        activo BOOLEAN DEFAULT TRUE
    )""",
    """CREATE TABLE IF NOT EXISTS notificaciones_app (
        id SERIAL PRIMARY KEY,
        usuario_id INTEGER NOT NULL REFERENCES usuarios(id),
        tipo VARCHAR(20) NOT NULL,
        titulo VARCHAR(200) NOT NULL,
        mensaje TEXT NOT NULL,
        leida BOOLEAN DEFAULT FALSE,
        fecha_creacion TIMESTAMP DEFAULT NOW(),
        url_destino VARCHAR(500),
        entidad_tipo VARCHAR(50),
        entidad_id INTEGER
    )""",
    # Nuevas columnas en tablas existentes
    "ALTER TABLE usuarios ADD COLUMN IF NOT EXISTS unidad_id INTEGER REFERENCES unidades_organizacionales(id)",
    "ALTER TABLE cat_informes ADD COLUMN IF NOT EXISTS unidad_id INTEGER REFERENCES unidades_organizacionales(id)",
    # Índices de rendimiento
    "CREATE INDEX IF NOT EXISTS idx_notif_usuario_leida ON notificaciones_app(usuario_id, leida)",
    "CREATE INDEX IF NOT EXISTS idx_delegaciones_activo ON delegaciones(delegante_id, activo, fecha_inicio, fecha_fin)",
    "CREATE INDEX IF NOT EXISTS idx_notif_fecha ON notificaciones_app(usuario_id, fecha_creacion DESC)",
]


if __name__ == "__main__":
    # Step 1: ensure all base tables exist (usuarios, cat_informes, etc.)
    print("🔄 Paso 1: Inicializando tablas base...")
    from app.init_db import init_db
    init_db()
    print("  ✅ Tablas base listas")

    # Step 2: run each v4 migration in its own transaction
    print(f"\n🔄 Paso 2: Aplicando {len(MIGRACIONES)} migraciones v4...")
    ok = 0
    errors = []

    for i, sql in enumerate(MIGRACIONES, 1):
        label = sql.strip().split('\n')[0][:80]
        # Use engine.begin() so each statement gets its own transaction.
        # If it fails, that transaction is rolled back and the next statement
        # starts fresh — no cascading "aborted transaction" errors.
        try:
            with engine.begin() as conn:
                conn.execute(text(sql))
            print(f"  ✅ [{i}/{len(MIGRACIONES)}] {label}")
            ok += 1
        except Exception as e:
            short = str(e).split('\n')[0][:120]
            print(f"  ⚠️  [{i}/{len(MIGRACIONES)}] {label}")
            print(f"       → {short}")
            errors.append((i, short))

    print()
    if errors:
        print(f"⚠️  Migración completada: {ok} OK, {len(errors)} advertencias")
        for idx, msg in errors:
            print(f"   [{idx}] {msg}")
    else:
        print(f"✅ Migración v4 completada exitosamente ({ok}/{len(MIGRACIONES)})")
