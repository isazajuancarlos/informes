"""Configuración de pytest para todo el repositorio.

Existe por una razón concreta: **poder dibujar la interfaz en una prueba**.

NiceGUI trae un simulador de cliente (`user`) que construye los elementos de
verdad y permite pulsarlos, sin navegador. Sin él, todo lo que se comprueba de
una pantalla es que el módulo importa — y un `ui.date(...).props('multiple')`
mal escrito, o un manejador que revienta al pulsar, importa perfectamente y
falla en cuanto alguien lo abre.

El plugin tiene que declararse aquí y no en `tests/`: pytest ya no admite
`pytest_plugins` fuera del directorio raíz.
"""
import os

pytest_plugins = ["nicegui.testing.user_plugin"]

# Las pruebas nunca deben tocar la base de datos de nadie. Se fija antes de que
# se importe cualquier módulo de la aplicación, porque `app.core.database` lee
# esto al importarse y después ya es tarde.
#
# EN MEMORIA, que es exactamente lo que usa el CI. Con un fichero, la base
# sobrevive entre corridas y se queda con el esquema de ayer: al añadir una
# columna, las pruebas fallaban con «no such column» —un fallo real de
# actualización, pero disfrazado de fallo del código nuevo, y que en el CI no
# aparecía—. Que el entorno de aquí y el del CI no coincidan ya costó una
# corrida entera hoy.
#
# La actualización de una base ANTIGUA sí se comprueba, y donde corresponde: el
# trabajo `migracion` del CI aplica `migrar_esquema.py` sobre una base vieja.
os.environ.setdefault("DATABASE_URL", "sqlite:///:memory:")
os.environ.setdefault("DEPLOYMENT_PROFILE", "standalone")
