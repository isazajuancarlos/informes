import os
import shutil
import sqlite3
import subprocess
from datetime import datetime
import glob
from app.core.config import get_db_credentials, BACKUP_PATH, DATABASE_URL, IS_SQLITE

class BackupService:
    # Configuración desde variables de entorno
    _db_creds = get_db_credentials()
    DB_NAME = _db_creds["database"]
    DB_USER = _db_creds["user"]
    DB_PASS = _db_creds["password"]
    DB_HOST = _db_creds["host"]
    DB_PORT = "5432"
    BACKUP_DIR = BACKUP_PATH

    def __init__(self):
        if not os.path.exists(self.BACKUP_DIR):
            os.makedirs(self.BACKUP_DIR)

    def _get_env(self):
        """Prepara el entorno con la contraseña para que pg_dump no la pida"""
        env = os.environ.copy()
        env["PGPASSWORD"] = self.DB_PASS
        return env

    @staticmethod
    def _ruta_sqlite() -> str:
        """El archivo .db que hay detrás de un DATABASE_URL de SQLite."""
        return DATABASE_URL.split("sqlite:///", 1)[-1]

    def _copia_sqlite(self, filepath) -> tuple[bool, str]:
        """Respaldo EN CALIENTE con la API de backup de sqlite3.

        NO se copia el archivo con `cp`. Una base SQLite viva puede estar en
        mitad de una transacción, y una copia byte a byte capturaría un estado
        a medias: un respaldo que parece bueno y no restaura. La API `backup()`
        toma una instantánea consistente aunque haya escrituras en curso, y es
        el motivo por el que este método existe en vez de documentar «copie el
        volumen» (que era el pendiente de DESPLIEGUE.md).
        """
        origen = self._ruta_sqlite()
        if not os.path.isfile(origen):
            return False, f"No se encontró la base SQLite en {origen}"
        try:
            with sqlite3.connect(origen) as src, sqlite3.connect(filepath) as dst:
                src.backup(dst)
            return True, os.path.basename(filepath)
        except sqlite3.Error as e:
            # Si la copia salió a medias, se borra: dejar un archivo corrupto
            # en la carpeta de respaldos es peor que no dejar ninguno, porque
            # alguien confiará en él el día que lo necesite.
            if os.path.exists(filepath):
                os.remove(filepath)
            return False, f"Error de SQLite al respaldar: {e}"

    def crear_copia(self, automatico=False):
        """Genera una copia completa de la base de datos.

        En PostgreSQL produce un `.sql` con `pg_dump`; en SQLite (perfil
        `standalone`) produce un `.db` con la API de respaldo en caliente.
        """
        prefix = "AUTO" if automatico else "MANUAL"
        timestamp = datetime.now().strftime("%Y-%m-%d_%H-%M-%S")
        extension = "db" if IS_SQLITE else "sql"
        filename = f"backup_{prefix}_{timestamp}.{extension}"
        filepath = os.path.join(self.BACKUP_DIR, filename)

        if IS_SQLITE:
            return self._copia_sqlite(filepath)

        # Comando pg_dump (Exportar)
        # --clean: Incluye comandos DROP antes de CREATE (vital para restaurar)
        # --if-exists: Evita errores si no existe tabla al borrar
        cmd = [
            "pg_dump",
            "-h", self.DB_HOST,
            "-p", self.DB_PORT,
            "-U", self.DB_USER,
            "--clean",
            "--if-exists",
            "-F", "p", # Formato plano (texto SQL)
            "-f", filepath,
            self.DB_NAME
        ]

        try:
            subprocess.run(cmd, env=self._get_env(), check=True)
            return True, filename
        except subprocess.CalledProcessError as e:
            return False, str(e)
        except FileNotFoundError:
            return False, "No se encontró 'pg_dump'. Instala PostgreSQL Client en el sistema."

    def restaurar_copia(self, filename):
        """Restaura la base de datos desde un archivo"""
        filepath = os.path.join(self.BACKUP_DIR, filename)
        
        if not os.path.exists(filepath):
            return False, "El archivo no existe"

        if IS_SQLITE:
            destino = self._ruta_sqlite()
            # Antes de sobrescribir la base viva se guarda la que había. Una
            # restauración es justo el momento en que alguien está nervioso y
            # puede equivocarse de archivo; sin esta red, ese error no tiene
            # vuelta atrás.
            if os.path.isfile(destino):
                previa = f"{destino}.previa-{datetime.now():%Y%m%d-%H%M%S}"
                shutil.copy2(destino, previa)
            try:
                with sqlite3.connect(filepath) as src, sqlite3.connect(destino) as dst:
                    src.backup(dst)
                return True, ("Restauración exitosa. Reinicie la aplicación: "
                              "las conexiones abiertas siguen viendo la base anterior.")
            except sqlite3.Error as e:
                return False, f"Error de SQLite al restaurar: {e}"

        # Comando psql (Importar)
        cmd = [
            "psql",
            "-h", self.DB_HOST,
            "-p", self.DB_PORT,
            "-U", self.DB_USER,
            "-d", self.DB_NAME,
            "-f", filepath
        ]

        try:
            subprocess.run(cmd, env=self._get_env(), check=True)
            return True, "Restauración exitosa"
        except subprocess.CalledProcessError as e:
            return False, f"Error al restaurar: {e}"

    def listar_copias(self):
        """Devuelve lista de archivos ordenados por fecha"""
        # Ambas extensiones: un mismo directorio puede tener copias de antes y
        # después de migrar de motor, y ocultar las que no son del motor actual
        # haría desaparecer respaldos válidos de la vista.
        files = (glob.glob(os.path.join(self.BACKUP_DIR, "*.sql"))
                 + glob.glob(os.path.join(self.BACKUP_DIR, "*.db")))
        files.sort(key=os.path.getmtime, reverse=True) # Más recientes primero
        
        lista = []
        for f in files:
            stats = os.stat(f)
            size_mb = stats.st_size / (1024 * 1024)
            name = os.path.basename(f)
            fecha = datetime.fromtimestamp(stats.st_mtime).strftime("%Y-%m-%d %H:%M:%S")
            tipo = "AUTOMÁTICA" if "AUTO" in name else "MANUAL"
            
            lista.append({
                "nombre": name,
                "fecha": fecha,
                "tipo": tipo,
                "tamano": f"{size_mb:.2f} MB",
                "path": f
            })
        return lista

    def eliminar_copia(self, filename):
        try:
            os.remove(os.path.join(self.BACKUP_DIR, filename))
            return True
        except:
            return False