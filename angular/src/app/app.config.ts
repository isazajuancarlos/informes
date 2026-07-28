import { registerLocaleData } from '@angular/common';
import localeEs from '@angular/common/locales/es-CO';
import { provideHttpClient, withFetch } from '@angular/common/http';
import { ApplicationConfig, LOCALE_ID, provideBrowserGlobalErrorListeners } from '@angular/core';
import { provideRouter, withComponentInputBinding } from '@angular/router';

import { routes } from './app.routes';

// Las fechas se muestran en español de Colombia. No es una preferencia estética:
// «02/03» es el 2 de marzo o el 3 de febrero según quién lo lea, y en un sistema
// cuyo objeto son las fechas límite esa ambigüedad es un incumplimiento en
// potencia. Con el locale puesto, el formato largo («02 mar 2026») no admite dos
// lecturas.
registerLocaleData(localeEs);

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes, withComponentInputBinding()),
    // `withFetch` en vez de XHR: menos peso y streaming de respuestas.
    provideHttpClient(withFetch()),
    // Sin `@angular/animations`. Material funciona sin él desde la v19, y el
    // paquete son 60 kB para animar transiciones que en una pantalla de gestión
    // no aportan nada. Menos código enviado a la máquina del cliente y una
    // dependencia menos que actualizar.
    { provide: LOCALE_ID, useValue: 'es-CO' },
  ],
};
