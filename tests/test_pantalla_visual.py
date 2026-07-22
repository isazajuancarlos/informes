"""Las pantallas MEDIDAS en un navegador de verdad.

`test_asistente_pantalla.py` construye los elementos y los pulsa, pero no
calcula CSS: para él una columna con `display:none` existe igual. Y ahí vivía un
fallo que llevaba desde el primer commit —el panel de marca del login no se
dibujaba nunca y media pantalla quedaba en negro— que ninguna de las 600 pruebas
podía ver, porque solo se manifiesta cuando alguien aplica las hojas de estilo.

Estas pruebas levantan la aplicación, la abren con Chromium y **miden**:
posición, ancho y `display` calculados. Es lento y por eso son pocas — las
imprescindibles para que nadie abra el producto y se encuentre media pantalla
vacía.

Se **omiten** si no hay navegador instalado, para no obligar a todo el mundo a
descargar 114 MB. En el CI sí está, y allí no se omiten nunca:

    pip install playwright && playwright install chromium
"""
from __future__ import annotations

import os
import socket
import subprocess
import sys
import time

import pytest

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

playwright = pytest.importorskip("playwright.sync_api",
                                 reason="playwright no está instalado")

#: Corte de Tailwind a partir del cual el diseño es de dos columnas.
ANCHO_LG = 1024


def _puerto_libre() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


@pytest.fixture(scope="module")
def servidor():
    """La aplicación de verdad, en un puerto propio y con base en blanco.

    No se reutiliza la instancia de desarrollo: una prueba que depende de que
    alguien haya dejado algo corriendo pasa o falla por el motivo equivocado.
    """
    puerto = _puerto_libre()
    # `PYTEST_CURRENT_TEST` se hereda al hijo, y NiceGUI lo lee para decidir que
    # está «bajo pytest»: entonces ignora el puerto y exige otra variable que
    # solo pone su propio complemento de pruebas. El proceso moría con un
    # KeyError que no se veía porque la salida iba a /dev/null.
    #
    # El entorno de la herramienta de pruebas no debe filtrarse a lo que se está
    # probando: aquí se levanta la aplicación como la levanta un cliente.
    entorno = {k: v for k, v in os.environ.items()
               if not k.startswith(("PYTEST_", "NICEGUI_"))}
    entorno |= {
               "DATABASE_URL": f"sqlite:///./test_visual_{puerto}.db",
               "DEPLOYMENT_PROFILE": "standalone",
               "PORT": str(puerto),
               # Sin recargador: uvicorn se reinicia solo al detectar cambios y
               # en una prueba eso es una fuente de fallos intermitentes.
               "RELOAD": "false",
               "PYTHONDONTWRITEBYTECODE": "1"}
    # La salida se GUARDA. Mandarla a /dev/null convierte «no arrancó» en un
    # fallo sin causa, que es el peor tipo de prueba roja.
    registro = open(os.path.join(RAIZ, f"test_visual_{puerto}.log"), "w+")
    proc = subprocess.Popen([sys.executable, "main.py"], cwd=RAIZ, env=entorno,
                            stdout=registro, stderr=subprocess.STDOUT)
    base = f"http://127.0.0.1:{puerto}"
    for _ in range(120):
        if proc.poll() is not None:
            break
        try:
            with socket.create_connection(("127.0.0.1", puerto), timeout=0.5):
                break
        except OSError:
            time.sleep(0.5)
    else:
        proc.terminate()
        registro.seek(0)
        pytest.fail("la aplicación no arrancó:\n" + registro.read()[-2000:])
    if proc.poll() is not None:
        registro.seek(0)
        pytest.fail("la aplicación se cerró sola:\n" + registro.read()[-2000:])
    yield base
    proc.terminate()
    proc.wait(timeout=20)
    registro.close()
    for sobra in (f"test_visual_{puerto}.db", f"test_visual_{puerto}.log"):
        try:
            os.remove(os.path.join(RAIZ, sobra))
        except OSError:
            pass


@pytest.fixture(scope="module")
def navegador():
    """Chromium, y una omisión que NO MIENTE.

    Aquí había un `except Exception` que decía «no hay Chromium instalado» ante
    cualquier fallo. Al correr la suite entera Chromium SÍ estaba, y el mensaje
    era falso: lo que ocurre es que la API síncrona de Playwright no arranca
    dentro de una corrida que ya tiene un bucle de eventos en marcha, y las
    pruebas del simulador de NiceGUI lo dejan puesto.

    Una omisión que miente es peor que un fallo. El fallo se arregla; la omisión
    se lee de pasada y se ignora — y entonces el banco entero deja de correr sin
    que nadie se entere, que es exactamente cómo el login llevaba meses roto.

    Ahora se distinguen las dos causas y cada una se dice con su nombre. En el
    trabajo `visual` del CI este fichero corre AISLADO, así que allí no se omite
    nunca y las comprobaciones se ejecutan de verdad.
    """
    from playwright.sync_api import sync_playwright

    with sync_playwright() as p:
        try:
            nav = p.chromium.launch()
        except Exception as e:  # noqa: BLE001
            texto = str(e).lower()
            if "executable doesn't exist" in texto or "playwright install" in texto:
                pytest.skip("Chromium no está instalado: "
                            "python -m playwright install chromium")
            pytest.skip(
                "El navegador síncrono no arranca dentro de una corrida que ya "
                f"tiene un bucle asíncrono en marcha ({e}). No es que falte el "
                "navegador. Estas comprobaciones corren aisladas:  "
                "pytest tests/test_pantalla_visual.py")
        yield nav
        nav.close()


