// SPDX-License-Identifier: LicenseRef-Xiliux-Comercial
//
// Los comportamientos de cliente de SIGER, sobre `boi`. Cada uno se marca con un
// `data-*` en el HTML que teje maud; boi los aplica al cargar y tras cada swap de
// htmx.
//
// **Nacen cuando una pantalla los pide** (directiva 26), no por especulación.
// Éste lo pidió la administración de cuentas: quitarle el segundo factor a
// alguien no se deshace.
//
// El de abajo es el MISMO que estrenó chasqui. Se copia y no se reescribe
// porque hoy no hay forma de compartir un comportamiento entre proyectos sin
// publicarlo; cuando el arnés salga a su repositorio, éste va con él
// (directiva 9).

import { registrar } from "./boi.js";

registrar("confirmar-doble", "[data-confirmar]", (el) => {
  const normal = el.textContent;
  const aviso = el.getAttribute("data-confirmar");
  let armado = false;
  let reloj = null;
  el.addEventListener("click", (e) => {
    if (!armado) {
      // Primera pulsación: NO deja que htmx dispare todavía.
      e.preventDefault();
      e.stopPropagation();
      armado = true;
      el.textContent = aviso;
      el.classList.add("armado");
      reloj = setTimeout(() => {
        armado = false;
        el.textContent = normal;
        el.classList.remove("armado");
      }, 5000);
      return;
    }
    // Segunda pulsación dentro de los 5 s: se deja pasar (htmx envía).
    clearTimeout(reloj);
    armado = false;
    el.textContent = normal;
    el.classList.remove("armado");
  });
});

