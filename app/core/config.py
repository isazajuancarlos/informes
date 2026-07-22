"""
Configuration module using environment variables
"""
import os
from pathlib import Path
from typing import Optional

# Try to load .env file if python-dotenv is available
try:
    from dotenv import load_dotenv
    load_dotenv()
except ImportError:
    print("⚠️ python-dotenv not installed. Using system environment variables only.")

# Base paths
BASE_DIR = Path(__file__).resolve().parent.parent.parent
STORAGE_DIR = BASE_DIR / "storage"
UPLOADS_DIR = STORAGE_DIR / "uploads"
BACKUPS_DIR = STORAGE_DIR / "backups"

# --- Cómo se llama el producto ------------------------------------------------
#
# UN solo sitio. Estaba en cinco, y decían cinco cosas distintas: la pantalla de
# entrada «SIGER», la cabecera de todas las demás «SISTEMA INTEGRAL», la pestaña
# del navegador «Sistema Gestión», los correos «Sistema de Gestión» y el
# clausulado «Sistema de Gestión de Informes». Nadie lo había juntado nunca en
# la misma frase, así que nadie lo había visto.
#
# Configurable a propósito: un cliente que lo instala en su casa puede querer su
# propio nombre, y obligarle a tocar el código para eso convertiría una decisión
# de negocio en una modificación del programa.
NOMBRE_PRODUCTO = os.getenv("NOMBRE_PRODUCTO", "SIGER").strip() or "SIGER"
NOMBRE_LARGO = os.getenv(
    "NOMBRE_LARGO", "Sistema Integral de Gestión de Informes").strip()


# --- Perfil de despliegue -----------------------------------------------------
# cloud     : SaaS que hospedamos nosotros (PostgreSQL, con internet).
# onprem    : el cliente lo corre en su propio servidor/LAN (su PostgreSQL, sin
#             internet). Aísla sus datos en su máquina por diseño.
# standalone: una sola máquina, sin administrar base de datos (SQLite), offline.
# El mismo código sirve a los tres; solo cambia la configuración.
DEPLOYMENT_PROFILE = os.getenv("DEPLOYMENT_PROFILE", "cloud").strip().lower()
if DEPLOYMENT_PROFILE not in ("cloud", "onprem", "standalone"):
    print(f"⚠️ DEPLOYMENT_PROFILE '{DEPLOYMENT_PROFILE}' no válido; usando 'cloud'.")
    DEPLOYMENT_PROFILE = "cloud"

# Database Configuration
# DATABASE_URL explícito siempre gana; si no, el default depende del perfil:
# standalone -> SQLite en storage/informes.db; los demás -> PostgreSQL.
_DEFAULT_DB = (
    f"sqlite:///{STORAGE_DIR / 'informes.db'}"
    if DEPLOYMENT_PROFILE == "standalone"
    else "postgresql://postgres:admin@localhost/informes"
)
DATABASE_URL = os.getenv("DATABASE_URL", _DEFAULT_DB)
IS_SQLITE = DATABASE_URL.startswith("sqlite")

# Modo offline: no asumir acceso a internet (afecta a servicios externos como el
# futuro asistente IA; el email ya se gobierna aparte por EMAIL_ENABLED). onprem
# y standalone son offline por defecto; cloud, online. Override con OFFLINE=.
_OFFLINE_DEFAULT = DEPLOYMENT_PROFILE in ("onprem", "standalone")
OFFLINE_MODE = os.getenv(
    "OFFLINE", str(_OFFLINE_DEFAULT)
).strip().lower() in ("true", "1", "yes", "on")

# Email Configuration
SMTP_HOST = os.getenv("SMTP_HOST", "smtp.gmail.com")
SMTP_PORT = int(os.getenv("SMTP_PORT", "587"))
SENDER_EMAIL = os.getenv("SENDER_EMAIL", "")
SENDER_PASSWORD = os.getenv("SENDER_PASSWORD", "")
EMAIL_ENABLED = bool(SENDER_EMAIL and SENDER_PASSWORD)

# Security Configuration
STORAGE_SECRET = os.getenv("STORAGE_SECRET", "SECRET_KEY_FINAL_V6")
SESSION_TIMEOUT_MINUTES = int(os.getenv("SESSION_TIMEOUT_MINUTES", "60"))

# Scheduler Configuration
ENABLE_AUTO_SCHEDULER = os.getenv("ENABLE_AUTO_SCHEDULER", "true").lower() == "true"
SCHEDULER_HOUR = int(os.getenv("SCHEDULER_HOUR", "0"))
SCHEDULER_MINUTE = int(os.getenv("SCHEDULER_MINUTE", "0"))

# Retroactivity Configuration
MAX_RETROACTIVE_DAYS = int(os.getenv("MAX_RETROACTIVE_DAYS", "730"))

# Internationalization & Regional
PAIS_ISO = os.getenv("PAIS_ISO", "CO")            # ISO 3166-1 alpha-2
TIMEZONE = os.getenv("TIMEZONE", "America/Bogota") # IANA timezone
IDIOMA_DEFECTO = os.getenv("IDIOMA_DEFECTO", "es") # es | en

# File Upload Configuration
MAX_FILE_SIZE_MB = int(os.getenv("MAX_FILE_SIZE_MB", "50"))
ALLOWED_EXTENSIONS = os.getenv(
    "ALLOWED_EXTENSIONS",
    ".pdf,.xlsx,.docx,.zip"
).split(",")

# Backup Configuration
BACKUP_PATH = os.getenv("BACKUP_PATH", str(BACKUPS_DIR))

# Application Base URL (used in email links)
APP_BASE_URL = os.getenv("APP_BASE_URL", "http://localhost:8003")

# Parse database URL for backup service
def get_db_credentials():
    """Extract database credentials from DATABASE_URL"""
    if DATABASE_URL.startswith("postgresql://"):
        # postgresql://user:password@host:port/database
        parts = DATABASE_URL.replace("postgresql://", "").split("@")
        if len(parts) == 2:
            user_pass = parts[0].split(":")
            host_db = parts[1].split("/")

            return {
                "user": user_pass[0] if len(user_pass) > 0 else "postgres",
                "password": user_pass[1] if len(user_pass) > 1 else "",
                "host": host_db[0].split(":")[0] if len(host_db) > 0 else "localhost",
                "database": host_db[1] if len(host_db) > 1 else "informes"
            }

    # Default fallback
    return {
        "user": "postgres",
        "password": "admin",
        "host": "localhost",
        "database": "informes"
    }

# Validation warnings
def validate_config():
    """Print warnings for missing critical configuration"""
    motor = "SQLite" if IS_SQLITE else "PostgreSQL"
    warnings = [f"🚀 Perfil de despliegue: {DEPLOYMENT_PROFILE} "
                f"({motor}, {'offline' if OFFLINE_MODE else 'online'})"]

    if not EMAIL_ENABLED and not OFFLINE_MODE:
        # En offline el email no se espera; no es una carencia.
        warnings.append("⚠️ Email not configured. Set SENDER_EMAIL and SENDER_PASSWORD in .env")

    if STORAGE_SECRET == "SECRET_KEY_FINAL_V6":
        warnings.append("⚠️ Using default STORAGE_SECRET. Change it in production!")

    # La alerta de contraseña por defecto solo aplica a PostgreSQL.
    if not IS_SQLITE and "admin" in DATABASE_URL:
        warnings.append("⚠️ Using default database password. Change it in production!")

    return warnings

# Print warnings on import
if __name__ != "__main__":
    for warning in validate_config():
        print(warning)