def _columnas(pagina):
    return pagina.evaluate("""() => {
        const fila = document.querySelector('.nicegui-row');
        if (!fila) return null;
        return [...fila.children].map(c => {
            const r = c.getBoundingClientRect();
            return {x: Math.round(r.x), y: Math.round(r.y),
                    w: Math.round(r.width),
                    display: getComputedStyle(c).display};
        });
    }""")


def _abrir(navegador, base, ancho):
    pagina = navegador.new_page(viewport={"width": ancho, "height": 900})
    pagina.goto(f"{base}/login", wait_until="networkidle")
    pagina.wait_for_timeout(1200)
    return pagina


def test_el_login_ocupa_la_pantalla_ENTERA_en_dos_columnas(navegador, servidor):
    """El fallo original: media pantalla en negro.

    La marca se mostraba con `hidden lg:flex`, y NiceGUI compila Tailwind en el
    navegador sin garantizar el orden entre `.hidden` y `.lg\\:flex` — ganaba
    `hidden` y el panel no aparecía jamás. Encima, la fila traía 16 px de
    separación, así que dos columnas al 50 % no cabían y la segunda bajaba.

    Se miden las dos cosas: que hay dos columnas, que están **a la misma
    altura** —si una baja, es que no cabían— y que entre las dos cubren el ancho.
    """
    pagina = _abrir(navegador, servidor, 1440)
    try:
        columnas = _columnas(pagina)
        assert columnas and len(columnas) == 2, columnas
        marca, formulario = columnas

        assert marca["display"] != "none", "el panel de marca no se dibuja"
        assert marca["y"] == formulario["y"] == 0, \
            f"una columna bajó: no caben. {columnas}"
        assert marca["x"] == 0, f"el diseño no llega al borde: {columnas}"
        assert marca["w"] + formulario["w"] == 1440, \
            f"las dos columnas no cubren la pantalla: {columnas}"
    finally:
        pagina.close()


def test_el_nombre_del_sistema_se_VE(navegador, servidor):
    """Estaba escrito desde el primer commit y no lo veía nadie."""
    pagina = _abrir(navegador, servidor, 1440)
    try:
        veces = pagina.evaluate("""() => {
            let n = 0;
            document.querySelectorAll('*').forEach(e => {
                if (e.children.length === 0 && e.innerText
                    && e.innerText.trim() === 'SIGER'
                    && e.getBoundingClientRect().width > 0) n++;
            });
            return n;
        }""")
        # Una vez: ni cero —el fallo original— ni dos, que sería la cabecera
        # para pantalla estrecha saliendo a la vez que el panel de marca.
        assert veces == 1, f"el nombre aparece {veces} veces"
    finally:
        pagina.close()


@pytest.mark.parametrize("ancho", [1440, ANCHO_LG, 800, 480])
def test_el_boton_de_entrar_se_ve_en_cualquier_pantalla(navegador, servidor, ancho):
    """Lo único imprescindible de un login. Si desaparece a algún ancho, el
    producto no se puede usar en esa pantalla y nadie lo sabría."""
    pagina = _abrir(navegador, servidor, ancho)
    try:
        caja = pagina.evaluate("""() => {
            const b = [...document.querySelectorAll('button')].find(
                x => x.innerText.includes('Ingresar'));
            if (!b) return null;
            const r = b.getBoundingClientRect();
            return {x: Math.round(r.x), y: Math.round(r.y),
                    w: Math.round(r.width), h: Math.round(r.height)};
        }""")
        assert caja, f"no hay botón «Ingresar» a {ancho}px"
        assert caja["w"] > 0 and caja["h"] > 0, caja
        assert 0 <= caja["x"] and caja["x"] + caja["w"] <= ancho, \
            f"el botón se sale de la pantalla a {ancho}px: {caja}"
        assert caja["y"] < 900, f"el botón cae bajo el pliegue a {ancho}px: {caja}"
    finally:
        pagina.close()


def test_por_debajo_del_corte_la_marca_se_OCULTA_y_el_formulario_se_ensancha(
        navegador, servidor):
    """El contrapunto: la prueba no puede pasar por enseñarlo todo siempre.

    En pantalla estrecha el panel de marca sobra —el nombre se enseña dentro de
    la tarjeta— y el formulario tiene que ocupar el ancho completo.
    """
    pagina = _abrir(navegador, servidor, 800)
    try:
        marca, formulario = _columnas(pagina)
        assert marca["display"] == "none", marca
        assert formulario["w"] == 800, formulario
    finally:
        pagina.close()
