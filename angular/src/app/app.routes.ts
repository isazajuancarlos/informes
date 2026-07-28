import { Routes } from '@angular/router';

import { guardaSesion } from './nucleo/sesion';

/**
 * Cada pantalla se carga cuando se visita (`loadComponent`).
 *
 * En una instalación de una sola máquina no hay latencia de red que ahorrar,
 * pero sí un binario que crece: el paquete inicial es lo que arranca en el
 * portátil viejo de una oficina, y no tiene por qué incluir el asistente de
 * cronograma para enseñar la lista de entregas.
 */
export const routes: Routes = [
  { path: '', pathMatch: 'full', redirectTo: 'entregas' },
  {
    path: 'acceso',
    loadComponent: () => import('./paginas/acceso').then((m) => m.Acceso),
  },
  {
    path: 'entregas',
    canActivate: [guardaSesion],
    loadComponent: () => import('./paginas/entregas').then((m) => m.Entregas),
  },
  {
    path: 'cronograma',
    canActivate: [guardaSesion],
    loadComponent: () => import('./paginas/cronograma').then((m) => m.Cronograma),
  },
  // Cualquier otra cosa va a entregas. La guarda decidirá si hay sesión: no se
  // adivina aquí, para que la respuesta salga de un solo sitio.
  { path: '**', redirectTo: 'entregas' },
];
