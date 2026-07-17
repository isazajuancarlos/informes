import os
import subprocess
from datetime import datetime
import glob
from app.core.config import get_db_credentials, BACKUP_PATH

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

    def crear_copia(self, automatico=False):
        """Genera un archivo .sql con toda la base de datos"""
        prefix = "AUTO" if automatico else "MANUAL"
        timestamp = datetime.now().strftime("%Y-%m-%d_%H-%M-%S")
        filename = f"backup_{prefix}_{timestamp}.sql"
        filepath = os.path.join(self.BACKUP_DIR, filename)

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
        files = glob.glob(os.path.join(self.BACKUP_DIR, "*.sql"))
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