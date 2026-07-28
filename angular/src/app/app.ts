import { Component, computed, inject, signal } from '@angular/core';
import { NavigationEnd, Router, RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';
import { MatListModule } from '@angular/material/list';
import { MatMenuModule } from '@angular/material/menu';
import { MatSidenavModule } from '@angular/material/sidenav';
import { MatToolbarModule } from '@angular/material/toolbar';
import { filter } from 'rxjs/operators';

import { Sesion } from './nucleo/sesion';

interface Entrada {
  ruta: string;
  titulo: string;
  icono: string;
}

@Component({
  selector: 'app-root',
  imports: [
    RouterOutlet,
    RouterLink,
    RouterLinkActive,
    MatButtonModule,
    MatIconModule,
    MatListModule,
    MatMenuModule,
    MatSidenavModule,
    MatToolbarModule,
  ],
  templateUrl: './app.html',
  styleUrl: './app.scss',
})
export class App {
  sesion = inject(Sesion);
  private router = inject(Router);

  readonly menu: Entrada[] = [
    { ruta: '/entregas', titulo: 'Entregas', icono: 'assignment_turned_in' },
    { ruta: '/notificaciones', titulo: 'Notificaciones', icono: 'notifications' },
    { ruta: '/estadisticas', titulo: 'Estadísticas', icono: 'insights' },
    { ruta: '/informes', titulo: 'Informes', icono: 'description' },
    { ruta: '/cronograma', titulo: 'Cronograma', icono: 'event_repeat' },
  ];

  /** El lateral arranca abierto solo si hay sitio. En un portátil de 13" abierto
   *  por defecto deja la tabla de entregas sin ancho para sus seis columnas. */
  readonly anchaLaPantalla = signal(window.innerWidth >= 1100);

  private readonly ruta = signal(this.router.url);
  readonly tituloActual = computed(
    () => this.menu.find((e) => this.ruta().startsWith(e.ruta))?.titulo ?? 'SIGER',
  );

  constructor() {
    this.router.events
      .pipe(filter((e): e is NavigationEnd => e instanceof NavigationEnd))
      .subscribe((e) => this.ruta.set(e.urlAfterRedirects));
  }
}
